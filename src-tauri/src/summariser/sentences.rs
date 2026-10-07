//! One sentence splitter for everything FNDR shortens.
//!
//! A sentence ends at `.`, `!` or `?` only when whitespace or the end of the
//! text follows, so file names (`hybrid.rs`), decimals (`0.120`), versions
//! (`v1.5`), paths (`mod.rs:2210`) and ellipses (`SELECT ... FOR`) stay whole. Splitting at the first
//! period anywhere used to cut memories off in the middle of a file name.

const ABBREVIATIONS: [&str; 4] = ["e.g", "i.e", "vs", "approx"];

/// Sentences in order, each trimmed and keeping its end punctuation.
pub fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if !matches!(ch, '.' | '!' | '?') {
            continue;
        }
        let at_boundary = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        // "SELECT ... FOR UPDATE": an ellipsis is a pause, not an ending.
        let in_ellipsis = ch == '.' && text[..idx].ends_with('.') && chars.peek().is_some();
        let after_abbreviation = ch == '.'
            && ABBREVIATIONS.iter().any(|abbreviation| {
                text[start..idx]
                    .to_ascii_lowercase()
                    .ends_with(&format!(" {abbreviation}"))
            });
        if at_boundary && !after_abbreviation && !in_ellipsis {
            let sentence = text[start..idx + ch.len_utf8()].trim();
            if !sentence.is_empty() {
                sentences.push(sentence);
            }
            start = idx + ch.len_utf8();
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

/// The first sentence without its end punctuation, or "" for empty text.
pub fn first_sentence(text: &str) -> &str {
    split_sentences(text)
        .first()
        .map_or("", |sentence| sentence.trim_end_matches(['.', '!', '?']))
}

/// At most `max` sentences, each one finished. A model that runs out of
/// tokens stops mid-sentence; that fragment is dropped when a finished
/// sentence came before it.
pub fn complete_sentences(text: &str, max: usize) -> String {
    let sentences = split_sentences(text);
    let finished = |sentence: &&str| sentence.ends_with(['.', '!', '?']);
    let kept: Vec<&str> = if sentences.iter().any(finished) {
        sentences.into_iter().filter(finished).take(max).collect()
    } else {
        sentences.into_iter().take(max).collect()
    };
    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_decimals_versions_and_paths_stay_whole() {
        assert_eq!(
            split_sentences("Reviewed search/hybrid.rs and mod.rs:2210. Margin rose from 0.120 to 0.432 on v1.5!"),
            vec![
                "Reviewed search/hybrid.rs and mod.rs:2210.",
                "Margin rose from 0.120 to 0.432 on v1.5!"
            ]
        );
        assert_eq!(
            first_sentence("Reviewed search/hybrid.rs today. Then left."),
            "Reviewed search/hybrid.rs today"
        );
        assert_eq!(
            split_sentences("Compared tools, e.g. ripgrep and ag. Done."),
            vec!["Compared tools, e.g. ripgrep and ag.", "Done."]
        );
        assert_eq!(first_sentence(""), "");
        assert_eq!(
            split_sentences("Read about SELECT ... FOR UPDATE SKIP LOCKED. Tried it."),
            vec!["Read about SELECT ... FOR UPDATE SKIP LOCKED.", "Tried it."]
        );
        assert_eq!(
            split_sentences("It trailed off..."),
            vec!["It trailed off..."]
        );
    }

    #[test]
    fn complete_sentences_drops_a_cut_off_tail_and_caps_the_count() {
        assert_eq!(
            complete_sentences("One. Two. Three. Four", 3),
            "One. Two. Three."
        );
        assert_eq!(complete_sentences("One. Two is cut of", 3), "One.");
        assert_eq!(
            complete_sentences("no terminal punctuation at all", 2),
            "no terminal punctuation at all"
        );
    }
}
