use crate::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    Up,
    Down,
    Left,
    Right,
    Confirm,      // Enter / Space
    Back,         // Escape / Backspace / Q
    SeeQuestion,  // S – shows the current question from answer/explanation screens
}

pub enum Action {
    None,
    Quit,
}

pub fn handle_keys(
    keys: &[AppKey],
    state: &mut AppState,
    explanation_page_info: Option<(usize, usize)>,
) -> Action {
    for &key in keys {
        match state {
            AppState::SubjectMenu { .. } => match key {
                AppKey::Up => state.move_menu_up(),
                AppKey::Down | AppKey::SeeQuestion => state.move_menu_down(),
                AppKey::Left => state.move_menu_left(),
                AppKey::Right => state.move_menu_right(),
                AppKey::Confirm => state.confirm_menu_selection(),
                AppKey::Back => return Action::Quit,
            },
            AppState::NewsCategoryMenu { .. } => match key {
                AppKey::Up => state.move_menu_up(),
                AppKey::Down | AppKey::SeeQuestion => state.move_menu_down(),
                AppKey::Left => state.move_menu_left(),
                AppKey::Right => state.move_menu_right(),
                AppKey::Confirm => state.confirm_menu_selection(),
                AppKey::Back => state.return_to_subject_menu(),
            },
            AppState::Loading { .. } => match key {
                AppKey::Back => {
                    state.return_to_subject_menu();
                }
                _ => {}
            },
            AppState::Error { request, .. } => match key {
                AppKey::Confirm | AppKey::Back => {
                    if request.subject == crate::app::TriviaSubject::RecentNews {
                        state.return_to_news_category_menu();
                    } else {
                        state.return_to_subject_menu();
                    }
                }
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
                AppKey::Left | AppKey::Right | AppKey::Up | AppKey::Down => {
                    state.move_answer_down(); // toggle between two buttons
                }
                AppKey::Confirm => state.confirm_answer_selection(),
                AppKey::SeeQuestion => state.return_to_question(),
                AppKey::Back => return Action::Quit,
            },
            AppState::ExplanationLoading { .. } => match key {
                AppKey::Back | AppKey::SeeQuestion => state.return_to_question(),
                _ => {}
            },
            AppState::Explanation { .. } => match key {
                AppKey::Confirm => {
                    if let Some((total, visible)) = explanation_page_info {
                        state.explanation_page_forward(total, visible);
                    } else {
                        state.return_to_answer();
                    }
                }
                AppKey::SeeQuestion | AppKey::Back => state.return_to_answer(),
                _ => {}
            },
        }
    }
    Action::None
}
