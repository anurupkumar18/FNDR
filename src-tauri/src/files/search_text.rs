//! Text the way keyword search keeps it and matches it, so a word is found
//! however it was typed or extracted. Ported from DigUp's `SearchText`.

use unicode_normalization::UnicodeNormalization;

/// NFKC (Arabic presentation forms and ligatures become plain letters,
/// full-width becomes ASCII, Bengali's two spellings of a vowel become one),
/// invisible formatting dropped (zero-width joiners, direction marks), and
/// Arabic the way people type it: no tatweel, no short-vowel marks, one alef.
pub fn normalized(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.nfkc() {
        match c as u32 {
            0x200B => out.push(' '), // zero-width space: a word break (Thai, Khmer)
            0x00AD
            | 0x061C
            | 0x200C..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x2069
            | 0xFEFF => {}
            0x0640 | 0x064B..=0x065F | 0x0670 => {} // tatweel, harakat, dagger alef
            0x0622 | 0x0623 | 0x0625 | 0x0671 => out.push('\u{0627}'),
            _ => out.push(c),
        }
    }
    out
}

/// Words of scripts that attach endings (Bengali, Hindi, Arabic) match as prefixes.
pub fn inflects(word: &str) -> bool {
    word.chars()
        .any(|c| matches!(c as u32, 0x0900..=0x0DFF | 0x0600..=0x06FF))
}

/// Whether text says something in words: at least a fifth of what is not space
/// is letters (marks count). A table of numbers gets keyword search only.
pub fn has_words(text: &str) -> bool {
    let (mut letters, mut others) = (0_usize, 0_usize);
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        if c.is_alphabetic() || is_mark(c) {
            letters += 1;
        } else {
            others += 1;
        }
    }
    letters > 0 && letters * 4 >= others
}

fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x0900..=0x0DFF | 0x064B..=0x065F)
        && !c.is_alphanumeric()
        && !c.is_whitespace()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

