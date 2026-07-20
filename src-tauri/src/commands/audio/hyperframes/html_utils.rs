//! Low-level HTML tag and attribute manipulation utilities.
//!
//! These functions operate on raw HTML tag strings (between `<` and `>`).
//! They are intentionally generic — no Hyperframes-specific logic lives here.

/// Extract the value of an attribute from an HTML tag string.
pub fn extract_attr_value<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let search = format!("{}=", attr_name);
    let attr_pos = tag.find(&search)?;
    let after_eq = attr_pos + search.len();
    let rest = &tag[after_eq..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_start = after_eq + 1;
    let value_end_rel = tag[value_start..].find(quote)?;
    Some(&tag[value_start..value_start + value_end_rel])
}

/// Replace an attribute's value in an HTML tag string, or add it if missing.
pub fn replace_or_add_attr(tag: &str, attr_name: &str, new_value: &str) -> String {
    let search = format!("{}=", attr_name);
    if let Some(attr_pos) = tag.find(&search) {
        let after_eq = attr_pos + search.len();
        let rest = &tag[after_eq..];
        let quote_char = rest.chars().next();
        if let Some(quote) = quote_char {
            if quote == '"' || quote == '\'' {
                let value_start = after_eq + 1;
                if let Some(value_end_rel) = tag[value_start..].find(quote) {
                    let value_end = value_start + value_end_rel;
                    return format!(
                        "{}{}=\"{}\"{}",
                        &tag[..attr_pos],
                        attr_name,
                        new_value,
                        &tag[value_end + 1..]
                    );
                }
            }
        }
        tag.to_string()
    } else if let Some(close_pos) = tag.rfind('>') {
        format!(
            "{} {}=\"{}\"{}",
            &tag[..close_pos],
            attr_name,
            new_value,
            &tag[close_pos..]
        )
    } else {
        tag.to_string()
    }
}

/// Find the byte range (tag_start..=tag_end) of the opening tag that carries
/// `data-composition-id` as a real HTML attribute — not a CSS selector occurrence
/// like `[data-composition-id="x"]` inside a `<style>` block.
pub fn find_composition_tag_range(html: &str) -> Option<(usize, usize)> {
    let needle = "data-composition-id";
    let mut search_from = 0;

    while let Some(rel) = html[search_from..].find(needle) {
        let pos = search_from + rel;
        search_from = pos + needle.len();

        let prev_char = html[..pos].chars().next_back();
        if !matches!(prev_char, Some(c) if c.is_whitespace()) {
            continue;
        }

        let Some(tag_start) = html[..pos].rfind('<') else {
            continue;
        };
        let after_lt = html[tag_start + 1..].chars().next();
        if !matches!(after_lt, Some(c) if c.is_ascii_alphabetic()) {
            continue;
        }

        if html[tag_start..pos].contains('>') {
            continue;
        }

        let Some(end_rel) = html[tag_start..].find('>') else {
            continue;
        };
        return Some((tag_start, tag_start + end_rel));
    }

    None
}

/// Set (or replace) `data-duration` to `duration` within the tag at `tag_start..=tag_end`.
/// Returns the modified full HTML and whether a change was made.
pub fn set_data_duration_in_tag(
    html: &str,
    tag_start: usize,
    tag_end: usize,
    duration: f64,
) -> (String, bool) {
    let tag_content = &html[tag_start..=tag_end];

    if let Some(attr_pos) = tag_content.find("data-duration=") {
        let after_eq = attr_pos + "data-duration=".len();
        if let Some(quote) = tag_content[after_eq..].chars().next() {
            if quote == '"' || quote == '\'' {
                let value_start = after_eq + 1;
                if let Some(value_end_rel) = tag_content[value_start..].find(quote) {
                    let current_val = &tag_content[value_start..value_start + value_end_rel];
                    let needs_fix = current_val
                        .parse::<f64>()
                        .map(|v| (v - duration).abs() > 0.1)
                        .unwrap_or(true);
                    if !needs_fix {
                        return (html.to_string(), false);
                    }
                    let new_attr = format!("data-duration=\"{:.3}\"", duration);
                    let attr_end = value_start + value_end_rel + 1;
                    let new_tag = format!(
                        "{}{}{}",
                        &tag_content[..attr_pos],
                        new_attr,
                        &tag_content[attr_end..]
                    );
                    let mut result = html.to_string();
                    result.replace_range(tag_start..=tag_end, &new_tag);
                    return (result, true);
                }
            }
        }
        (html.to_string(), false)
    } else {
        let addition = format!(" data-duration=\"{:.3}\"", duration);
        let mut result = html.to_string();
        result.insert_str(tag_end, &addition);
        (result, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_attr_double_quotes() {
        let tag = r#"<div data-start="3.5" class="clip">"#;
        assert_eq!(extract_attr_value(tag, "data-start"), Some("3.5"));
        assert_eq!(extract_attr_value(tag, "class"), Some("clip"));
    }

    #[test]
    fn test_extract_attr_single_quotes() {
        let tag = "<div data-start='7.2'>";
        assert_eq!(extract_attr_value(tag, "data-start"), Some("7.2"));
    }

    #[test]
    fn test_extract_attr_missing() {
        let tag = r#"<div class="clip">"#;
        assert_eq!(extract_attr_value(tag, "data-start"), None);
    }

    #[test]
    fn test_replace_or_add_attr_existing() {
        let tag = r#"<div data-start="0" data-duration="3">"#;
        let result = replace_or_add_attr(tag, "data-duration", "5.000");
        assert!(result.contains(r#"data-duration="5.000""#));
        assert!(result.contains(r#"data-start="0""#));
    }

    #[test]
    fn test_replace_or_add_attr_missing() {
        let tag = r#"<div class="clip">"#;
        let result = replace_or_add_attr(tag, "data-start", "1.500");
        assert!(result.contains(r#"data-start="1.500""#));
    }

    #[test]
    fn test_find_composition_tag_range() {
        let html = r#"<html data-composition-id="test" data-width="1920">"#;
        let range = find_composition_tag_range(html);
        assert!(range.is_some());
        let (start, end) = range.unwrap();
        assert!(html[start..=end].contains("data-composition-id"));
    }

    #[test]
    fn test_find_composition_tag_ignores_css_selector() {
        let html =
            r#"<style>[data-composition-id="x"] { }</style><div data-composition-id="real">"#;
        let range = find_composition_tag_range(html);
        assert!(range.is_some());
        let (start, _end) = range.unwrap();
        // Should point to <div>, not the CSS selector
        assert!(html[start..].starts_with("<div"));
    }

    #[test]
    fn test_set_data_duration_replace() {
        let html = r#"<html data-duration="100" data-width="1920">"#;
        let (result, changed) = set_data_duration_in_tag(html, 0, html.len() - 1, 8.5);
        assert!(changed);
        assert!(result.contains(r#"data-duration="8.500""#));
    }

    #[test]
    fn test_set_data_duration_add_missing() {
        let html = r#"<html data-width="1920">"#;
        let (result, changed) = set_data_duration_in_tag(html, 0, html.len() - 1, 5.0);
        assert!(changed);
        assert!(result.contains(r#"data-duration="5.000""#));
    }

    #[test]
    fn test_set_data_duration_already_correct() {
        let html = r#"<html data-duration="8.5" data-width="1920">"#;
        let (_, changed) = set_data_duration_in_tag(html, 0, html.len() - 1, 8.5);
        assert!(!changed);
    }
}
