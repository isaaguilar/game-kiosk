use crate::words::{load_queue, WordQueue};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuSelection {
    Start,
}

impl MenuSelection {
    pub fn label(self) -> &'static str {
        "Start"
    }
}

pub const MENU_ITEMS: [MenuSelection; 1] = [MenuSelection::Start];

pub enum AppState {
    Menu {
        selected: usize,
    },
    Playing {
        current_prompt: String,
        queue: WordQueue,
    },
}

impl AppState {
    pub fn initial() -> Self {
        AppState::Menu { selected: 0 }
    }

    /// Move selection up (wraps).
    pub fn menu_up(&mut self) {
        if let AppState::Menu { selected } = self {
            *selected = selected.checked_sub(1).unwrap_or(MENU_ITEMS.len() - 1);
        }
    }

    /// Move selection down (wraps).
    pub fn menu_down(&mut self) {
        if let AppState::Menu { selected } = self {
            *selected = (*selected + 1) % MENU_ITEMS.len();
        }
    }

    /// Start game with currently selected difficulty.
    pub fn start_game(&mut self) {
        if let AppState::Menu { selected } = self {
            let _item = MENU_ITEMS[*selected];
            let mut queue = load_queue();
            let first = queue.next();
            *self = AppState::Playing {
                current_prompt: first,
                queue,
            };
        }
    }

    /// Advance to next prompt while in Playing state.
    pub fn next_prompt(&mut self) {
        if let AppState::Playing {
            current_prompt,
            queue,
            ..
        } = self
        {
            *current_prompt = queue.next();
        }
    }

    /// Return to menu.
    pub fn back_to_menu(&mut self) {
        *self = AppState::Menu { selected: 0 };
    }
}
