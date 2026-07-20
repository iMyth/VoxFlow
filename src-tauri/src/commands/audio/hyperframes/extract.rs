//! Extract HTML from LLM responses.
//!
//! The LLM is instructed to output raw HTML, but may still wrap it in
//! explanations, code fences, or thinking blocks. This module handles
//! all extraction strategies with graceful fallback.

/// Extract the HTML content from the LLM response.
///
/// Strategies tried in order:
/// 1. Strip `<think>` blocks, then check for raw HTML at start
/// 2. Extract from markdown code fences
/// 3. Search for embedded HTML anywhere in the text
pub fn extract_html(response: &str) -> Result<String, String> {
    let cleaned = strip_thinking_blocks(response);
    let trimmed = cleaned.trim();

    if let Some(html) = try_extract_raw_html(trimmed) {
        return Ok(html);
    }

    if let Some(html) = try_extract_from_code_fences(trimmed) {
        return Ok(html);
    }

    if let Some(html) = try_extract_embedded_html(trimmed) {
        return Ok(html);
    }

    let preview: String = trimmed.chars().take(300).collect();
    Err(format!(
        "LLM response does not contain valid HTML.\n\nResponse preview (first 300 chars):\n{preview}\n\n\
         Hint: Ensure the model outputs raw HTML starting with <!DOCTYPE html> or <html>."
    ))
}

/// Strip `<think>...` blocks from the response.
/// Models with thinking mode enabled may prepend reasoning in these tags.
fn strip_thinking_blocks(text: &str) -> String {
    let mut result = text.to_string();
    loop {
        let lower = result.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            break;
        };
        let end_tag = "</think>";
        if let Some(end_pos) = lower[start..].find(end_tag) {
            let remove_end = start + end_pos + end_tag.len();
            result = format!("{}{}", &result[..start], &result[remove_end..]);
        } else {
            // Opening  without closing — remove from start to end of text
            result = result[..start].to_string();
            break;
        }
    }
    result
}

/// Try to extract HTML that starts at the beginning of the text.
fn try_extract_raw_html(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("<!doctype") || lower.starts_with("<html") {
        Some(text.trim().to_string())
    } else {
        None
    }
}

/// Try to extract HTML from markdown code fences.
fn try_extract_from_code_fences(text: &str) -> Option<String> {
    let fence_start = text.find("```")?;
    let after_fence_start = &text[fence_start + 3..];

    let nl_pos = after_fence_start.find('\n')?;
    let content_start = fence_start + 3 + nl_pos + 1;

    let content = &text[content_start..];
    let content_end = content.rfind("```").unwrap_or(content.len());
    let extracted = content[..content_end].trim();

    let lower = extracted.to_ascii_lowercase();
    if lower.starts_with("<!doctype") || lower.starts_with("<html") {
        Some(extracted.to_string())
    } else {
        None
    }
}

/// Try to extract HTML embedded in explanatory text.
fn try_extract_embedded_html(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let markers = ["<!doctype html>", "<!doctype", "<html"];

    for marker in markers {
        if let Some(start_pos) = lower.find(marker) {
            let html_candidate = &text[start_pos..];

            if let Some(end_rel) = html_candidate.to_ascii_lowercase().rfind("</html>") {
                let extracted = html_candidate[..end_rel + 7].trim();
                return Some(extracted.to_string());
            }

            if html_candidate.len() > 100 {
                let extracted = html_candidate.trim_end_matches("```").trim().to_string();
                if extracted.to_ascii_lowercase().starts_with("<!doctype")
                    || extracted.to_ascii_lowercase().starts_with("<html")
                {
                    return Some(extracted);
                }
            }
        }
    }

    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_html_raw_doctype() {
        let input = "<!DOCTYPE html>\n<html><body>Hello</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
        assert!(result.contains("<body>Hello</body>"));
    }

    #[test]
    fn test_extract_html_with_code_fence() {
        let input = "```html\n<!DOCTYPE html>\n<html><body>Test</body></html>\n```";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
        assert!(result.contains("<body>Test</body>"));
    }

    #[test]
    fn test_extract_html_with_leading_text() {
        let input = "Here is the HTML:\n\n<!DOCTYPE html>\n<html><body>Content</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_extract_html_no_html_found() {
        let input = "This is just plain text with no HTML.";
        let result = extract_html(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("does not contain valid HTML"));
        assert!(err.contains("Response preview"));
    }

    #[test]
    fn test_extract_html_starts_with_html_tag() {
        let input = "<html><head></head><body>Direct</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<html>"));
    }

    #[test]
    fn test_extract_html_whitespace_trimmed() {
        let input = "  \n\n  <!DOCTYPE html>\n<html><body>Trimmed</body></html>  \n  ";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_extract_html_case_insensitive_doctype() {
        let input = "<!doctype HTML>\n<html><body>Lower</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.to_ascii_lowercase().starts_with("<!doctype"));
    }

    #[test]
    fn test_extract_html_with_explanatory_prefix() {
        let input =
            "好的，我来为您生成这个作品。以下是 HTML 代码：\n\n<!DOCTYPE html>\n<html><head><meta charset=\"UTF-8\"></head><body>Content</body></html>";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
        assert!(result.contains("</html>"));
    }

    #[test]
    fn test_extract_html_code_fence_uppercase() {
        let input = "```HTML\n<!DOCTYPE html>\n<html><body>Upper fence</body></html>\n```";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_extract_html_truncated_no_closing_tag() {
        let input =
            "<!DOCTYPE html>\n<html><head><title>Test</title></head><body><div>Long content that gets cut off...";
        let result = extract_html(input).unwrap();
        assert!(result.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_strip_thinking_blocks() {
        let input = "<think>Let me think about this...</think><!DOCTYPE html><html>Test</html>";
        let cleaned = strip_thinking_blocks(input);
        assert!(!cleaned.contains("</think>"));
        assert!(!cleaned.contains("</think>"));
        assert!(cleaned.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_strip_thinking_blocks_unclosed() {
        let input = "<think>This has no closing tag<!DOCTYPE html><html>Test</html>";
        let cleaned = strip_thinking_blocks(input);
        assert!(!cleaned.contains("<think>"));
        // Everything after <think> should be removed
        assert!(cleaned.is_empty() || !cleaned.contains("<!DOCTYPE"));
    }
}