/// Lowercase, accents dropped: the fold `matches` compares with.
fn fold(text: &str) -> String {
    normalized(text)
        .nfd()
        .filter(|c| !matches!(*c as u32, 0x0300..=0x036F))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Where `word` is in `text` as keyword search would find it, as byte ranges
/// of the text as written: whole words, and for an inflecting script the whole
/// word it starts. At most 500 ranges.
pub fn matches(word: &str, text: &str) -> Vec<std::ops::Range<usize>> {
    let needle: Vec<char> = fold(word).chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let prefix = inflects(word);
    // Each folded char remembers the byte range of the original char it came from.
    let mut folded: Vec<char> = Vec::with_capacity(text.len());
    let mut origin: Vec<std::ops::Range<usize>> = Vec::with_capacity(text.len());
    for (start, c) in text.char_indices() {
        let range = start..start + c.len_utf8();
        for piece in fold(&c.to_string()).chars() {
            folded.push(piece);
            origin.push(range.clone());
        }
    }
    let mut found: Vec<std::ops::Range<usize>> = Vec::new();
    let mut at = 0;
    while at + needle.len() <= folded.len() && found.len() < 500 {
        if folded[at..at + needle.len()] != needle[..] {
            at += 1;
            continue;
        }
        let mut range = origin[at].start..origin[at + needle.len() - 1].end;
        at += needle.len();
        if text[..range.start]
            .chars()
            .next_back()
            .is_some_and(is_word_char)
        {
            continue;
        }
        if text[range.end..].chars().next().is_some_and(is_word_char) {
            if !prefix {
                continue;
            }
            let rest = &text[range.end..];
            let tail = rest.find(|c: char| !is_word_char(c)).unwrap_or(rest.len());
            range.end += tail;
        }
        if found.last().is_some_and(|last| last.end > range.start) {
            continue;
        }
        found.push(range);
    }
    found
}

/// One line around the first of `terms` in `text`, the terms marked with
/// guillemets as SQLite's `snippet()` marks them, quoted as written. About
/// `words` words. `None` when none of the terms is in it.
pub fn snippet(text: &str, terms: &[String], words: usize) -> Option<String> {
    let mut marks: Vec<_> = terms.iter().flat_map(|term| matches(term, text)).collect();
    marks.sort_by_key(|range| range.start);
    let first = marks.first()?.clone();
    let before_words = words / 3;
    let head = &text[..first.start];
    let mut start = first.start;
    let mut seen = 0;
    for (index, c) in head.char_indices().rev() {
        if c.is_whitespace() {
            seen += 1;
            if seen > before_words {
                start = index + c.len_utf8();
                break;
            }
        }
        start = index;
    }
    let tail = &text[first.end..];
    let mut end = text.len();
    let mut after = 0;
    for (index, c) in tail.char_indices() {
        if c.is_whitespace() {
            after += 1;
            if after >= words.saturating_sub(before_words) {
                end = first.end + index;
                break;
            }
        }
    }
    let mut out = String::new();
    if start > 0 {
        out.push('\u{2026}');
    }
    let mut cursor = start;
    for mark in &marks {
        if mark.start < cursor || mark.end > end {
            continue;
        }
        out.push_str(&text[cursor..mark.start]);
        out.push('\u{AB}');
        out.push_str(&text[mark.clone()]);
        out.push('\u{BB}');
        cursor = mark.end;
    }
    out.push_str(&text[cursor..end]);
    if end < text.len() {
        out.push('\u{2026}');
    }
    Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// One line, at most `limit` characters: for result lists.
pub fn excerpt(text: &str, limit: usize) -> String {
    let start: String = text.chars().take(limit * 8).collect();
    let line = start.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > limit || start.len() < text.len() {
        let mut cut: String = line.chars().take(limit).collect();
        cut.push('\u{2026}');
        cut
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_is_matched_the_way_people_type_it() {
        // Hamza on alef, harakat and tatweel all fold away.
        assert_eq!(
            normalized("\u{0623}\u{064E}\u{0640}\u{0628}"),
            "\u{0627}\u{0628}"
        );
        // A presentation-form ligature becomes plain letters.
        assert_eq!(normalized("\u{FEFB}"), "\u{0644}\u{0627}");
    }

    #[test]
    fn invisible_formatting_is_dropped() {
        assert_eq!(normalized("a\u{200D}b\u{200E}c"), "abc");
        assert_eq!(normalized("a\u{200B}b"), "a b");
        assert_eq!(normalized("\u{FF21}\u{FF22}"), "AB");
    }

    #[test]
    fn inflecting_scripts() {
        assert!(inflects("\u{0995}\u{09C1}\u{09AE}\u{09BF}\u{09B0}"));
        assert!(inflects("\u{0645}\u{062F}\u{0631}\u{0633}"));
        assert!(!inflects("crocodile"));
    }

    #[test]
    fn numbers_are_not_words() {
        assert!(has_words("The clause about pets in the lease."));
        assert!(!has_words(
            "1,204.55 3,881.20 9,004.01 12 14 19 22 31 44 51"
        ));
        assert!(!has_words(""));
    }

    #[test]
    fn whole_words_only_except_in_inflecting_scripts() {
        let text = "The cat sat in a category.";
        let found = matches("cat", text);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].clone()], "cat");
        assert_eq!(matches("CAFE", "Le Caf\u{E9} est ouvert").len(), 1);
        let bengali = "\u{0995}\u{09C1}\u{09AE}\u{09BF}\u{09B0}\u{0993} \u{098F}\u{09B2}";
        let found = matches("\u{0995}\u{09C1}\u{09AE}\u{09BF}\u{09B0}", bengali);
        assert_eq!(
            &bengali[found[0].clone()],
            "\u{0995}\u{09C1}\u{09AE}\u{09BF}\u{09B0}\u{0993}"
        );
    }

    #[test]
    fn snippets_mark_the_words_as_written() {
        let text = "Tenants may keep one cat. Pets other than cats need written consent from the landlord.";
        let line = snippet(text, &["pets".to_string()], 12).unwrap();
        assert!(line.contains("\u{AB}Pets\u{BB}"), "{line}");
        assert!(snippet(text, &["zebra".to_string()], 12).is_none());
    }

    #[test]
    fn excerpts_are_one_bounded_line() {
        assert_eq!(excerpt("a\n\n b", 10), "a b");
        assert_eq!(excerpt("abcdefghijkl", 5), "abcde\u{2026}");
    }
}
