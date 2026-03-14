use chrono::Local;
use rand::seq::SliceRandom;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ROUND_SIZE_BIBLE_FUN_FACTS: usize = 21;
const ROUND_SIZE_DEFAULT: usize = 5;
const CACHE_TTL_SECS: i64 = 14 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriviaItem {
    pub question: String,
    pub answer: String,
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
    RecentNews,
    FunFacts,
}

impl TriviaSubject {
    pub fn label(self) -> &'static str {
        match self {
            TriviaSubject::Bible => "Bible",
            TriviaSubject::RecentNews => "Recent News",
            TriviaSubject::FunFacts => "Fun Facts",
        }
    }

    pub fn all() -> Vec<TriviaSubject> {
        vec![
            TriviaSubject::Bible,
            TriviaSubject::RecentNews,
            TriviaSubject::FunFacts,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NewsCategory {
    All,
    ScienceTechnology,
    Entertainment,
    Politics,
}

impl NewsCategory {
    pub fn label(self) -> &'static str {
        match self {
            NewsCategory::All => "All",
            NewsCategory::ScienceTechnology => "Science + Technology",
            NewsCategory::Entertainment => "Entertainment",
            NewsCategory::Politics => "Politics",
        }
    }

    pub fn all() -> Vec<NewsCategory> {
        vec![
            NewsCategory::All,
            NewsCategory::ScienceTechnology,
            NewsCategory::Entertainment,
            NewsCategory::Politics,
        ]
    }

    fn search_query(self) -> &'static str {
        match self {
            NewsCategory::All => "(weird OR bizarre OR funny OR odd OR unexpected OR unusual OR offbeat OR quirky)",
            NewsCategory::ScienceTechnology => "(science OR technology OR AI OR space OR robotics) AND (weird OR funny OR odd OR surprising)",
            NewsCategory::Entertainment => "(entertainment OR celebrity OR movie OR music OR tv) AND (weird OR funny OR awkward OR bizarre)",
            NewsCategory::Politics => "(politics OR election OR congress OR senate OR government) AND (unexpected OR odd OR surprising)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriviaRequest {
    pub subject: TriviaSubject,
    pub news_category: Option<NewsCategory>,
}

impl TriviaRequest {
    fn for_bible() -> Self {
        Self {
            subject: TriviaSubject::Bible,
            news_category: None,
        }
    }

    fn for_recent_news(category: NewsCategory) -> Self {
        Self {
            subject: TriviaSubject::RecentNews,
            news_category: Some(category),
        }
    }

    pub fn menu_title(self) -> String {
        match self.subject {
            TriviaSubject::Bible => "Bible".to_string(),
            TriviaSubject::RecentNews => {
                let cat = self.news_category.map(|c| c.label()).unwrap_or("All");
                format!("Recent News ({})", cat)
            }
            TriviaSubject::FunFacts => "Fun Facts".to_string(),
        }
    }

    pub fn loading_status(self) -> String {
        match self.subject {
            TriviaSubject::Bible => "Loading Bible questions".to_string(),
            TriviaSubject::RecentNews => {
                let cat = self.news_category.map(|c| c.label()).unwrap_or("All");
                format!("Loading Recent News [{}]", cat)
            }
            TriviaSubject::FunFacts => "Loading Fun Facts".to_string(),
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

#[derive(Debug, Deserialize)]
struct NewsApiResponse {
    status: String,
    articles: Vec<NewsArticle>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewsArticle {
    source: NewsSource,
    title: String,
    description: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewsSource {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NewsContextItem {
    source: String,
    title: String,
    description: Option<String>,
    url: Option<String>,
}

const US_TOP_SOURCE_IDS: [&str; 20] = [
    "abc-news",
    "associated-press",
    "axios",
    "bloomberg",
    "business-insider",
    "cbs-news",
    "cnn",
    "fox-news",
    "google-news",
    "msnbc",
    "nbc-news",
    "newsweek",
    "politico",
    "reuters",
    "the-hill",
    "the-new-york-times",
    "the-washington-post",
    "the-wall-street-journal",
    "time",
    "usa-today",
];

pub type BackgroundLoadResult = (TriviaRequest, Result<Vec<TriviaItem>, String>);

pub enum AppState {
    SubjectMenu {
        subjects: Vec<TriviaSubject>,
        selected: usize,
    },
    NewsCategoryMenu {
        categories: Vec<NewsCategory>,
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
        matches!(self, AppState::Loading { .. })
    }

    pub fn loading_request(&self) -> Option<TriviaRequest> {
        match self {
            AppState::Loading { request, .. } => Some(*request),
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
            AppState::NewsCategoryMenu {
                categories,
                selected,
            } => {
                if categories.is_empty() {
                    return;
                }
                *selected = if *selected == 0 {
                    categories.len() - 1
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
            AppState::NewsCategoryMenu {
                categories,
                selected,
            } => {
                if categories.is_empty() {
                    return;
                }
                *selected = (*selected + 1) % categories.len();
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
                    TriviaSubject::RecentNews => {
                        *self = AppState::NewsCategoryMenu {
                            categories: NewsCategory::all(),
                            selected: 0,
                        };
                    }
                    TriviaSubject::FunFacts => {
                        let request = TriviaRequest {
                            subject: TriviaSubject::FunFacts,
                            news_category: None,
                        };
                        *self = AppState::Loading {
                            request,
                            status: request.loading_status(),
                            started_at: Instant::now(),
                        };
                    }
                }
            }
            AppState::NewsCategoryMenu {
                categories,
                selected,
            } => {
                if categories.is_empty() {
                    return;
                }
                let category = categories[*selected];
                let request = TriviaRequest::for_recent_news(category);
                *self = AppState::Loading {
                    request,
                    status: request.loading_status(),
                    started_at: Instant::now(),
                };
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

    pub fn return_to_news_category_menu(&mut self) {
        *self = AppState::NewsCategoryMenu {
            categories: NewsCategory::all(),
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
                };
            }
            AppState::Answer {
                request,
                items,
                current_idx,
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

fn cache_threshold(subject: TriviaSubject) -> usize {
    match subject {
        TriviaSubject::RecentNews => 100,
        _ => 1000,
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

fn parse_gemini_json<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    if let Ok(parsed_direct) = serde_json::from_str::<T>(text.trim()) {
        return Ok(parsed_direct);
    }

    let start = text
        .find('[')
        .ok_or_else(|| format!("No JSON array found in response: {}", text))?;
    let end = text
        .rfind(']')
        .ok_or_else(|| format!("No JSON array found in response: {}", text))?;
    if end <= start {
        return Err(format!("No JSON array found in response: {}", text));
    }

    let clean_json = &text[start..=end];
    serde_json::from_str::<T>(clean_json)
        .map_err(|e| format!("JSON parse error: {} | Body: {}", e, clean_json))
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
    matches!(subject, TriviaSubject::Bible | TriviaSubject::FunFacts)
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
    let round_size = if should_ai_dedup(request.subject) {
        ROUND_SIZE_BIBLE_FUN_FACTS
    } else {
        ROUND_SIZE_DEFAULT
    };

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

    if should_ai_dedup(request.subject) && cached.len() >= cache_threshold(request.subject) {
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

    // Recent News still uses the old flow and round-size.
    if !should_ai_dedup(request.subject) && cached.len() >= cache_threshold(request.subject) {
        return Ok(pick_random_items(
            &unique_question_items_from_cache(&cached),
            round_size,
        ));
    }

    let cached_trivia = unique_question_items_from_cache(&cached);
    let base_prompt = match request.subject {
        TriviaSubject::Bible => {
            let (books, chapters) = get_bible_scope()?;
            bible_prompt(&cached_trivia, &books, &chapters)
        }
        TriviaSubject::RecentNews => recent_news_prompt(&client, request, &cached_trivia)?,
        TriviaSubject::FunFacts => {
            let domains = get_domains()?;
            fun_facts_prompt(&cached_trivia, &domains)
        }
    };
    let prompt = format!("{}\n\n{}", json_output_contract(), base_prompt);
    let text = call_gemini(&client, &api_key, &prompt, trivia_item_array_schema())?;
    let parsed = parse_gemini_json::<Vec<TriviaItem>>(&text)?;

    if parsed.len() < ROUND_SIZE_DEFAULT {
        return Err("Model returned fewer than 5 trivia items. Please retry.".to_string());
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
Generate exactly 100 difficult, highly diverse, 'Wait Wait... Don't Tell Me!' style Bible trivia questions.
The humor should be light, but the underlying facts must be challenging and rooted in lesser-known details.

Content rules:
- All facts must come directly from the King James Bible.
- Prioritize obscure verses, minor characters, unusual laws, strange events, and overlooked narrative details.
- Avoid any story, character, or theme that appears in the cached list.
- Avoid all high-frequency trivia topics (Creation, Flood, Exodus, David, Solomon, Daniel, Jonah, Nativity, Crucifixion, Resurrection).
- Avoid adjacent repeats: do not reuse the same book, character, or narrative cluster more than once.
- Use the following randomly selected scope for this run:
    * Books focus (5 random books): __BOOKS__
    * Chapter focus (20 random Book:Chapter targets, chapters only): __CHAPTERS__
- For each question, choose scope from either the selected books OR the selected Book:Chapter targets.
- Do not use verses in scope selection.

Difficulty rules:
- Each question must hinge on a detail that is *not* widely known.
- Prefer specifics: numbers, names, locations, odd commands, unusual phrasing, rare events.
- No paraphrasing: the fact must be verifiable word-for-word in the KJV.
- No two questions may rely on similar types of facts (e.g., no two “who said this?” or “which king did X?”).

Style rules:
- Humor should come from the framing, not from altering the biblical fact.
- Keep questions accessible but intellectually demanding.

Output rules:
- Return ONLY a JSON array of exactly 100 objects.
- Each object must contain fields 'question' and 'answer'.
- No markdown, no commentary, no code fences.
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
Generate exactly 100 intellectually stimulating, real-world trivia questions with concise answers.
The tone should feel playful and sharp, like a strong public-radio trivia segment, but the substance should be genuinely challenging.

Content rules:
- All facts must be true, verifiable, and non-exaggerated.
- Use a wide variety of domains from the following list: {domain_list}.
- Each question must come from a different domain whenever possible.
- If the domain list contains fewer than 100 items, cycle through them but do NOT reuse the same fact type, idea, or concept.
- Every question must require genuine thought. Avoid anything answerable by broad common knowledge alone.
- No question should feel obvious, classroom-basic, or like a generic children's fact book.
- Hard questions must still be age-appropriate and understandable once answered.
- Avoid violent, sexual, disturbing, or fear-based content.

Difficulty rules:
- Roughly 25% should be counterintuitive or surprising facts that most people would initially guess wrong.
- Roughly 25% should be quantitative or comparative: bigger/smaller, older/newer, faster/slower, longer/shorter, higher/lower, earlier/later.
- Roughly 25% should require multi-step reasoning, not mere recall.
- Roughly 25% should be deep-cut facts from science, history, geography, language, nature, engineering, or human culture.
- Prefer specifics: numbers, dates, names, rankings, measurements, durations, distances, and unusual constraints.
- Answers must be precise, not vague.
- Aim for questions that fewer than 30% of adults would answer correctly without thinking.

Novelty rules:
- Do NOT repeat or reuse ANY previously asked question in ANY form.
- Do NOT reuse the same underlying fact, idea, theme, event, concept, or data point.
- Do NOT create a question that is a rephrasing, variation, or semantic equivalent of any previously asked question.
- Do NOT use the same source material, topic cluster, or conceptual category as any previously asked item.
- Treat all previously asked questions as permanently banned concepts.

Output rules:
- Return ONLY a JSON array of exactly 100 objects.
- Each object must contain fields 'question' and 'answer'.
- No markdown, no commentary, no code fences.
"#
    );

    prompt.push_str(&previously_asked_block(cached));
    prompt
}

fn recent_news_prompt(
    client: &Client,
    request: TriviaRequest,
    cached: &[TriviaItem],
) -> Result<String, String> {
    let category = request.news_category.ok_or_else(|| {
        "Recent News request missing category. Return to menu and select a category.".to_string()
    })?;

    let selected = fetch_recent_news_items(client, category)?;

    if selected.len() < 5 {
        return Err(format!(
            "{} did not have enough usable stories. Try another category.",
            category.label()
        ));
    }

    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut context = String::new();
    for (idx, item) in selected.iter().enumerate() {
        let summary = item
            .description
            .as_deref()
            .unwrap_or("No summary available");
        let url = item.url.as_deref().unwrap_or("(no url)");
        context.push_str(&format!(
            "{}. [{}] {} - {} ({})\n",
            idx + 1,
            item.source,
            item.title,
            summary,
            url
        ));
    }

    let mut prompt = format!(
        "Task: Generate exactly 5 funny 'Wait Wait... Don't Tell Me!' style trivia questions about current events.
Today: {}.
Category: {}.

Use ONLY the stories listed below.
Do not use outside knowledge.
Do not invent details.
Write one trivia item per story.

Stories:
{}

Output constraints:
- Return ONLY a JSON array of exactly 5 objects with fields 'question' and 'answer'.
- No markdown, no prose, no extra fields.",
        today,
        category.label(),
        context
    );
    prompt.push_str(&previously_asked_block(cached));
    Ok(prompt)
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
        TriviaSubject::RecentNews => {
            let cat = request
                .news_category
                .map(|c| c.label().to_ascii_lowercase().replace(' ', "-"))
                .unwrap_or_else(|| "all".to_string());
            format!("recent-news-{}.json", cat)
        }
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
        TriviaSubject::RecentNews => return None,
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

// ---------------------------------------------------------------------------
// News article cache — fetch once from NewsAPI, cache to disk, reuse until stale
// ---------------------------------------------------------------------------

const NEWS_CACHE_MAX_AGE_SECS: u64 = 24 * 60 * 60; // 24 hours

fn news_cache_path(category: NewsCategory) -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME is not set; unable to build news cache path".to_string())?;
    let mut dir = PathBuf::from(home);
    dir.push(".config");
    dir.push("games-kiosk");
    dir.push("trivia-cache");
    let cat = category.label().to_ascii_lowercase().replace(' ', "-");
    dir.push(format!("news-articles-{}.json", cat));
    Ok(dir)
}

fn load_news_cache(path: &PathBuf) -> Option<Vec<NewsContextItem>> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = modified.elapsed().ok()?;
    if age.as_secs() > NEWS_CACHE_MAX_AGE_SECS {
        return None; // stale
    }
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Vec<NewsContextItem>>(&contents).ok()
}

fn persist_news_cache(path: &PathBuf, items: &[NewsContextItem]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(items) {
        let _ = fs::write(path, json);
    }
}

fn fetch_recent_news_items(
    client: &Client,
    category: NewsCategory,
) -> Result<Vec<NewsContextItem>, String> {
    let path = news_cache_path(category)?;

    // Use cached articles if fresh
    if let Some(articles) = load_news_cache(&path) {
        let mut picked = articles;
        let mut rng = rand::rng();
        picked.shuffle(&mut rng);
        picked.truncate(5);
        return Ok(picked);
    }

    // Not cached or stale — fetch from NewsAPI (page 1 only, up to 100)
    let news_key = std::env::var("NEWS_API_KEY").map_err(|_| {
        "NEWS_API_KEY is not set for Recent News mode. Choose Bible or set NEWS_API_KEY in trivia.env.".to_string()
    })?;

    let sources_csv = US_TOP_SOURCE_IDS.join(",");
    let mut merged = Vec::new();

    let us_query: Vec<(&str, String)> = vec![
        ("pageSize", "100".to_string()),
        ("page", "1".to_string()),
        ("sources", sources_csv),
        ("q", category.search_query().to_string()),
        ("sortBy", "publishedAt".to_string()),
        ("language", "en".to_string()),
    ];
    merged.extend(fetch_everything(client, &news_key, &us_query)?);

    let mx_query: Vec<(&str, String)> = vec![
        ("pageSize", "100".to_string()),
        ("page", "1".to_string()),
        (
            "q",
            format!(
                "({}) AND (Mexico OR Mexicano OR mexicana OR CDMX OR Monterrey OR Guadalajara)",
                category.search_query()
            ),
        ),
        ("sortBy", "publishedAt".to_string()),
        ("language", "es".to_string()),
    ];
    merged.extend(fetch_everything(client, &news_key, &mx_query)?);

    // Dedup by title
    let mut dedup = Vec::new();
    let mut seen_titles = HashSet::new();
    for item in merged {
        let key = item.title.to_lowercase();
        if seen_titles.insert(key) {
            dedup.push(item);
        }
    }

    if dedup.len() < 5 {
        return Err(format!(
            "Not enough usable articles for {}. Try another category.",
            category.label()
        ));
    }

    // Persist to disk
    persist_news_cache(&path, &dedup);

    // Return 5 random from the full set
    let mut rng = rand::rng();
    dedup.shuffle(&mut rng);
    Ok(dedup.into_iter().take(5).collect())
}

fn fetch_everything(
    client: &Client,
    news_key: &str,
    query: &[(&str, String)],
) -> Result<Vec<NewsContextItem>, String> {
    let res = client
        .get("https://newsapi.org/v2/everything")
        .query(query)
        .header("X-Api-Key", news_key)
        .send()
        .map_err(|e| format!("Failed to fetch news articles: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("News API everything error ({}): {}", status, body));
    }

    let parsed: NewsApiResponse = res
        .json()
        .map_err(|e| format!("News API parse error: {}", e))?;

    if parsed.status != "ok" {
        return Err(format!(
            "News API returned status '{}': {}",
            parsed.status,
            parsed
                .message
                .unwrap_or_else(|| "Unknown news API error".to_string())
        ));
    }

    let mut out = Vec::new();
    for article in parsed.articles {
        if article.title.trim().is_empty() || article.title == "[Removed]" {
            continue;
        }

        let source = article
            .source
            .name
            .or(article.source.id)
            .unwrap_or_else(|| "Unknown source".to_string());

        let description = article
            .description
            .map(|s| s.trim().chars().take(200).collect::<String>());

        out.push(NewsContextItem {
            source,
            title: article.title,
            description,
            url: article.url,
        });
    }

    Ok(out)
}
