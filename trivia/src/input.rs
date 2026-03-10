use crate::app::AppState;

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
            AppState::Loading { .. } => match key {
                AppKey::Back => return Action::Quit,
                _ => {}
            },
            AppState::Error(_) => match key {
                AppKey::Confirm => state.request_reload(),
                AppKey::Back => return Action::Quit,
                _ => {}
            },
            AppState::Ready { .. } => match key {
                AppKey::Confirm => state.start_game(),
                AppKey::Back => return Action::Quit,
                _ => {}
            },
            AppState::Question { .. } => match key {
                AppKey::Confirm => state.next_step(),
                AppKey::Back => return Action::Quit,
                _ => {}
            },
            AppState::Answer { .. } => match key {
                AppKey::Confirm => state.next_step(),
                AppKey::Back => return Action::Quit,
                _ => {}
            },
        }
    }
    Action::None
}
