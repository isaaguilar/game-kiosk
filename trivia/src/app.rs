use rand::seq::SliceRandom;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ROUND_SIZE_BIBLE_FUN_FACTS: usize = 20;
const ROUND_SIZE_RIDDLES: usize = 10;
const CACHE_TTL_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriviaItem {
    pub question: String,
    pub answer: String,
    #[serde(skip)]
    pub explanation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CachedTriviaItem {
    question: String,
    answer: String,
    added_at_epoch_secs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriviaSubject {
    Bible,
    FunFacts,
    Riddles,
}

impl TriviaSubject {
    pub fn label(self) -> &'static str {
        match self {
            TriviaSubject::Bible => "Bible",
            TriviaSubject::FunFacts => "Fun Facts",
            TriviaSubject::Riddles => "Riddles",
        }
    }

    pub fn all() -> Vec<TriviaSubject> {
        vec![
            TriviaSubject::Bible,
            TriviaSubject::FunFacts,
            TriviaSubject::Riddles,
        ]
    }

    pub fn supports_explanation(self) -> bool {
        matches!(self, TriviaSubject::Bible | TriviaSubject::FunFacts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriviaRequest {
    pub subject: TriviaSubject,
}

impl TriviaRequest {
    fn for_bible() -> Self {
        Self {
            subject: TriviaSubject::Bible,
        }
    }

    pub fn menu_title(self) -> String {
        match self.subject {
            TriviaSubject::Bible => "Bible".to_string(),
            TriviaSubject::FunFacts => "Fun Facts".to_string(),
            TriviaSubject::Riddles => "Riddles".to_string(),
        }
    }

    pub fn loading_status(self) -> String {
        match self.subject {
            TriviaSubject::Bible => "Loading Bible questions".to_string(),
            TriviaSubject::FunFacts => "Loading Fun Facts".to_string(),
            TriviaSubject::Riddles => "Loading Riddles".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplanationRequest {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerAction {
    ExplainFurther,
    Continue,
}

impl AnswerAction {
    pub fn label(self) -> &'static str {
        match self {
            AnswerAction::ExplainFurther => "Explain further",
            AnswerAction::Continue => "Continue",
        }
    }

    pub fn other(self) -> Self {
        match self {
            AnswerAction::ExplainFurther => AnswerAction::Continue,
            AnswerAction::Continue => AnswerAction::ExplainFurther,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Debug, Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Part {
    text: String,
}

pub type BackgroundLoadResult = (TriviaRequest, Result<Vec<TriviaItem>, String>);
pub type BackgroundExplanationResult = (ExplanationRequest, Result<String, String>);

pub enum PendingBackgroundJob {
    Trivia(TriviaRequest),
    Explanation(ExplanationRequest),
}

pub enum AppState {
    SubjectMenu {
        subjects: Vec<TriviaSubject>,
        selected: usize,
    },
    Loading {
        request: TriviaRequest,
        status: String,
        started_at: Instant,
    },
    Error {
        request: TriviaRequest,
        message: String,
    },
    Ready {
        request: TriviaRequest,
        items: Vec<TriviaItem>,
        current_idx: usize,
    },
    Question {
        request: TriviaRequest,
        items: Vec<TriviaItem>,
        current_idx: usize,
        start_time: Instant,
        duration: Duration,
    },
    Answer {
        request: TriviaRequest,
        items: Vec<TriviaItem>,
        current_idx: usize,
        selected_action: AnswerAction,
    },
    ExplanationLoading {
        request: TriviaRequest,
        items: Vec<TriviaItem>,
        current_idx: usize,
        question: String,
        answer: String,
        status: String,
        started_at: Instant,
    },
    Explanation {
        request: TriviaRequest,
        items: Vec<TriviaItem>,
        current_idx: usize,
        explanation: String,
        scroll_offset: usize,
    },
}

impl AppState {
    pub fn initial() -> Self {
        AppState::SubjectMenu {
            subjects: TriviaSubject::all(),
            selected: 0,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self,
            AppState::Loading { .. } | AppState::ExplanationLoading { .. }
        )
    }

    pub fn pending_background_job(&self) -> Option<PendingBackgroundJob> {
        match self {
            AppState::Loading { request, .. } => Some(PendingBackgroundJob::Trivia(*request)),
            AppState::ExplanationLoading {
                question,
                answer,
                ..
            } => Some(PendingBackgroundJob::Explanation(ExplanationRequest {
                question: question.clone(),
                answer: answer.clone(),
            })),
            _ => None,
        }
    }

    pub fn move_menu_up(&mut self) {
        match self {
            AppState::SubjectMenu { subjects, selected } => {
                if subjects.is_empty() {
                    return;
                }
                *selected = if *selected == 0 {
                    subjects.len() - 1
                } else {
                    *selected - 1
                };
            }
            _ => {}
        }
    }

    pub fn move_menu_left(&mut self) {
        match self {
            AppState::SubjectMenu { subjects, selected } => {
                if subjects.is_empty() {
                    return;
                }
                *selected = if *selected == 0 {
                    subjects.len() - 1
                } else {
                    *selected - 1
                };
            }
            _ => {}
        }
    }

    pub fn move_menu_down(&mut self) {
        match self {
            AppState::SubjectMenu { subjects, selected } => {
                if subjects.is_empty() {
                    return;
                }
                *selected = (*selected + 1) % subjects.len();
            }
            _ => {}
        }
    }

    pub fn move_menu_right(&mut self) {
        match self {
            AppState::SubjectMenu { subjects, selected } => {
                if subjects.is_empty() {
                    return;
                }
                *selected = (*selected + 1) % subjects.len();
            }
            _ => {}
        }
    }

    pub fn confirm_menu_selection(&mut self) {
        match self {
            AppState::SubjectMenu { subjects, selected } => {
                if subjects.is_empty() {
                    return;
                }
                let subject = subjects[*selected];
                match subject {
                    TriviaSubject::Bible => {
                        let request = TriviaRequest::for_bible();
                        *self = AppState::Loading {
                            request,
                            status: request.loading_status(),
                            started_at: Instant::now(),
                        };
                    }
                    TriviaSubject::FunFacts => {
                        let request = TriviaRequest {
                            subject: TriviaSubject::FunFacts,
                        };
                        *self = AppState::Loading {
                            request,
                            status: request.loading_status(),
                            started_at: Instant::now(),
                        };
                    }
                    TriviaSubject::Riddles => {
                        let request = TriviaRequest {
                            subject: TriviaSubject::Riddles,
                        };
                        *self = AppState::Loading {
                            request,
                            status: request.loading_status(),
                            started_at: Instant::now(),
                        };
                    }
                }
            }
            _ => {}
        }
    }

    pub fn return_to_subject_menu(&mut self) {
        *self = AppState::SubjectMenu {
            subjects: TriviaSubject::all(),
            selected: 0,
        };
    }

    pub fn apply_load_result(&mut self, load: BackgroundLoadResult) {
        let (request, result) = load;
        match result {
            Ok(items) => {
                *self = AppState::Ready {
                    request,
                    items,
                    current_idx: 0,
                }
            }
            Err(e) => {
                *self = AppState::Error {
                    request,
                    message: e,
                }
            }
        }
    }

    pub fn apply_explanation_result(&mut self, load: BackgroundExplanationResult) {
        let (_request, result) = load;
        match result {
            Ok(explanation) => {
                if let AppState::ExplanationLoading {
                    request: trivia_request,
                    items,
                    current_idx,
                    ..
                } = self
                {
                    // Cache explanation in the item
                    if let Some(item) = items.get_mut(*current_idx) {
                        item.explanation = Some(explanation.clone());
                    }
                    let items = items.clone();
                    let current_idx = *current_idx;
                    *self = AppState::Explanation {
                        request: *trivia_request,
                        items,
                        current_idx,
                        explanation,
                        scroll_offset: 0,
                    };
                }
            }
            Err(_message) => {
                // Gracefully return to Answer screen instead of destroying the game session.
                if let AppState::ExplanationLoading {
                    request: trivia_request,
                    items,
                    current_idx,
                    ..
                } = self
                {
                    *self = AppState::Answer {
                        request: *trivia_request,
                        items: items.clone(),
                        current_idx: *current_idx,
                        selected_action: AnswerAction::Continue,
                    };
                }
            }
        }
    }

    /// Handle background trivia-load thread crash / channel disconnect.
    pub fn apply_load_result_disconnected(&mut self) {
        if let AppState::Loading { request, .. } = self {
            *self = AppState::Error {
                request: *request,
                message: "Background task crashed unexpectedly.".to_string(),
            };
        }
    }

    /// Handle background explanation thread crash / channel disconnect.
    pub fn apply_explanation_result_disconnected(&mut self) {
        if let AppState::ExplanationLoading {
            request: trivia_request,
            items,
            current_idx,
            ..
        } = self
        {
            *self = AppState::Answer {
                request: *trivia_request,
                items: items.clone(),
                current_idx: *current_idx,
                selected_action: AnswerAction::Continue,
            };
        }
    }

    pub fn start_game(&mut self) {
        if let AppState::Ready {
            request,
            items,
            current_idx,
        } = self
        {
            if let Some(item) = items.get(*current_idx) {
                mark_question_seen_on_display(*request, &item.question);
            }
            *self = AppState::Question {
                request: *request,
                items: items.clone(),
                current_idx: *current_idx,
                start_time: Instant::now(),
                duration: Duration::from_secs(60),
            };
        }
    }

    pub fn move_answer_down(&mut self) {
        if let AppState::Answer {
            request,
            selected_action,
            ..
        } = self
        {
            if request.subject.supports_explanation() {
                *selected_action = (*selected_action).other();
            }
        }
    }

    pub fn confirm_answer_selection(&mut self) {
        match self {
            AppState::Answer {
                selected_action, ..
            } => match *selected_action {
                AnswerAction::ExplainFurther => self.begin_explanation(),
                AnswerAction::Continue => self.next_step(),
            },
            _ => {}
        }
    }

    pub fn explanation_page_forward(&mut self, total_lines: usize, visible_lines: usize) {
        let should_return = if let AppState::Explanation { scroll_offset, .. } = self {
            let max_scroll = total_lines.saturating_sub(visible_lines);
            if *scroll_offset >= max_scroll {
                true
            } else {
                *scroll_offset = (*scroll_offset + visible_lines).min(max_scroll);
                false
            }
        } else {
            false
        };
        if should_return {
            self.return_to_answer();
        }
    }

    pub fn return_to_answer(&mut self) {
        match self {
            AppState::Explanation {
                request,
                items,
                current_idx,
                ..
            }
            | AppState::ExplanationLoading {
                request,
                items,
                current_idx,
                ..
            } => {
                *self = AppState::Answer {
                    request: *request,
                    items: items.clone(),
                    current_idx: *current_idx,
                    selected_action: AnswerAction::Continue,
                };
            }
            _ => {}
        }
    }

    pub fn begin_explanation(&mut self) {
        if let AppState::Answer {
            request,
            items,
            current_idx,
            ..
        } = self
        {
            if let Some(item) = items.get(*current_idx) {
                if let Some(cached) = &item.explanation {
                    *self = AppState::Explanation {
                        request: *request,
                        items: items.clone(),
                        current_idx: *current_idx,
                        explanation: cached.clone(),
                        scroll_offset: 0,
                    };
                } else {
                    *self = AppState::ExplanationLoading {
                        request: *request,
                        items: items.clone(),
                        current_idx: *current_idx,
                        question: item.question.clone(),
                        answer: item.answer.clone(),
                        status: "Loading explanation".to_string(),
                        started_at: Instant::now(),
                    };
                }
            }
        }
    }

    pub fn next_step(&mut self) {
        match self {
            AppState::Question {
                request,
                items,
                current_idx,
                ..
            } => {
                *self = AppState::Answer {
                    request: *request,
                    items: items.clone(),
                    current_idx: *current_idx,
                    selected_action: AnswerAction::Continue,
                };
            }
            AppState::Answer {
                request,
                items,
                current_idx,
                ..
            } => {
                let next_idx = *current_idx + 1;
                if next_idx < items.len() {
                    if let Some(item) = items.get(next_idx) {
                        mark_question_seen_on_display(*request, &item.question);
                    }
                    *self = AppState::Question {
                        request: *request,
                        items: items.clone(),
                        current_idx: next_idx,
                        start_time: Instant::now(),
                        duration: Duration::from_secs(60),
                    };
                } else {
                    *self = AppState::Loading {
                        request: *request,
                        status: request.loading_status(),
                        started_at: Instant::now(),
                    };
                }
            }
            _ => {}
        }
    }

    pub fn update(&mut self) {}
}

pub fn start_background_load(request: TriviaRequest) -> mpsc::Receiver<BackgroundLoadResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = fetch_trivia(request);
        let _ = tx.send((request, result));
    });
    rx
}

pub fn start_background_explanation(
    request: ExplanationRequest,
) -> mpsc::Receiver<BackgroundExplanationResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = fetch_explanation(&request);
        let _ = tx.send((request, result));
    });
    rx
}

fn round_size(subject: TriviaSubject) -> usize {
    match subject {
        TriviaSubject::Bible | TriviaSubject::FunFacts => ROUND_SIZE_BIBLE_FUN_FACTS,
        TriviaSubject::Riddles => ROUND_SIZE_RIDDLES,
    }
}

fn json_output_contract() -> &'static str {
    "MOST IMPORTANT REQUIREMENT: Return valid JSON only.\n\
Do not include markdown, explanations, or code fences.\n\
Return exactly one top-level JSON array where each item has string fields 'question' and 'answer'."
}

fn trivia_item_array_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "ARRAY",
        "items": {
            "type": "OBJECT",
            "required": ["question", "answer"],
            "properties": {
                "question": { "type": "STRING" },
                "answer": { "type": "STRING" }
            }
        }
    })
}

