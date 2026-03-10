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
}

pub fn handle_keys(keys: &[AppKey], state: &mut AppState) -> Action {
    for &key in keys {
        match state {
            AppState::Menu { .. } => match key {
                AppKey::Up => state.menu_up(),
                AppKey::Down => state.menu_down(),
                AppKey::Confirm => state.start_game(),
                AppKey::Back => return Action::Quit,
            },
            AppState::Playing { .. } => match key {
                AppKey::Confirm => state.next_prompt(),
                AppKey::Back => state.back_to_menu(),
                _ => {}
            },
        }
    }
    Action::None
}

