//! English text to Kokoro phonemes: misaki's `G2P.__call__` (`misaki/en.py`,
//! v0.9.4) without spaCy and without espeak.
//!
//! What differs from misaki, on purpose:
//! - Part-of-speech tags come from a small rule tagger (`tag_word`), not
//!   spaCy. Tags only matter for heteronyms (read, record, live, used),
//!   function words and capitalised names, so a few neighbour rules cover
//!   the cases misaki's dictionaries distinguish.
//! - A word missing from both dictionaries is never sent to espeak or a
//!   network: `lts::guess` spells it from letter rules. misaki's
//!   `fallback=None` would leave it silent.
//! - In a word made of several pieces ("PostgreSQL", "VSCode"), only the
//!   piece the dictionaries miss is guessed; misaki's fallback guesses the
//!   whole word.
//! - Numbers, currency and symbols are already words (`normalize`).

use super::lexicon::{
    apply_stress, is_vowel, stress_weight, Context, Lexicon, CONSONANTS, PRIMARY,
};
use super::lts;

const SUBTOKEN_JUNKS: &str = "',-._‘’/";
const NON_QUOTE_PUNCTS: &str = ";:,.!?—…";
const KEPT_PUNCTS: &str = ";:,.!?—…\"“”()";
const TRAILING: &str = ".,!?;:…\"”)]}—";
const LEADING: &str = "\"“([{";

const PRONOUNS: &[&str] = &[
    "i", "he", "she", "it", "we", "they", "you", "me", "him", "her", "us", "them",
];
const DETERMINERS: &[&str] = &[
    "the", "a", "an", "this", "these", "those", "my", "your", "his", "her", "its", "our", "their",
    "some", "any", "no", "every", "each", "another",
];
const PREPOSITIONS: &[&str] = &[
    "of", "in", "on", "at", "by", "for", "with", "from", "about", "into", "onto", "over", "under",
    "after", "before", "between", "through", "during", "without", "within", "across", "against",
    "along", "among", "around", "behind", "below", "beside", "beyond", "near", "since", "than",
    "toward", "towards", "upon", "via", "per", "vs", "vs.",
];
/// Words after which the next word is a bare verb.
const VERB_BEFORE: &[&str] = &[
    "to",
    "will",
    "would",
    "can",
    "could",
    "should",
    "must",
    "may",
    "might",
    "shall",
    "do",
    "does",
    "did",
    "don't",
    "doesn't",
    "didn't",
    "won't",
    "can't",
    "cannot",
    "couldn't",
    "wouldn't",
    "shouldn't",
    "please",
    "let's",
    "i'll",
    "you'll",
    "we'll",
    "they'll",
    "he'll",
    "she'll",
    "it'll",
    "i'd",
    "you'd",
    "we'd",
    "they'd",
    "he'd",
    "she'd",
];
const HAVE: &[&str] = &[
    "has", "have", "had", "i've", "you've", "we've", "they've", "hasn't", "haven't", "hadn't",
];
const BE: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "being", "am", "it's", "that's", "he's", "she's",
    "i'm", "you're", "we're", "they're", "isn't", "aren't", "wasn't", "weren't",
];
const BE_OR_MODAL_AFTER_THAT: &[&str] = &[
    "is", "was", "will", "would", "could", "can", "has", "had", "should", "might", "may",
];

#[derive(Debug, Clone)]
struct Tok {
    text: String,
    tag: &'static str,
    phonemes: Option<String>,
    prespace: bool,
}

#[derive(Debug)]
enum Item {
    Word(Tok),
    Group(Vec<Tok>),
    Punct(String),
}

struct Piece {
    item: Item,
    space_after: bool,
}

pub struct G2p {
    lexicon: Lexicon,
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '‘' | '’')
}

