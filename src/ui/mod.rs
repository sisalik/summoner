pub mod status_bar;
pub mod session_view;
pub mod selection;
pub mod smart;
pub mod links;
pub mod dashboard;
pub mod dashboard_nav;
pub mod dir_picker;
pub mod session_switcher;

/// Delete the last word from a query string (Ctrl+Backspace / Ctrl+W).
/// Words end at whitespace or `/`, so path components delete one at a time.
pub fn delete_last_word(query: &mut String) {
    let is_sep = |c: char| c.is_whitespace() || c == '/';
    while query.chars().next_back().is_some_and(is_sep) {
        query.pop();
    }
    while query.chars().next_back().is_some_and(|c| !is_sep(c)) {
        query.pop();
    }
}