fn string_array_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "ARRAY",
        "items": {
            "type": "STRING"
        }
    })
}

fn explanation_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "OBJECT",
        "required": ["explanation"],
        "properties": {
            "explanation": { "type": "STRING" }
        }
    })
}

fn parse_gemini_json<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    if let Ok(parsed_direct) = serde_json::from_str::<T>(text.trim()) {
        return Ok(parsed_direct);
    }

    // Try extracting a JSON array
    if let (Some(s), Some(e)) = (text.find('['), text.rfind(']')) {
        if e > s {
            if let Ok(parsed) = serde_json::from_str::<T>(&text[s..=e]) {
                return Ok(parsed);
            }
        }
    }

    // Try extracting a JSON object
    if let (Some(s), Some(e)) = (text.find('{'), text.rfind('}')) {
        if e > s {
            if let Ok(parsed) = serde_json::from_str::<T>(&text[s..=e]) {
                return Ok(parsed);
            }
        }
    }

    Err(format!("No valid JSON found in response: {}", text))
}

fn call_gemini(
    client: &Client,
    api_key: &str,
    prompt: &str,
    response_schema: serde_json::Value,
) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite-preview:generateContent?key={}",
        api_key
    );

    let payload = serde_json::json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": response_schema
        }
    });

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| format!("Network error: {}. Check internet connection.", e))?;

    let status = res.status();
    if !status.is_success() {
        let err_text = res.text().unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Gemini API error ({}): {}", status, err_text));
    }

    let gemini_res: GeminiResponse = res
        .json()
        .map_err(|e| format!("Response format error: {}. Maybe wrong API type?", e))?;

    let candidate = gemini_res
        .candidates
        .first()
        .ok_or("No candidates in response (Check safety filters/billing)")?;
    let part = candidate
        .content
        .parts
        .first()
        .ok_or("No text in response part")?;

    Ok(part.text.clone())
}

