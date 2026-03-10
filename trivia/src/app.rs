use std::sync::mpsc;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use chrono::Local;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriviaItem {
    pub question: String,
    pub answer: String,
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

pub enum AppState {
    Loading {
        status: String,
        started_at: Instant,
    },
    Error(String),
    Ready {
        items: Vec<TriviaItem>,
        current_idx: usize,
    },
    Question {
        items: Vec<TriviaItem>,
        current_idx: usize,
        start_time: Instant,
        duration: Duration,
    },
    Answer {
        items: Vec<TriviaItem>,
        current_idx: usize,
    },
}

impl AppState {
    pub fn initial() -> Self {
        AppState::Loading {
            status: "Initializing...".to_string(),
            started_at: Instant::now(),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, AppState::Loading { .. })
    }

    pub fn apply_load_result(&mut self, result: Result<Vec<TriviaItem>, String>) {
        match result {
            Ok(items) => *self = AppState::Ready { items, current_idx: 0 },
            Err(e) => *self = AppState::Error(e),
        }
    }

    pub fn start_game(&mut self) {
        if let AppState::Ready { items, current_idx } = self {
            *self = AppState::Question {
                items: items.clone(),
                current_idx: *current_idx,
                start_time: Instant::now(),
                duration: Duration::from_secs(60),
            };
        }
    }

    pub fn next_step(&mut self) {
        match self {
            AppState::Question { items, current_idx, .. } => {
                *self = AppState::Answer {
                    items: items.clone(),
                    current_idx: *current_idx,
                };
            }
            AppState::Answer { items, current_idx } => {
                let next_idx = *current_idx + 1;
                if next_idx < items.len() {
                    *self = AppState::Question {
                        items: items.clone(),
                        current_idx: next_idx,
                        start_time: Instant::now(),
                        duration: Duration::from_secs(60),
                    };
                } else {
                    *self = AppState::Loading {
                        status: "Loading new questions...".to_string(),
                        started_at: Instant::now(),
                    };
                }
            }
            _ => {}
        }
    }

    pub fn request_reload(&mut self) {
        *self = AppState::Loading {
            status: "Loading questions...".to_string(),
            started_at: Instant::now(),
        };
    }

    pub fn update(&mut self) {}
}

/// Spawn a background thread to fetch trivia questions.
/// Returns a `Receiver` that the main loop polls each frame.
pub fn start_background_load() -> mpsc::Receiver<Result<Vec<TriviaItem>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = fetch_trivia();
        let _ = tx.send(result);
    });
    rx
}

fn fetch_trivia() -> Result<Vec<TriviaItem>, String> {
    let api_key = std::env::var("GOOGLE_API_KEY")
        .map_err(|_| "GOOGLE_API_KEY environment variable is not set. Export it in your shell: export GOOGLE_API_KEY='...'".to_string())?;

    let today = Local::now().format("%Y-%m-%d").to_string();
    let prompt = format!(
        "Generate 5 funny 'Wait Wait... Don't Tell Me!' style trivia questions about recent news or fun facts from the past week (today is {}). \
         Each should be a short question and a concise answer. \
         Format the output as a JSON array of objects with 'question' and 'answer' fields. \
         Return ONLY the raw JSON array, no other text or markdown formatting.",
        today
    );

    let url = format!(
        "https://generativelanguage.googleapis.com/v1/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let client = Client::builder()
        .user_agent("Kiosk-Trivia/1.0")
        .build()
        .unwrap_or_else(|_| Client::new());

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
        return Err(format!("API error ({}): {}", status, err_text));
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

    let start = text.find('[').ok_or_else(|| format!("No JSON array found in response: {}", text))?;
    let end = text.rfind(']').ok_or_else(|| format!("No JSON array found in response: {}", text))?;
    if end <= start {
        return Err(format!("No JSON array found in response: {}", text));
    }

    let clean_json = &text[start..=end];
    serde_json::from_str::<Vec<TriviaItem>>(clean_json)
        .map_err(|e| format!("JSON parse error: {} | Body: {}", e, clean_json))
}
