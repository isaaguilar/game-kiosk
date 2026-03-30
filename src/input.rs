use crate::app::AppState;

/// Platform-agnostic key actions understood by the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    Up,
    Down,
    Confirm, // Enter / Space
    Back,    // Escape / Backspace / Q
}

pub enum Action {
    None,
    Quit,
    Launch,
}

pub fn handle_keys(keys: &[AppKey], state: &mut AppState) -> Action {
    for &key in keys {
        match state {
            AppState::GameSelect { .. } => match key {
                AppKey::Up => state.menu_up(),
                AppKey::Down => state.menu_down(),
                AppKey::Confirm => {
                    state.select_game();
                    return Action::Launch;
                }
                AppKey::Back => state.open_quit_prompt(),
            },
            AppState::QuitPrompt { .. } => match key {
                AppKey::Confirm => return Action::Quit,
                AppKey::Back => state.close_quit_prompt(),
                _ => {}
            },
            AppState::LaunchGame { .. } => {}
        }
    }
    Action::None
}