fn should_ai_dedup(subject: TriviaSubject) -> bool {
    matches!(
        subject,
        TriviaSubject::Bible | TriviaSubject::FunFacts | TriviaSubject::Riddles
    )
}

fn should_generate_new_batch(subject: TriviaSubject) -> bool {
    matches!(subject, TriviaSubject::Riddles)
}

fn now_epoch_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn unique_question_items_from_cache(cached: &[CachedTriviaItem]) -> Vec<TriviaItem> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in cached {
        if seen.insert(item.question.clone()) {
            out.push(TriviaItem {
                question: item.question.clone(),
                answer: item.answer.clone(),
                explanation: None,
            });
        }
    }
    out
}

fn unseen_items_from_cache(
    cached: &[CachedTriviaItem],
    seen_questions: &[String],
) -> Vec<TriviaItem> {
    let seen_set: HashSet<String> = seen_questions.iter().cloned().collect();
    let mut emitted = HashSet::new();
    let mut out = Vec::new();

    for item in cached {
        if seen_set.contains(&item.question) {
            continue;
        }
        if emitted.insert(item.question.clone()) {
            out.push(TriviaItem {
                question: item.question.clone(),
                answer: item.answer.clone(),
                explanation: None,
            });
        }
    }

    out
}

fn pick_random_items(items: &[TriviaItem], count: usize) -> Vec<TriviaItem> {
    let mut picked = items.to_vec();
    let mut rng = rand::rng();
    picked.shuffle(&mut rng);
    picked.truncate(count);
    picked
}

