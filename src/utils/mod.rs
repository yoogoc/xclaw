pub fn truncate_preview(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Walk backwards from max_bytes to find a valid char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut result = format!("{}...", &s[..end]);

    // Re-close <tool_output> if truncation cut through the closing tag.
    if s.starts_with("<tool_output") && !result.ends_with("</tool_output>") {
        result.push_str("\n</tool_output>");
    }

    result
}