/// misaki's `subtokenize` regex, as a scanner (Rust's `regex` has no
/// lookahead). Alternatives are tried in the regex's order at each position.
fn subtokenize(word: &str) -> Vec<String> {
    let c: Vec<char> = word.chars().collect();
    let n = c.len();
    let mut out = Vec::new();
    let mut i = 0;
    let letter = |k: usize| k < n && c[k].is_alphabetic();
    let lower = |k: usize| k < n && c[k].is_lowercase();
    let upper = |k: usize| k < n && c[k].is_uppercase();
    let digit = |k: usize| k < n && c[k].is_ascii_digit();
    let sep = |k: usize| k < n && (c[k] == ',' || c[k] == '.');
    while i < n {
        let start = i;
        let mut end = None;
        if i == 0 && is_apostrophe(c[0]) {
            let mut j = 0;
            while j < n && is_apostrophe(c[j]) {
                j += 1;
            }
            end = Some(j);
        }
        if end.is_none() && upper(i) && upper(i + 1) && lower(i + 2) {
            end = Some(i + 1);
        }
        if end.is_none() {
            let mut k = i;
            if i == 0 && c[i] == '-' {
                k += 1;
            }
            let body = k;
            loop {
                let group = if digit(k) && sep(k + 1) && digit(k + 2) {
                    3
                } else if digit(k) && digit(k + 1) {
                    2
                } else if sep(k) && digit(k + 1) {
                    2
                } else if digit(k) {
                    1
                } else {
                    0
                };
                if group == 0 {
                    break;
                }
                k += group;
            }
            if k > body {
                end = Some(k);
            }
        }
        if end.is_none() && (c[i] == '-' || c[i] == '_') {
            let mut j = i;
            while j < n && (c[j] == '-' || c[j] == '_') {
                j += 1;
            }
            end = Some(j);
        }
        if end.is_none() && is_apostrophe(c[i]) && i + 1 < n && is_apostrophe(c[i + 1]) {
            let mut j = i;
            while j < n && is_apostrophe(c[j]) {
                j += 1;
            }
            end = Some(j);
        }
        if end.is_none() && letter(i) {
            let mut j = i;
            while j < n && (letter(j) || (is_apostrophe(c[j]) && letter(j + 1))) {
                if lower(j) && upper(j + 1) {
                    end = Some(j + 1);
                    break;
                }
                j += 1;
            }
        }
        if end.is_none() && letter(i) {
            let mut j = i;
            while letter(j) {
                j += 1;
            }
            while j + 1 < n && is_apostrophe(c[j]) && letter(j + 1) {
                j += 2;
            }
            end = Some(j);
        }
        if end.is_none()
            && !(c[i] == '-' || c[i] == '_' || is_apostrophe(c[i]) || c[i].is_ascii_digit())
        {
            end = Some(i + 1);
        }
        if end.is_none() && is_apostrophe(c[i]) && c[i..].iter().all(|ch| is_apostrophe(*ch)) {
            end = Some(n);
        }
        match end {
            Some(e) if e > start => {
                out.push(c[start..e].iter().collect());
                i = e;
            }
            _ => i += 1,
        }
    }
    out
}

/// A part-of-speech guess from the neighbouring words, in spaCy's tag names.
fn tag_word(
    word: &str,
    prev: Option<&str>,
    next: Option<&str>,
    sentence_start: bool,
) -> &'static str {
    let lower = word.to_lowercase();
    let l = lower.as_str();
    let prev = prev.map(str::to_lowercase);
    let prev = prev.as_deref();
    let next = next.map(str::to_lowercase);
    let next = next.as_deref();
    if word.chars().all(|c| c.is_ascii_digit()) {
        return "CD";
    }
    match l {
        "a" | "an" | "the" => return "DT",
        "to" => return "TO",
        _ => {}
    }
    if word == "I" || (l != "i" && PRONOUNS.contains(&l)) {
        return "PRP";
    }
    if l == "that" {
        return match next {
            None => "DT",
            Some(n) if PRONOUNS.contains(&n) || DETERMINERS.contains(&n) || n == "there" => "IN",
            Some(n) if BE_OR_MODAL_AFTER_THAT.contains(&n) => "DT",
            _ if prev.is_none() => "DT",
            _ => "IN",
        };
    }
    if PREPOSITIONS.contains(&l) {
        return if next.is_none() { "RB" } else { "IN" };
    }
    let is_upper = word.chars().count() > 1
        && word == word.to_uppercase()
        && word.chars().any(char::is_alphabetic);
    let capitalised = word.chars().next().is_some_and(char::is_uppercase);
    if is_upper {
        return "NNP";
    }
    if let Some(p) = prev {
        if VERB_BEFORE.contains(&p) {
            return "VB";
        }
        if matches!(p, "i" | "we" | "they" | "you") {
            return if l.ends_with("ed") { "VBD" } else { "VBP" };
        }
        if matches!(p, "he" | "she" | "it") {
            return if l.ends_with('s') { "VBZ" } else { "VBD" };
        }
        if HAVE.contains(&p) {
            return "VBN";
        }
        if BE.contains(&p) {
            return if l.ends_with("ed") || l.ends_with("en") {
                "VBN"
            } else if l.ends_with("ing") {
                "VBG"
            } else {
                "JJ"
            };
        }
        if DETERMINERS.contains(&p) {
            return if capitalised && !sentence_start {
                "NNP"
            } else {
                "NN"
            };
        }
    }
    if capitalised && !sentence_start {
        return "NNP";
    }
    if l.ends_with("ly") {
        return "RB";
    }
    "NN"
}