fn all_cache_questions_seen(cached: &[CachedTriviaItem], seen_questions: &[String]) -> bool {
    let cache_questions: HashSet<String> = cached.iter().map(|i| i.question.clone()).collect();
    if cache_questions.is_empty() {
        return false;
    }
    let seen_set: HashSet<String> = seen_questions.iter().cloned().collect();
    cache_questions.iter().all(|q| seen_set.contains(q))
}

fn select_round_from_cache(
    cached: &[CachedTriviaItem],
    seen_questions: &[String],
    round_size: usize,
) -> Vec<TriviaItem> {
    let unseen = unseen_items_from_cache(cached, seen_questions);
    if unseen.len() >= round_size {
        return pick_random_items(&unseen, round_size);
    }

    if all_cache_questions_seen(cached, seen_questions) {
        return pick_random_items(&unique_question_items_from_cache(cached), round_size);
    }

    if unseen.is_empty() {
        return pick_random_items(&unique_question_items_from_cache(cached), round_size);
    }

    // Early-life cache edge case: fewer than round-size unseen exist but cache is not fully seen.
    unseen
}

fn reconcile_seen_with_cache(seen: &[String], cached: &[CachedTriviaItem]) -> Vec<String> {
    let cache_questions: HashSet<String> = cached.iter().map(|i| i.question.clone()).collect();
    seen.iter()
        .filter(|q| cache_questions.contains(*q))
        .cloned()
        .collect()
}

fn filter_trivia_items_by_question_list(
    items: &[TriviaItem],
    questions: &[String],
) -> Vec<TriviaItem> {
    let allowed: HashSet<String> = questions.iter().cloned().collect();
    let mut kept = HashSet::new();
    let mut filtered = Vec::new();
    for item in items {
        if !allowed.contains(&item.question) || !kept.insert(item.question.clone()) {
            continue;
        }
        filtered.push(item.clone());
    }

    filtered
}

fn filter_cached_items_by_question_list(
    items: &[CachedTriviaItem],
    questions: &[String],
) -> Vec<CachedTriviaItem> {
    let allowed: HashSet<String> = questions.iter().cloned().collect();
    let mut kept = HashSet::new();
    let mut filtered = Vec::new();
    for item in items {
        if !allowed.contains(&item.question) || !kept.insert(item.question.clone()) {
            continue;
        }
        filtered.push(item.clone());
    }

    filtered
}

fn ensure_non_stale_cache(cached: &[CachedTriviaItem]) -> Vec<CachedTriviaItem> {
    let now = now_epoch_secs();
    cached
        .iter()
        .filter(|item| now.saturating_sub(item.added_at_epoch_secs) <= CACHE_TTL_SECS)
        .cloned()
        .collect()
}

fn ai_dedup_question_batch(
    client: &Client,
    api_key: &str,
    seen_questions: &[String],
    candidate_questions: &[String],
) -> Result<Vec<String>, String> {
    if candidate_questions.is_empty() {
        return Ok(Vec::new());
    }

    let prompt = format!(
        "Return valid JSON only.\n\
You are deduplicating trivia questions.\n\
Previously accepted questions are a permanent ban list.\n\
Candidate questions may also contain duplicates of each other.\n\
Remove any candidate question that is a semantic duplicate, paraphrase, same fact, same underlying concept, or same knowledge point as any banned question or any other candidate question.\n\
Keep the earliest candidate when two candidates overlap.\n\
Return ONLY the surviving candidate questions as a JSON array of strings, preserving each surviving question's exact original text.\n\
Do not rewrite, summarize, normalize, or reformat any question.\n\
\n\
BANNED_QUESTIONS_JSON:\n{}\n\
\n\
CANDIDATE_QUESTIONS_JSON:\n{}",
        serde_json::to_string_pretty(seen_questions)
            .map_err(|e| format!("Failed to serialize seen questions: {}", e))?,
        serde_json::to_string_pretty(candidate_questions)
            .map_err(|e| format!("Failed to serialize candidate questions: {}", e))?
    );

    let text = call_gemini(client, api_key, &prompt, string_array_schema())?;
    parse_gemini_json::<Vec<String>>(&text)
}

