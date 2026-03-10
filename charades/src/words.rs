use rand::seq::SliceRandom;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// Loads, normalizes, and returns prompts from a string of newline-delimited lines.
fn parse_prompts(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn difficulty_filename(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "easy.txt",
        Difficulty::Medium => "medium.txt",
        Difficulty::Hard => "hard.txt",
    }
}

fn asset_path_from_exe(filename: &str) -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("failed to resolve executable path: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "failed to resolve executable directory".to_string())?;

    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(filename),
        exe_dir.join("charades-assets").join(filename),
        exe_dir.join("assets").join(filename),
        exe_dir.join("..").join("assets").join(filename),
        exe_dir.join("..").join("..").join("assets").join(filename),
        exe_dir
            .join("..")
            .join("..")
            .join("..")
            .join("assets")
            .join(filename),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(format!(
        "asset file '{}' not found relative to executable {}; checked: {}",
        filename,
        exe.display(),
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn read_prompts_from_asset(path: &Path, difficulty: Difficulty) -> Vec<String> {
    let raw = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "Failed to load {:?} prompts from {}: {}",
            difficulty,
            path.display(),
            e
        )
    });

    let prompts = parse_prompts(&raw);
    assert!(
        !prompts.is_empty(),
        "No prompts loaded for difficulty {:?} from {}",
        difficulty,
        path.display()
    );
    prompts
}

/// Holds the full pool for a difficulty and an in-progress shuffled queue.
pub struct WordQueue {
    pool: Vec<String>,
    queue: VecDeque<String>,
}

impl WordQueue {
    pub fn new(mut pool: Vec<String>) -> Self {
        assert!(!pool.is_empty(), "word pool must not be empty");
        let mut rng = rand::rng();
        pool.shuffle(&mut rng);
        let queue: VecDeque<String> = pool.iter().cloned().collect();
        Self { pool, queue }
    }

    /// Return the next prompt. Reshuffles automatically when the queue is exhausted.
    pub fn next(&mut self) -> String {
        if self.queue.is_empty() {
            let mut rng = rand::rng();
            self.pool.shuffle(&mut rng);
            self.queue = self.pool.iter().cloned().collect();
        }
        self.queue.pop_front().expect("pool is non-empty")
    }
}

pub fn load_queue(difficulty: Difficulty) -> WordQueue {
    let filename = difficulty_filename(difficulty);
    let path = asset_path_from_exe(filename).unwrap_or_else(|e| {
        panic!(
            "Failed to resolve asset path for {:?} ({}): {}",
            difficulty, filename, e
        )
    });
    let pool = read_prompts_from_asset(&path, difficulty);

    WordQueue::new(pool)
}
