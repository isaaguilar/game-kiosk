use chrono::Local;
use rand::seq::SliceRandom;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriviaItem {
    pub question: String,
    pub answer: String,
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
                let cat = self
                    .news_category
                    .map(|c| c.label())
                    .unwrap_or("All");
                format!("Recent News ({})", cat)
            }
            TriviaSubject::FunFacts => "Fun Facts".to_string(),
        }
    }

    pub fn loading_status(self) -> String {
        match self.subject {
            TriviaSubject::Bible => "Loading Bible questions".to_string(),
            TriviaSubject::RecentNews => {
                let cat = self
                    .news_category
                    .map(|c| c.label())
                    .unwrap_or("All");
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

fn fetch_trivia(request: TriviaRequest) -> Result<Vec<TriviaItem>, String> {
    let path = cache_file_path(request)?;
    let mut cached = load_trivia_cache(&path);

    // If cache is full, serve randomly from it
    if cached.len() >= cache_threshold(request.subject) {
        let mut rng = rand::rng();
        cached.shuffle(&mut rng);
        return Ok(cached.into_iter().take(5).collect());
    }

    let api_key = std::env::var("GOOGLE_API_KEY")
        .map_err(|_| "GOOGLE_API_KEY environment variable is not set. Export it in your shell: export GOOGLE_API_KEY='...'".to_string())?;

    let client = Client::builder()
        .user_agent("Kiosk-Trivia/1.0")
        .build()
        .unwrap_or_else(|_| Client::new());

    let prompt = match request.subject {
        TriviaSubject::Bible => bible_prompt(&cached),
        TriviaSubject::RecentNews => recent_news_prompt(&client, request, &cached)?,
        TriviaSubject::FunFacts => fun_facts_prompt(&cached),
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite-preview:generateContent?key={}",
        api_key
    );

    let payload = serde_json::json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }]
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
        .get(0)
        .ok_or("No candidates in response (Check safety filters/billing)")?;
    let part = candidate
        .content
        .parts
        .get(0)
        .ok_or("No text in response part")?;
    let text = &part.text;

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
    let parsed = serde_json::from_str::<Vec<TriviaItem>>(clean_json)
        .map_err(|e| format!("JSON parse error: {} | Body: {}", e, clean_json))?;

    if parsed.len() < 5 {
        return Err("Model returned fewer than 5 trivia items. Please retry.".to_string());
    }

    // Deduplicate against cache
    let seen_keys: HashSet<String> = cached.iter().map(|i| normalize_question(&i.question)).collect();
    let mut fresh: Vec<TriviaItem> = Vec::new();
    for item in parsed {
        let key = normalize_question(&item.question);
        if key.is_empty() || seen_keys.contains(&key) {
            continue;
        }
        fresh.push(item);
    }

    if fresh.is_empty() {
        return Err(
            "All generated questions were duplicates. Press Enter to request another batch."
                .to_string(),
        );
    }

    // Append fresh items to cache and persist
    cached.extend(fresh.clone());
    persist_trivia_cache(&path, &cached);

    // Return up to 5 fresh items
    fresh.truncate(5);
    Ok(fresh)
}

fn previously_asked_block(cached: &[TriviaItem]) -> String {
    if cached.is_empty() {
        return String::new();
    }
    let n = cached.len().min(200);
    let start = cached.len().saturating_sub(n);
    let mut block = String::from("\n\nDo NOT repeat any of the following previously asked questions:\n");
    for item in &cached[start..] {
        block.push_str("- ");
        block.push_str(&item.question);
        block.push('\n');
    }
    block
}

fn bible_prompt(cached: &[TriviaItem]) -> String {
    let mut prompt = "Task: Generate exactly 12 funny 'Wait Wait... Don't Tell Me!' style Bible trivia questions.
Topic constraints:
- Use stories and details from the King James Bible and related biblical context.
- Mix well-known and lesser-known passages.
- Keep questions accessible but not childish.

Output constraints:
- Return ONLY a JSON array of exactly 12 objects with fields 'question' and 'answer'.
- No markdown, no prose, no code fences."
        .to_string();
    prompt.push_str(&previously_asked_block(cached));
    prompt
}

fn fun_facts_prompt(cached: &[TriviaItem]) -> String {
    let mut prompt = "Task: Generate exactly 12 fun, kid-friendly, real-world trivia questions with concise answers.
Theme constraints:
- Focus on true fun facts that are interesting, weird, bizarre, surprising, or funny.
- Keep content generally appropriate for kids and families.
- Use facts from science, animals, geography, inventions, language, and history.
- Avoid violent, sexual, or disturbing topics.
- Facts must be real; do not invent or exaggerate.

Output constraints:
- Return ONLY a JSON array of exactly 12 objects with fields 'question' and 'answer'.
- No markdown, no prose, no extra fields."
        .to_string();
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

fn load_trivia_cache(path: &PathBuf) -> Vec<TriviaItem> {
    if let Ok(contents) = fs::read_to_string(path) {
        serde_json::from_str::<Vec<TriviaItem>>(&contents).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn persist_trivia_cache(path: &PathBuf, items: &[TriviaItem]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(items) {
        let _ = fs::write(path, json);
    }
}

fn normalize_question(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
            parsed.message.unwrap_or_else(|| "Unknown news API error".to_string())
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