fn ai_dedup_cache(
    client: &Client,
    api_key: &str,
    cached: &[CachedTriviaItem],
) -> Result<Vec<CachedTriviaItem>, String> {
    const AI_DEDUP_BATCH_SIZE: usize = 500;

    if cached.len() <= 1 {
        return Ok(cached.to_vec());
    }

    let mut survivors: Vec<CachedTriviaItem> = Vec::new();
    for chunk in cached.chunks(AI_DEDUP_BATCH_SIZE) {
        let seen_questions: Vec<String> =
            survivors.iter().map(|item| item.question.clone()).collect();
        let candidate_questions: Vec<String> =
            chunk.iter().map(|item| item.question.clone()).collect();
        let surviving_questions =
            ai_dedup_question_batch(client, api_key, &seen_questions, &candidate_questions)?;
        survivors.extend(filter_cached_items_by_question_list(
            chunk,
            &surviving_questions,
        ));
    }

    Ok(survivors)
}

fn ai_dedup_new_items(
    client: &Client,
    api_key: &str,
    new_items: &[TriviaItem],
    seen_questions: &[String],
) -> Result<Vec<TriviaItem>, String> {
    if new_items.is_empty() {
        return Ok(Vec::new());
    }

    let prompt = format!(
        "Return valid JSON only.\n\
You are deduplicating newly generated trivia questions.\n\
Remove any candidate that overlaps with the banned list or with another candidate by fact, concept, event, person, object, data point, or knowledge point, even if phrased differently.\n\
Keep the earliest candidate when two candidates overlap.\n\
Return ONLY the surviving candidate items as a JSON array of objects with string fields 'question' and 'answer'.\n\
Preserve the exact original question and answer text for every surviving item.\n\
Do not rewrite, summarize, or normalize the text.\n\
\n\
BANNED_QUESTIONS_JSON:\n{}\n\
\n\
CANDIDATE_ITEMS_JSON:\n{}",
        serde_json::to_string_pretty(seen_questions)
            .map_err(|e| format!("Failed to serialize seen questions: {}", e))?,
        serde_json::to_string_pretty(new_items)
            .map_err(|e| format!("Failed to serialize candidate items: {}", e))?
    );

    let text = call_gemini(client, api_key, &prompt, trivia_item_array_schema())?;
    let surviving_items = parse_gemini_json::<Vec<TriviaItem>>(&text)?;
    let surviving_questions: Vec<String> = surviving_items
        .into_iter()
        .map(|item| item.question)
        .collect();

    Ok(filter_trivia_items_by_question_list(
        new_items,
        &surviving_questions,
    ))
}

fn fetch_trivia(request: TriviaRequest) -> Result<Vec<TriviaItem>, String> {
    let path = cache_file_path(request)?;
    let mut cached = load_trivia_cache(&path);
    let round_size = round_size(request.subject);

    let mut cache_changed = false;
    let pruned = ensure_non_stale_cache(&cached);
    if pruned.len() != cached.len() {
        cached = pruned;
        cache_changed = true;
    }

    let seen_path = seen_file_path(request);
    let mut seen = if let Some(path) = &seen_path {
        load_seen_questions(path)
    } else {
        Vec::new()
    };
    if seen_path.is_some() {
        let reconciled = reconcile_seen_with_cache(&seen, &cached);
        if reconciled != seen {
            seen = reconciled;
            if let Some(path) = &seen_path {
                persist_seen_questions(path, &seen);
            }
        }
    }

    if cache_changed {
        persist_trivia_cache(&path, &cached);
    }

    let unseen = unseen_items_from_cache(&cached, &seen);

    if should_ai_dedup(request.subject)
        && unseen.len() >= round_size
        && !should_generate_new_batch(request.subject)
    {
        return Ok(select_round_from_cache(&cached, &seen, round_size));
    }

    let api_key = std::env::var("GOOGLE_API_KEY")
        .map_err(|_| "GOOGLE_API_KEY environment variable is not set. Export it in your shell: export GOOGLE_API_KEY='...'".to_string())?;

    let client = Client::builder()
        .user_agent("Kiosk-Trivia/1.0")
        .build()
        .unwrap_or_else(|_| Client::new());

    if should_ai_dedup(request.subject) && !cached.is_empty() {
        match ai_dedup_cache(&client, &api_key, &cached) {
            Ok(deduped) => {
                if deduped.len() != cached.len() {
                    cached = deduped;
                    persist_trivia_cache(&path, &cached);
                } else {
                    cached = deduped;
                }
            }
            Err(err) => {
                eprintln!(
                    "AI cache dedup failed for {}: {}",
                    request.menu_title(),
                    err
                );
            }
        }
    }

    let cached_trivia = unique_question_items_from_cache(&cached);
    let base_prompt = match request.subject {
        TriviaSubject::Bible => {
            let (books, chapters) = get_bible_scope()?;
            bible_prompt(&cached_trivia, &books, &chapters)
        }
        TriviaSubject::FunFacts => {
            let domains = get_domains()?;
            fun_facts_prompt(&cached_trivia, &domains)
        }
        TriviaSubject::Riddles => {
            let topics = get_riddle_topics()?;
            riddles_prompt(&cached_trivia, &topics)
        }
    };
    let prompt = format!("{}\n\n{}", json_output_contract(), base_prompt);
    let text = call_gemini(&client, &api_key, &prompt, trivia_item_array_schema())?;
    let parsed = parse_gemini_json::<Vec<TriviaItem>>(&text)?;

    if parsed.len() < round_size {
        return Err(format!(
            "Model returned fewer than {} trivia items. Please retry.",
            round_size
        ));
    }

    let mut fresh = if should_ai_dedup(request.subject) {
        let seen_questions: Vec<String> = cached.iter().map(|item| item.question.clone()).collect();
        match ai_dedup_new_items(&client, &api_key, &parsed, &seen_questions) {
            Ok(items) => items,
            Err(err) => {
                eprintln!(
                    "AI new-item dedup failed for {}: {}",
                    request.menu_title(),
                    err
                );
                parsed.clone()
            }
        }
    } else {
        parsed.clone()
    };

    // Final exact-string safety net dedup against cache and within this batch.
    let mut seen_keys: HashSet<String> = cached.iter().map(|i| i.question.clone()).collect();
    let mut final_fresh: Vec<TriviaItem> = Vec::new();
    for item in fresh.drain(..) {
        if item.question.trim().is_empty() || !seen_keys.insert(item.question.clone()) {
            continue;
        }
        final_fresh.push(item);
    }
    fresh = final_fresh;

    if fresh.is_empty() {
        return Err(
            "All generated questions were duplicates. Press Enter to request another batch."
                .to_string(),
        );
    }

    // Append fresh items to cache with timestamps and persist.
    let now = now_epoch_secs();
    for item in &fresh {
        cached.push(CachedTriviaItem {
            question: item.question.clone(),
            answer: item.answer.clone(),
            added_at_epoch_secs: now,
        });
    }
    persist_trivia_cache(&path, &cached);

    if should_ai_dedup(request.subject) {
        return Ok(select_round_from_cache(&cached, &seen, round_size));
    }

    // Recent News and other default paths still return smaller rounds.
    fresh.truncate(round_size);
    Ok(fresh)
}

