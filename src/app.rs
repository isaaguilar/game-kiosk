#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GameSelection {
    Charades,
    Pictionary,
}

impl GameSelection {
    pub fn label(self) -> &'static str {
        match self {
            GameSelection::Charades => "Charades",
            GameSelection::Pictionary => "Pictionary",
        }
    }

    pub fn bin_name(self) -> &'static str {
        match self {
            GameSelection::Charades => "charades",
            GameSelection::Pictionary => "pictionary",
        }
    }
}

pub const MENU_ITEMS: [GameSelection; 2] = [GameSelection::Charades, GameSelection::Pictionary];

pub enum AppState {
    GameSelect { selected: usize },
    QuitPrompt { selected: usize },
    LaunchGame { game: GameSelection, selected: usize },
}

impl AppState {
    pub fn initial() -> Self {
        AppState::GameSelect { selected: 0 }
    }

    pub fn menu_up(&mut self) {
        if let AppState::GameSelect { selected } = self {
            *selected = selected.checked_sub(1).unwrap_or(MENU_ITEMS.len() - 1);
        }
    }

    pub fn menu_down(&mut self) {
        if let AppState::GameSelect { selected } = self {
            *selected = (*selected + 1) % MENU_ITEMS.len();
        }
    }

    pub fn select_game(&mut self) {
        if let AppState::GameSelect { selected } = self {
            *self = AppState::LaunchGame {
                game: MENU_ITEMS[*selected],
                selected: *selected,
            };
        }
    }

    pub fn open_quit_prompt(&mut self) {
        if let AppState::GameSelect { selected } = self {
            *self = AppState::QuitPrompt {
                selected: *selected,
            };
        }
    }

    pub fn close_quit_prompt(&mut self) {
        if let AppState::QuitPrompt { selected } = self {
            *self = AppState::GameSelect {
                selected: *selected,
            };
        }
    }

    pub fn restore_selection(&mut self, selected: usize) {
        *self = AppState::GameSelect { selected };
    }

    pub fn current_selection(&self) -> usize {
        match self {
            AppState::GameSelect { selected } | AppState::QuitPrompt { selected } => *selected,
            AppState::LaunchGame { selected, .. } => *selected,
        }
    }
}