/// misaki's `TokenContext` update after a word is pronounced.
fn token_context(ctx: Context, ps: &str, text: &str, tag: &str) -> Context {
    let mut vowel = ctx.future_vowel;
    for c in ps.chars() {
        if NON_QUOTE_PUNCTS.contains(c) {
            vowel = None;
            break;
        }
        if is_vowel(c) {
            vowel = Some(true);
            break;
        }
        if CONSONANTS.contains(c) {
            vowel = Some(false);
            break;
        }
    }
    let future_to = text == "to" || text == "To" || (text == "TO" && (tag == "TO" || tag == "IN"));
    Context {
        future_vowel: vowel,
        future_to,
    }
}

/// The tag of the most capitalised piece, as misaki's `merge_tokens`.
fn merged_tag(toks: &[Tok]) -> &'static str {
    toks.iter()
        .max_by_key(|t| {
            t.text
                .chars()
                .map(|c| {
                    if c.to_lowercase().eq(std::iter::once(c)) {
                        1
                    } else {
                        2
                    }
                })
                .sum::<usize>()
        })
        .map(|t| t.tag)
        .unwrap_or("NN")
}

/// misaki's `resolve_tokens`: decides whether pieces of one word are spoken
/// with spaces between them, and demotes extra primary stresses inside a
/// word built from several dictionary entries.
fn resolve_tokens(toks: &mut [Tok]) {
    let text: String = toks.iter().map(|t| t.text.as_str()).collect();
    let mut kinds = std::collections::HashSet::new();
    for c in text.chars().filter(|c| !SUBTOKEN_JUNKS.contains(*c)) {
        kinds.insert(if c.is_alphabetic() {
            0
        } else if c.is_ascii_digit() {
            1
        } else {
            2
        });
    }
    let prespace = text.contains(' ') || text.contains('/') || kinds.len() > 1;
    for (i, tok) in toks.iter_mut().enumerate() {
        if i > 0 {
            tok.prespace = prespace;
        }
    }
    if prespace {
        return;
    }
    let mut indices: Vec<(bool, usize, usize)> = toks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let ps = t.phonemes.as_deref().filter(|ps| !ps.is_empty())?;
            Some((ps.contains(PRIMARY), stress_weight(ps), i))
        })
        .collect();
    if indices.len() == 2 && toks[indices[0].2].text.chars().count() == 1 {
        let i = indices[1].2;
        let ps = toks[i].phonemes.clone().unwrap_or_default();
        toks[i].phonemes = Some(apply_stress(&ps, Some(-0.5)));
        return;
    }
    let primaries = indices.iter().filter(|(p, _, _)| *p).count();
    if indices.len() < 2 || primaries <= (indices.len() + 1) / 2 {
        return;
    }
    indices.sort();
    let half = indices.len() / 2;
    for (_, _, i) in indices.into_iter().take(half) {
        let ps = toks[i].phonemes.clone().unwrap_or_default();
        toks[i].phonemes = Some(apply_stress(&ps, Some(-0.5)));
    }
}