fn previously_asked_block(cached: &[TriviaItem]) -> String {
    if cached.is_empty() {
        return String::new();
    }

    let n = cached.len().min(1000);
    let start = cached.len().saturating_sub(n);

    let mut block = String::from(
        "\n\nDo NOT repeat or reuse ANY of the following in ANY form:\n\
        - Do NOT repeat the same question text.\n\
        - Do NOT reuse the same underlying fact, idea, theme, event, character, concept, or data point.\n\
        - Do NOT create a question that is a rephrasing, variation, or semantic equivalent of any previously asked question.\n\
        - Do NOT use the same source material, narrative cluster, or conceptual category as any previously asked item.\n\
        - Treat all previously asked questions as permanently banned concepts.\n\
        Previously asked items:\n"
    );

    for item in &cached[start..] {
        block.push_str("- ");
        block.push_str(&item.question);
        block.push('\n');
    }

    block
}

fn bible_prompt(cached: &[TriviaItem], books: &[String], chapters: &[String]) -> String {
    let books_list = books.join(", ");
    let chapters_list = chapters.join(", ");

    let mut prompt = r#"
Task:
Generate exactly 100 unique, highly challenging Bible trivia questions rooted in deep textual facts from the King James Bible.

Difficulty Level:
- Target Audience: Seasoned Christians with deep knowledge of the biblical text.
- Challenge: Focus on obscure verses, minor characters, specific numbers, unusual laws, rare events, and overlooked narrative details. Avoid surface-level stories (e.g., the Nativity, the Flood, David vs Goliath) unless asking about an extremely niche technical detail of the account.

Style Guidelines:
- Format: Use a professional, direct trivia-competition style (Jeopardy-like).
- Clarity: Questions must be sharp, factual, and free of conversational filler, "witty" setups, or humorous framing.
- Answers: Answers should be concise but can be phrases or names as required by the text. Avoid unnecessary explanations unless required for identification.

Content Rules:
- All facts must come directly from the King James Bible.
- No paraphrasing: the fact must be verifiable word-for-word in the KJV.
- Use the following randomly selected scope for this run to ensure diversity:
    * Books focus (5 random books): __BOOKS__
    * Chapter focus (20 random Book:Chapter targets): __CHAPTERS__
- For each question, choose a fact from either the selected books OR the selected Book:Chapter targets.

Novelty Rules:
- Do NOT repeat or reuse ANY previously asked question in ANY form.
- Do NOT reuse the same underlying fact, event, or character detail.
- Treat all previously asked questions as permanently banned concepts.
"#
    .replace("__BOOKS__", &books_list)
    .replace("__CHAPTERS__", &chapters_list);

    prompt.push_str(&previously_asked_block(cached));
    prompt
}

fn load_bible_book_chapter_counts() -> Result<Vec<(String, usize)>, String> {
    let contents = include_str!("../assets/bible-books-chapter-counts.txt");
    let mut out = Vec::new();

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split('|');
        let book = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Invalid bible chapter-count line {}", idx + 1))?;
        let count_str = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Missing chapter count on line {}", idx + 1))?;

        if parts.next().is_some() {
            return Err(format!("Too many fields on line {}", idx + 1));
        }

        let count = count_str
            .parse::<usize>()
            .map_err(|e| format!("Invalid chapter count on line {}: {}", idx + 1, e))?;

        if count == 0 {
            return Err(format!("Chapter count must be > 0 on line {}", idx + 1));
        }

        out.push((book.to_string(), count));
    }

    if out.len() < 5 {
        return Err("Bible chapter-count asset has fewer than 5 books".to_string());
    }

    Ok(out)
}

