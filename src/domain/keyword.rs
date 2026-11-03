use unicode_normalization::UnicodeNormalization;

/// True only when the whole comment normalizes to the configured keyword.
/// Punctuation, whitespace, emoji, and other non-alphanumeric separators are ignored.
pub fn is_exact_keyword(comment: &str, keyword: &str) -> bool {
    let comment_tokens = tokens(comment);
    let keyword_tokens = tokens(keyword);

    !keyword_tokens.is_empty() && comment_tokens == keyword_tokens
}

fn tokens(value: &str) -> Vec<String> {
    let normalized: String = value.nfkc().flat_map(char::to_lowercase).collect();
    let mut result = Vec::new();
    let mut current = String::new();

    for character in normalized.chars() {
        if character.is_alphanumeric() {
            current.push(character);
        } else if !current.is_empty() {
            result.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::is_exact_keyword;

    #[test]
    fn matches_keyword_case_insensitively_with_surrounding_punctuation_and_emoji() {
        assert!(is_exact_keyword("БОТ", "бот"));
        assert!(is_exact_keyword(" 🤖 Бот!!! ", "бот"));
    }

    #[test]
    fn rejects_keyword_inside_a_sentence_or_inflected_form() {
        assert!(!is_exact_keyword("давно хотел себе такого бота", "бот"));
        assert!(!is_exact_keyword("бота", "бот"));
        assert!(!is_exact_keyword("бот бот", "бот"));
    }
}