fn punct_phonemes(text: &str) -> String {
    if text.chars().all(|c| matches!(c, '-' | '–' | '—')) {
        return "—".to_string();
    }
    text.chars().filter(|c| KEPT_PUNCTS.contains(*c)).collect()
}

/// Splits whitespace-separated text into words and the punctuation around
/// them, the way spaCy separates them before misaki sees them.
fn split_pieces(text: &str) -> Vec<(String, bool, bool)> {
    let mut out = Vec::new();
    let chunks: Vec<&str> = text.split_whitespace().collect();
    for (index, chunk) in chunks.iter().enumerate() {
        let space_after = index + 1 < chunks.len();
        let chars: Vec<char> = chunk.chars().collect();
        if chars.iter().all(|c| matches!(c, '-' | '–' | '—')) {
            out.push((chunk.to_string(), true, space_after));
            continue;
        }
        let mut start = 0;
        while start < chars.len() && LEADING.contains(chars[start]) {
            start += 1;
        }
        let mut end = chars.len();
        while end > start && (TRAILING.contains(chars[end - 1]) || chars[end - 1] == '–') {
            end -= 1;
        }
        let mut parts: Vec<(String, bool)> = Vec::new();
        for c in &chars[..start] {
            parts.push((c.to_string(), true));
        }
        if end > start {
            parts.push((chars[start..end].iter().collect(), false));
        }
        let trailing: String = chars[end..].iter().collect();
        if !trailing.is_empty() {
            parts.push((trailing, true));
        }
        let count = parts.len();
        for (k, (part, is_punct)) in parts.into_iter().enumerate() {
            out.push((part, is_punct, space_after && k + 1 == count));
        }
    }
    out
}

impl G2p {
    pub fn new(lexicon: Lexicon) -> Self {
        Self { lexicon }
    }

    pub fn british(&self) -> bool {
        self.lexicon.british()
    }

    fn fallback(&self, text: &str) -> String {
        lts::guess(&self.lexicon, text)
    }

    fn word_phonemes(&self, tok: &Tok, ctx: Context) -> String {
        self.lexicon
            .word(&tok.text, tok.tag, ctx)
            .map(|(ps, _)| ps)
            .unwrap_or_else(|| self.fallback(&tok.text))
    }