fn get_bible_scope() -> Result<(Vec<String>, Vec<String>), String> {
    let mut rng = rand::rng();
    let entries = load_bible_book_chapter_counts()?;

    let mut books: Vec<String> = entries.iter().map(|(book, _)| book.clone()).collect();
    books.shuffle(&mut rng);
    books.truncate(5);

    let mut chapter_pool: Vec<String> = Vec::new();
    for (book, count) in &entries {
        for chapter in 1..=*count {
            chapter_pool.push(format!("{}:{}", book, chapter));
        }
    }

    if chapter_pool.len() < 20 {
        return Err("Bible chapter pool has fewer than 20 entries".to_string());
    }

    chapter_pool.shuffle(&mut rng);
    chapter_pool.truncate(20);

    Ok((books, chapter_pool))
}

fn get_domains() -> Result<Vec<String>, String> {
    // Keep this simple and consistent with other bundled assets.
    let contents = include_str!("../assets/dictionary-words.txt");

    let mut words: Vec<String> = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    if words.is_empty() {
        return Err("dictionary-words.txt is empty".to_string());
    }

    let mut rng = rand::rng();
    words.shuffle(&mut rng);
    words.truncate(words.len().min(100));

    Ok(words)
}

fn fun_facts_prompt(cached: &[TriviaItem], domains: &[String]) -> String {
    let domain_list = domains.join(", ");

    let mut prompt = format!(
        r#"
Task:
Generate exactly 100 unique, intellectually stimulating trivia questions based on the provided domain list.

Difficulty Level:
- Target Audience: 10th-grade education level and up.
- Challenge: Questions must be genuinely difficult. Avoid common knowledge, obvious answers, or basic pop culture. Aim for questions that would challenge educated adults in a competitive trivia setting.

Style Guidelines:
- Format: Use a professional, trivia-competition style (similar to Jeopardy! or Trivial Pursuit).
- Clarity: Questions must be direct and unambiguous. Avoid conversational filler or 'witty' setups.
- Answers: Answers should be concise. While they can be phrases if absolutely necessary for accuracy, prefer proper nouns, specific dates, or specific scientific terms.

Anti-Stupidity & Anti-Definition Rules:
- NEVER write a question that asks for the definition, purpose, or functional description of a common object. Do NOT ask "What is X?" or "What is X used for?".
- Answers must NEVER be a descriptive phrase or a functional explanation (e.g., "a projectile designed for impact").
- The answer must not be a word that is already contained within the question, and the question must not be a generic synonym of the answer.
- CRITICAL EXAMPLE OF WHAT NOT TO DO:
  * BAD QUESTION: "What is a pellet used for in a pellet gun?" (Answer: "A projectile for aerodynamic stability.") -> REASON: It is a functional definition of a common object with a descriptive answer.
  * GOOD QUESTION: "In 1886, William F. Markham invented this specific type of spring-piston air gun, made largely of maple wood, to sell alongside his wooden cisterns." (Answer: "The Daisy BB Gun.") -> REASON: It asks for a specific proper noun based on a historical fact.

Content Categories (Ensure a balanced mix of the following):
- Counterintuitive: Common misconceptions or surprising truths that most people would initially guess wrong.
- Quantitative: Specific measurements, dates, records, rankings, or durations.
- Logical Deduction: Facts that can be reasoned through via context clues and multi-step thought, not just recall.
- Academic/Niche: Deep-cut history, science, geography, arts, and human culture.

Domain Rules:
- Primary Source: Use the following list as a foundation for diversity: {domain_list}.
- Expansion: You ARE encouraged to expand beyond this list. Incorporate history, famous people, significant places, and events not explicitly listed, provided they fit the categories above.
- Variety: Each question should ideally tackle a different specific topic or conceptual cluster.
- Safety: Avoid violent, sexual, disturbing, or fear-based content.

Novelty Rules:
- Do NOT repeat or reuse ANY previously asked question in ANY form.
- Do NOT reuse the same underlying fact, idea, theme, event, concept, or data point.
- Do NOT create a question that is a rephrasing, variation, or semantic equivalent of any previously asked question.
- Treat all previously asked questions as permanently banned concepts.
"#
    );

    prompt.push_str(&previously_asked_block(cached));
    prompt
}

fn get_riddle_topics() -> Result<Vec<String>, String> {
    let contents = include_str!("../assets/riddle-topics.json");
    let mut topics = serde_json::from_str::<Vec<String>>(contents)
        .map_err(|e| format!("Failed to parse riddle-topics.json: {}", e))?;

    topics.retain(|topic| !topic.trim().is_empty());
    if topics.is_empty() {
        return Err("riddle-topics.json has no usable topics".to_string());
    }

    let mut rng = rand::rng();
    topics.shuffle(&mut rng);
    topics.truncate(topics.len().min(16));
    Ok(topics)
}

fn riddles_prompt(cached: &[TriviaItem], topics: &[String]) -> String {
    let topic_list = topics
        .iter()
        .map(|topic| format!("- {}", topic))
        .collect::<Vec<String>>()
        .join("\n");

    let mut prompt = format!(
        r#"
Tasks:
- Generate exactly 10 original, creative riddles.
- Provide one concise answer per riddle.
- Use the random topic list below as inspiration and spread coverage across multiple topics.
- Keep difficulty balanced: 50% should match the current fun/challenging level, and 50% should be noticeably harder.
- For the harder 50%, self-check each one at >=8/10 for cleverness, misdirection, and non-obvious deduction.

Content Rules:
- Riddles must be solvable, clear, and family-friendly.
- Avoid extreme obscurity or trick wording that makes the answer arbitrary.
- Answers should usually be short (1 to 4 words) and specific.
- Use a mix of concrete objects, nature, time, language, and everyday concepts.
- Avoid common stock riddles and overused patterns.
- Harder riddles should use layered clues, metaphor, and at least one subtle misdirection.
- Easier-half riddles should still avoid trivial one-clue giveaways.
- Random topics for this run:
__TOPICS__

Novelty Rules:
- Do NOT repeat or reuse ANY previously asked question in ANY form.
- Do NOT reuse the same underlying answer + clue pattern as any previously asked item.
- Do NOT create a paraphrase or near-duplicate of any previously asked riddle.
- Treat all previously asked questions as permanently banned concepts.

Output Rules:
- Return ONLY a JSON array of exactly 10 objects.
- Each object must contain fields 'question' and 'answer'.
- No markdown, no commentary, no code fences.
- Target split in this 10-item batch: 5 medium-hard and 5 hard.
"#
    )
    .replace("__TOPICS__", &topic_list);

    prompt.push_str(&previously_asked_block(cached));
    prompt
}

