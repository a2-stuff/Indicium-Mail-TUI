//! Snippet builder: collapses whitespace and truncates at a word boundary.

/// Build a snippet from `text`, collapsing whitespace and truncating at the
/// last word boundary at or before `max` characters.
pub fn make_snippet(text: &str, max: usize) -> String {
    let mut collapsed = String::with_capacity(text.len().min(max + 16));
    let mut last_was_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_ws && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            last_was_ws = true;
        } else {
            collapsed.push(ch);
            last_was_ws = false;
        }
    }
    let trimmed = collapsed.trim_end().to_string();
    if trimmed.chars().count() <= max {
        return trimmed;
    }
    let mut end = 0usize;
    let mut count = 0usize;
    for (idx, _) in trimmed.char_indices() {
        if count >= max {
            end = idx;
            break;
        }
        count += 1;
    }
    if end == 0 {
        end = trimmed.len();
    }
    let slice = &trimmed[..end];
    if let Some(space) = slice.rfind(char::is_whitespace) {
        slice[..space].trim_end().to_string()
    } else {
        slice.to_string()
    }
}