    fn resolve_group(&self, toks: &mut [Tok], ctx: &mut Context) {
        let (mut left, mut right) = (0usize, toks.len());
        while left < right {
            let span = &toks[left..right];
            let fixed = span.iter().any(|t| t.phonemes.is_some());
            let text: String = span.iter().map(|t| t.text.as_str()).collect();
            let tag = merged_tag(span);
            let found = if fixed {
                None
            } else {
                self.lexicon.word(&text, tag, *ctx)
            };
            if let Some((ps, _)) = found {
                toks[left].phonemes = Some(ps.clone());
                for tok in &mut toks[left + 1..right] {
                    tok.phonemes = Some(String::new());
                }
                *ctx = token_context(*ctx, &ps, &text, tag);
                right = left;
                left = 0;
            } else if left + 1 < right {
                left += 1;
            } else {
                right -= 1;
                let tok = &mut toks[right];
                if tok.phonemes.is_none() {
                    if tok.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c)) {
                        tok.phonemes = Some(String::new());
                    } else {
                        let ps = self.fallback(&tok.text);
                        *ctx = token_context(*ctx, &ps, &tok.text, tok.tag);
                        tok.phonemes = Some(ps);
                    }
                }
                left = 0;
            }
        }
        resolve_tokens(toks);
    }

    fn pieces(&self, text: &str) -> Vec<Piece> {
        let raw = split_pieces(text);
        let words: Vec<(usize, &str)> = raw
            .iter()
            .enumerate()
            .filter(|(_, (_, is_punct, _))| !is_punct)
            .map(|(i, (t, _, _))| (i, t.as_str()))
            .collect();
        let mut pieces = Vec::with_capacity(raw.len());
        let mut sentence_start = true;
        for (i, (part, is_punct, space_after)) in raw.iter().enumerate() {
            if *is_punct {
                if part.chars().any(|c| matches!(c, '.' | '!' | '?')) {
                    sentence_start = true;
                }
                pieces.push(Piece {
                    item: Item::Punct(punct_phonemes(part)),
                    space_after: *space_after,
                });
                continue;
            }
            let position = words.iter().position(|(k, _)| *k == i).unwrap_or(0);
            let prev =
                (position > 0 && words[position - 1].0 + 1 == i).then(|| words[position - 1].1);
            let next = words
                .get(position + 1)
                .filter(|(k, _)| *k == i + 1)
                .map(|(_, w)| *w);
            let tag = tag_word(part, prev, next, sentence_start);
            sentence_start = false;
            let subs = subtokenize(part);
            let make = |text: String| {
                let tag = if text.chars().all(|c| c.is_ascii_digit()) {
                    "CD"
                } else {
                    tag
                };
                Tok {
                    text,
                    tag,
                    phonemes: None,
                    prespace: false,
                }
            };
            let item = if subs.len() == 1 {
                Item::Word(make(subs.into_iter().next().unwrap_or_default()))
            } else {
                Item::Group(
                    subs.into_iter()
                        .map(|s| {
                            let mut tok = make(s);
                            if !tok.text.chars().any(|c| c.is_alphanumeric())
                                && !tok.text.chars().all(|c| SUBTOKEN_JUNKS.contains(c))
                            {
                                tok.phonemes = Some(punct_phonemes(&tok.text));
                            }
                            tok
                        })
                        .collect(),
                )
            };
            pieces.push(Piece {
                item,
                space_after: *space_after,
            });
        }
        pieces
    }

    /// Phonemes for normalised English text, in Kokoro's symbol set.
    pub fn phonemize(&self, text: &str) -> String {
        let mut pieces = self.pieces(text);
        let mut ctx = Context::default();
        for piece in pieces.iter_mut().rev() {
            match &mut piece.item {
                Item::Punct(ps) => {
                    ctx = token_context(ctx, ps, ps, ".");
                }
                Item::Word(tok) => {
                    let ps = self.word_phonemes(tok, ctx);
                    ctx = token_context(ctx, &ps, &tok.text, tok.tag);
                    tok.phonemes = Some(ps);
                }
                Item::Group(toks) => self.resolve_group(toks, &mut ctx),
            }
        }
        let mut out = String::new();
        for piece in &pieces {
            match &piece.item {
                Item::Punct(ps) => out.push_str(ps),
                Item::Word(tok) => out.push_str(tok.phonemes.as_deref().unwrap_or("")),
                Item::Group(toks) => {
                    let mut joined = String::new();
                    for tok in toks {
                        let ps = tok.phonemes.as_deref().unwrap_or("");
                        if tok.prespace
                            && !joined.is_empty()
                            && !joined.ends_with(' ')
                            && !ps.is_empty()
                        {
                            joined.push(' ');
                        }
                        joined.push_str(ps);
                    }
                    out.push_str(&joined);
                }
            }
            if piece.space_after {
                out.push(' ');
            }
        }
        out.replace('ɾ', "T").replace('ʔ', "t")
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexicon::tests::sample;
    use super::*;

    fn g2p() -> G2p {
        G2p::new(sample())
    }

    #[test]
    fn subtokenize_matches_misaki_regex() {
        assert_eq!(subtokenize("well-known"), vec!["well", "-", "known"]);
        assert_eq!(subtokenize("PostgreSQL"), vec!["Postgre", "SQL"]);
        assert_eq!(subtokenize("VSCode"), vec!["VSCode"]);
        assert_eq!(subtokenize("don't"), vec!["don't"]);
        assert_eq!(subtokenize("you're"), vec!["you'r", "e"]);
        assert_eq!(subtokenize("'tis"), vec!["'", "tis"]);
        assert_eq!(subtokenize("v2.0"), vec!["v", "2.0"]);
        assert_eq!(subtokenize("iPhone"), vec!["i", "Phone"]);
        assert_eq!(subtokenize("James'"), vec!["James", "'"]);
    }

    #[test]
    fn tagger_separates_the_heteronym_cases() {
        assert_eq!(tag_word("read", Some("I'll"), Some("the"), false), "VB");
        assert_eq!(
            tag_word("read", Some("she"), Some("yesterday"), false),
            "VBD"
        );
        assert_eq!(tag_word("record", Some("Please"), Some("the"), false), "VB");
        assert_eq!(tag_word("record", Some("the"), None, false), "NN");
        assert_eq!(tag_word("live", Some("I"), Some("here"), false), "VBP");
        assert_eq!(tag_word("live", Some("and"), Some("music"), false), "NN");
        assert_eq!(tag_word("used", Some("I"), Some("to"), false), "VBD");
        assert_eq!(tag_word("that", Some("book"), Some("she"), false), "IN");
        assert_eq!(
            tag_word("Priya", Some("with"), Some("starts"), false),
            "NNP"
        );
        assert_eq!(tag_word("README", Some("the"), Some("in"), false), "NNP");
        assert_eq!(tag_word("in", Some("come"), None, false), "RB");
    }

    #[test]
    fn misaki_golden_sentences_with_sample_entries() {
        let g = g2p();
        assert_eq!(
            g.phonemize("Life is like a box of chocolates. You never know what you're gonna get."),
            "lˈIf ɪz lˈIk ɐ bˈɑks ʌv ʧˈɑkᵊləts. jˌu nˈɛvəɹ nˈO wˌʌt jʊɹ ɡˈʌnə ɡˈɛt."
        );
        assert_eq!(
            g.phonemize("He wanted to eat an apple."),
            "hˌi wˈɑntᵻd tʊ ˈit ɐn ˈæpᵊl."
        );
        assert_eq!(
            g.phonemize("The cats walked, talked, and kept jumping."),
            "ðə kˈæts wˈɔkt, tˈɔkt, ænd kˈɛpt ʤˈʌmpɪŋ."
        );
        assert_eq!(g.phonemize("Hello there."), "həlˈO ðˈɛɹ.");
        assert_eq!(
            g.phonemize("Thanks for waiting.").split(' ').last(),
            Some("wˈATɪŋ.")
        );
    }

    #[test]
    fn hyphenated_words_join_and_demote_stress() {
        let g = g2p();
        assert_eq!(g.phonemize("well-known"), "wˌɛlnˈOn");
        assert_eq!(g.phonemize("state-of-the-art"), "stˈAtʌvðiˈɑɹt");
    }

    #[test]
    fn possessives_and_heteronyms_in_sentences() {
        let g = g2p();
        assert_eq!(
            g.phonemize("James' room"),
            "ʤˈAmz ".to_string() + &g.phonemize("room")
        );
        assert_eq!(g.phonemize("I live here").split(' ').nth(1), Some("lˈɪv"));
        assert_eq!(
            g.phonemize("and live music").split(' ').nth(1),
            Some("lˈIv")
        );
        assert_eq!(
            g.phonemize("Please record the record.").split(' ').nth(1),
            Some("ɹəkˈɔɹd")
        );
        assert_eq!(
            g.phonemize("Please record the record.").split(' ').nth(3),
            Some("ɹˈɛkəɹd.")
        );
    }

    #[test]
    fn camel_case_keeps_the_known_piece() {
        let g = g2p();
        let ps = g.phonemize("PostgreSQL");
        assert!(ps.ends_with("sˈikwəl"), "{ps}");
        assert!(ps.len() > "sˈikwəl".len(), "{ps}");
    }

    #[test]
    fn unknown_words_are_never_silent() {
        let g = g2p();
        for word in ["Kokoro", "Priya", "snarfblat", "Kubernetes", "npm"] {
            let ps = g.phonemize(word);
            assert!(!ps.trim().is_empty(), "{word}");
            assert!(ps.chars().any(is_vowel), "{word}: {ps}");
        }
    }

    #[test]
    fn dashes_and_quotes_become_kokoro_punctuation() {
        let g = g2p();
        assert_eq!(g.phonemize("hello - there"), "həlˈO — ðˈɛɹ");
        assert_eq!(g.phonemize("“hello”"), "“həlˈO”");
    }
}