fn explanation_prompt(request: &ExplanationRequest) -> String {
    let context = serde_json::to_string_pretty(&serde_json::json!({
        "question": request.question,
        "answer": request.answer,
    }))
    .unwrap_or_else(|_| {
        format!(
            "{{\"question\": {:?}, \"answer\": {:?}}}",
            request.question, request.answer
        )
    });

    format!(
        "Task: Explain the trivia answer in a clearer, friendlier way.\n\
Question and answer context:\n{}\n\n\
Write a concise but helpful explanation that:\n\
- names the subject plainly,\n\
- explains why the answer is correct,\n\
- adds a little extra context or a simple example,\n\
- stays factual and does not invent details.\n\n\
Keep it to a few short paragraphs so it is easy to scroll on a small screen.\n\
Return only valid JSON with a single string field named 'explanation'.",
        context
    )
}

fn fetch_explanation(request: &ExplanationRequest) -> Result<String, String> {
    let api_key = std::env::var("GOOGLE_API_KEY")
        .map_err(|_| "GOOGLE_API_KEY environment variable is not set. Export it in your shell: export GOOGLE_API_KEY='...'".to_string())?;

    let client = Client::builder()
        .user_agent("Kiosk-Trivia/1.0")
        .build()
        .unwrap_or_else(|_| Client::new());

    let prompt = explanation_prompt(request);
    let text = call_gemini(&client, &api_key, &prompt, explanation_schema())?;

    #[derive(Deserialize)]
    struct ExplanationResponse {
        explanation: String,
    }

    let parsed = parse_gemini_json::<ExplanationResponse>(&text)?;
    if parsed.explanation.trim().is_empty() {
        return Err("Model returned an empty explanation.".to_string());
    }

    Ok(parsed.explanation)
}

// ---------------------------------------------------------------------------
// Trivia cache (JSON file with full question + answer pairs)
// ---------------------------------------------------------------------------

fn cache_file_path(request: TriviaRequest) -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME is not set; unable to build trivia cache path".to_string())?;
    let mut dir = PathBuf::from(home);
    dir.push(".config");
    dir.push("games-kiosk");
    dir.push("trivia-cache");

    let file_name = match request.subject {
        TriviaSubject::Bible => "bible.json".to_string(),
        TriviaSubject::FunFacts => "fun-facts.json".to_string(),
        TriviaSubject::Riddles => "riddles.json".to_string(),
    };

    dir.push(file_name);
    Ok(dir)
}

fn load_trivia_cache(path: &PathBuf) -> Vec<CachedTriviaItem> {
    if let Ok(contents) = fs::read_to_string(path) {
        if let Ok(parsed) = serde_json::from_str::<Vec<CachedTriviaItem>>(&contents) {
            return parsed;
        }

        // Backward-compat: old format had no timestamps.
        if let Ok(legacy) = serde_json::from_str::<Vec<TriviaItem>>(&contents) {
            let now = now_epoch_secs();
            return legacy
                .into_iter()
                .map(|item| CachedTriviaItem {
                    question: item.question,
                    answer: item.answer,
                    added_at_epoch_secs: now,
                })
                .collect();
        }

        Vec::new()
    } else {
        Vec::new()
    }
}

fn persist_trivia_cache(path: &PathBuf, items: &[CachedTriviaItem]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(items) {
        let _ = fs::write(path, json);
    }
}

fn seen_file_path(request: TriviaRequest) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let mut dir = PathBuf::from(home);
    dir.push(".config");
    dir.push("games-kiosk");
    dir.push("trivia-cache");

    let file_name = match request.subject {
        TriviaSubject::Bible => "bible-seen.json",
        TriviaSubject::FunFacts => "fun-facts-seen.json",
        TriviaSubject::Riddles => "riddles-seen.json",
    };

    dir.push(file_name);
    Some(dir)
}

fn load_seen_questions(path: &PathBuf) -> Vec<String> {
    if let Ok(contents) = fs::read_to_string(path) {
        serde_json::from_str::<Vec<String>>(&contents).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn persist_seen_questions(path: &PathBuf, seen: &[String]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(seen) {
        let _ = fs::write(path, json);
    }
}

fn mark_question_seen_on_display(request: TriviaRequest, question: &str) {
    let Some(path) = seen_file_path(request) else {
        return;
    };

    if question.trim().is_empty() {
        return;
    }

    let mut seen = load_seen_questions(&path);
    if seen.iter().any(|existing| existing == question) {
        return;
    }

    seen.push(question.to_string());
    persist_seen_questions(&path, &seen);
}

