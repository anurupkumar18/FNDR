//! Pronunciation for a word neither dictionary has: names, product names,
//! made-up words. Deterministic letter rules, written for FNDR, in misaki's
//! phoneme set; never an external program or model. A word without a vowel
//! letter is spelled out. The guess is meant to be intelligible, not right:
//! "Kokoro" comes out as misaki's own dictionary would say it, "Kubernetes"
//! does not.

use super::lexicon::{apply_stress, Lexicon, PRIMARY};
use super::normalize;

#[derive(Debug, Clone)]
struct Seg {
    ps: String,
    vowel: bool,
    /// A single vowel letter's sound, which weakens to /ə/ when unstressed.
    reducible: bool,
    /// The vowel ends its syllable.
    open: bool,
}

fn seg(ps: &str) -> Seg {
    Seg {
        ps: ps.to_string(),
        vowel: false,
        reducible: false,
        open: false,
    }
}

fn vowel(ps: &str, reducible: bool, open: bool) -> Seg {
    Seg {
        ps: ps.to_string(),
        vowel: true,
        reducible,
        open,
    }
}

fn is_vowel_letter(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Graphemes that make one consonant sound, longest first.
const CONSONANT_GROUPS: &[(&str, &str)] = &[
    ("tch", "ʧ"),
    ("sch", "sk"),
    ("ch", "ʧ"),
    ("sh", "ʃ"),
    ("th", "θ"),
    ("ph", "f"),
    ("wh", "w"),
    ("ck", "k"),
    ("ng", "ŋ"),
    ("qu", "kw"),
    ("dg", "ʤ"),
];

/// Vowel graphemes that are never reduced, longest first.
const VOWEL_GROUPS: &[(&str, &str)] = &[
    ("eigh", "A"),
    ("ough", "O"),
    ("augh", "ɔ"),
    ("igh", "I"),
    ("ee", "i"),
    ("ea", "i"),
    ("oo", "u"),
    ("ou", "W"),
    ("oi", "Y"),
    ("oy", "Y"),
    ("ai", "A"),
    ("ay", "A"),
    ("au", "ɔ"),
    ("aw", "ɔ"),
    ("ei", "A"),
    ("ey", "A"),
    ("ie", "i"),
    ("ue", "u"),
    ("ui", "u"),
    ("eu", "u"),
    ("ew", "u"),
    ("oa", "O"),
];

/// Endings with a fixed sound; the syllable before them takes the stress.
const STRESS_BEFORE: &[(&str, &[(&str, bool)])] = &[
    ("tion", &[("ʃ", false), ("ə", true), ("n", false)]),
    ("sion", &[("ʒ", false), ("ə", true), ("n", false)]),
    ("cial", &[("ʃ", false), ("ə", true), ("l", false)]),
    ("tial", &[("ʃ", false), ("ə", true), ("l", false)]),
    ("ical", &[("ɪ", true), ("k", false), ("ᵊl", false)]),
    ("ity", &[("ə", true), ("ɾ", false), ("i", true)]),
    ("ic", &[("ɪ", true), ("k", false)]),
];

const PLAIN_ENDINGS: &[(&str, &[(&str, bool)])] = &[
    ("ture", &[("ʧ", false), ("əɹ", true)]),
    ("ous", &[("ə", true), ("s", false)]),
    (
        "ment",
        &[("m", false), ("ə", true), ("n", false), ("t", false)],
    ),
];

fn rules(word: &str) -> (Vec<Seg>, Option<usize>) {
    let mut body = word.to_string();
    let mut tail: Vec<Seg> = Vec::new();
    let mut stress_hint = None;
    for (ending, sounds) in STRESS_BEFORE.iter().chain(PLAIN_ENDINGS.iter()) {
        if body.len() > ending.len() + 1 && body.ends_with(ending) {
            body.truncate(body.len() - ending.len());
            tail = sounds
                .iter()
                .map(|(ps, is_vowel)| {
                    if *is_vowel {
                        vowel(ps, false, false)
                    } else {
                        seg(ps)
                    }
                })
                .collect();
            if STRESS_BEFORE.iter().any(|(e, _)| e == ending) {
                stress_hint = Some(());
            }
            break;
        }
    }
    let c: Vec<char> = body.chars().collect();
    let n = c.len();
    let at = |k: usize| c.get(k).copied();
    let starts = |k: usize, s: &str| {
        let s: Vec<char> = s.chars().collect();
        k + s.len() <= n && c[k..k + s.len()] == s[..]
    };
    let mut out: Vec<Seg> = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = c[i];
        if i == 0 {
            if let Some((g, ps)) = [
                ("kn", "n"),
                ("wr", "ɹ"),
                ("gn", "n"),
                ("ps", "s"),
                ("pn", "n"),
            ]
            .iter()
            .find(|(g, _)| starts(0, g))
            {
                out.push(seg(ps));
                i += g.len();
                continue;
            }
        }
        if let Some((g, ps)) = VOWEL_GROUPS
            .iter()
            .find(|(g, _)| starts(i, g) && !(*g == "ie" && at(i + 2) == Some('r')))
        {
            out.push(vowel(ps, false, false));
            i += g.chars().count();
            continue;
        }
        if starts(i, "ow") {
            out.push(vowel(if i + 2 == n { "O" } else { "W" }, false, false));
            i += 2;
            continue;
        }
        if starts(i, "gh") {
            if i == 0 {
                out.push(seg("ɡ"));
            }
            i += 2;
            continue;
        }
        if let Some((g, ps)) = CONSONANT_GROUPS.iter().find(|(g, _)| starts(i, g)) {
            out.push(seg(ps));
            i += g.chars().count();
            continue;
        }
        if is_vowel_letter(ch)
            && at(i + 1) == Some('r')
            && !at(i + 2).is_some_and(|x| is_vowel_letter(x) || x == 'y' || x == 'r')
        {
            let ps = match ch {
                'a' => "ɑɹ",
                'o' => "ɔɹ",
                'e' => "əɹ",
                _ => "ɜɹ",
            };
            out.push(vowel(ps, false, false));
            i += 2;
            continue;
        }
        if i + 2 == n && ch == 'l' && at(i + 1) == Some('e') && i > 0 && !is_vowel_letter(c[i - 1])
        {
            out.push(seg("ᵊl"));
            i += 2;
            continue;
        }
        if is_vowel_letter(ch) {
            let last = i + 1 == n;
            if last {
                match ch {
                    'e' if out.iter().any(|s| s.vowel) => {}
                    'e' => out.push(vowel("i", false, true)),
                    'a' => out.push(vowel("ə", false, true)),
                    'o' => out.push(vowel("O", false, true)),
                    'i' => out.push(vowel("i", false, true)),
                    _ => out.push(vowel("u", false, true)),
                }
                i += 1;
                continue;
            }
            let open = at(i + 1).is_some_and(is_vowel_letter)
                || (at(i + 1).is_some_and(|x| x != 'x' && x != 'y' && !is_vowel_letter(x))
                    && at(i + 2).is_some_and(|x| is_vowel_letter(x) || x == 'y')
                    && at(i + 1) != at(i + 2))
                || (at(i + 1) == Some('y') && at(i + 2).is_some_and(is_vowel_letter));
            let magic_e = open && at(i + 2) == Some('e') && i + 3 == n;
            let ps = match (ch, open) {
                ('a', true) => "A",
                ('e', true) => "i",
                ('i', true) if magic_e => "I",
                ('i', true) => "i",
                ('o', true) => "O",
                ('u', true) => "u",
                ('a', false) => "æ",
                ('e', false) => "ɛ",
                ('i', false) => "ɪ",
                ('o', false) => "ɑ",
                _ => "ʌ",
            };
            let hiatus = at(i + 1).is_some_and(is_vowel_letter);
            out.push(vowel(ps, !magic_e && !hiatus, open));
            i += 1;
            continue;
        }
        if ch == 'y' {
            let before_vowel = at(i + 1).is_some_and(is_vowel_letter);
            if before_vowel {
                out.push(seg("j"));
            } else if i + 1 == n {
                let others = out.iter().any(|s| s.vowel);
                out.push(vowel(if others { "i" } else { "I" }, false, true));
            } else {
                out.push(vowel("ɪ", true, false));
            }
            i += 1;
            continue;
        }
        let next_soft = at(i + 1).is_some_and(|x| matches!(x, 'e' | 'i' | 'y'));
        let ps = match ch {
            'c' if next_soft => "s",
            'c' => "k",
            'g' if next_soft && i > 0 => "ʤ",
            'g' => "ɡ",
            'j' => "ʤ",
            'q' => "k",
            'r' => "ɹ",
            'x' if i == 0 => "z",
            'x' => "ks",
            's' if i > 0 && i + 1 < n && is_vowel_letter(c[i - 1]) && is_vowel_letter(c[i + 1]) => {
                "z"
            }
            'h' if i > 0 && !is_vowel_letter(c[i - 1]) => {
                i += 1;
                continue;
            }
            'b' | 'd' | 'f' | 'h' | 'k' | 'l' | 'm' | 'n' | 'p' | 's' | 't' | 'v' | 'w' | 'z' => {
                let s = ch.to_string();
                if out.last().is_some_and(|last: &Seg| last.ps == s) {
                    i += 1;
                    continue;
                }
                out.push(seg(&s));
                i += 1;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        };
        if out.last().is_some_and(|last| last.ps == ps) {
            i += 1;
            continue;
        }
        out.push(seg(ps));
        i += 1;
    }
    let stressed = if stress_hint.is_some() {
        out.iter().rposition(|s| s.vowel)
    } else {
        None
    };
    out.extend(tail);
    (out, stressed)
}

fn assemble(mut segs: Vec<Seg>, stressed: Option<usize>) -> String {
    let vowels: Vec<usize> = segs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.vowel)
        .map(|(i, _)| i)
        .collect();
    let Some(first) = vowels.first().copied() else {
        return segs.into_iter().map(|s| s.ps).collect();
    };
    let target = stressed.unwrap_or(first);
    let last_vowel = *vowels.last().unwrap_or(&first);
    for &i in &vowels {
        if i != target && segs[i].reducible && i != last_vowel && (i > target || segs[i].open) {
            segs[i].ps = "ə".to_string();
        }
    }
    let mut out = String::new();
    for (i, s) in segs.iter().enumerate() {
        if i == target {
            out.push(PRIMARY);
        }
        out.push_str(&s.ps);
    }
    out
}

fn to_british(ps: &str) -> String {
    let mut s = ps
        .replace("əɹ", "ə")
        .replace("ɜɹ", "ɜː")
        .replace("ɑɹ", "ɑː")
        .replace("ɔɹ", "ɔː")
        .replace("ɛɹ", "ɛː")
        .replace("ɪɹ", "ɪə")
        .replace('æ', "a")
        .replace('ɑ', "ɒ")
        .replace('O', "Q")
        .replace('ɾ', "t");
    s = s.replace("ɒː", "ɑː");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (k, c) in chars.iter().enumerate() {
        if *c == 'ɹ'
            && !chars
                .get(k + 1)
                .is_some_and(|n| "AIQWYaiuɑɒɔəɛɜɪʊʌᵻˈˌ".contains(*n))
        {
            continue;
        }
        out.push(*c);
    }
    out
}

fn core(lexicon: &Lexicon, word: &str) -> String {
    let lower = word.to_lowercase();
    if !lower.chars().any(|c| is_vowel_letter(c) || c == 'y') {
        return lexicon.spell(word).map(|(ps, _)| ps).unwrap_or_default();
    }
    let (segs, stressed) = rules(&lower);
    let ps = assemble(segs, stressed);
    if lexicon.british() {
        to_british(&ps)
    } else {
        ps
    }
}

fn number(lexicon: &Lexicon, digits: &str) -> String {
    normalize::cardinal_words(digits)
        .split(|c: char| c == ' ' || c == '-')
        .filter(|w| !w.is_empty())
        .filter_map(|w| lexicon.lookup(w, None, None, None).map(|(ps, _)| ps))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "VSCode" splits after "VS": capitals, then a capitalised word.
fn initialism_then_word(word: &str) -> Option<usize> {
    let chars: Vec<char> = word.chars().collect();
    let caps = chars.iter().take_while(|c| c.is_ascii_uppercase()).count();
    let rest = &chars[caps.min(chars.len())..];
    (caps >= 3 && rest.len() >= 2 && rest.iter().all(|c| c.is_ascii_lowercase())).then(|| caps - 1)
}

/// A pronunciation for any word, in the lexicon's accent.
pub fn guess(lexicon: &Lexicon, word: &str) -> String {
    let clean: String = word
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect();
    if clean.is_empty() {
        return String::new();
    }
    if clean.chars().all(|c| c.is_ascii_digit()) {
        return number(lexicon, &clean);
    }
    if clean.chars().any(|c| c.is_ascii_digit()) {
        let mut runs: Vec<String> = Vec::new();
        for c in clean.chars() {
            let digit = c.is_ascii_digit();
            match runs.last_mut() {
                Some(run) if run.chars().next().is_some_and(|f| f.is_ascii_digit()) == digit => {
                    run.push(c)
                }
                _ => runs.push(c.to_string()),
            }
        }
        return runs
            .iter()
            .map(|run| guess(lexicon, run))
            .filter(|ps| !ps.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
    }
    let letters: String = clean.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return String::new();
    }
    if let Some(split) = initialism_then_word(&letters) {
        let (head, tail) = letters.split_at(split);
        let head = lexicon.spell(head).map(|(ps, _)| ps).unwrap_or_default();
        let tail = lexicon
            .lookup(&tail.to_lowercase(), None, None, None)
            .map(|(ps, _)| ps)
            .unwrap_or_else(|| guess(lexicon, tail));
        return format!("{} {tail}", apply_stress(&head, Some(-1.0)));
    }
    let lower = clean.to_lowercase();
    if let Some(stem) = lower.strip_suffix("'s") {
        if let Some(ps) = lexicon.suffix_s(&guess(lexicon, stem)) {
            return ps;
        }
    }
    let n = letters.chars().count();
    if n > 5 && lower.ends_with("ing") {
        let stem = &lower[..lower.len() - 3];
        if let Some(ps) = lexicon.suffix_ing(&core(lexicon, stem)) {
            return ps;
        }
    }
    if n > 4 && lower.ends_with("ed") && !lower.ends_with("eed") {
        let base = &lower[..lower.len() - 1];
        let stem = if base.ends_with("le") && base.len() > 3 {
            base
        } else {
            &lower[..lower.len() - 2]
        };
        if let Some(ps) = lexicon.suffix_ed(&core(lexicon, stem)) {
            return ps;
        }
    }
    if n > 3
        && lower.ends_with('s')
        && !lower.ends_with("ss")
        && !lower.ends_with("us")
        && !lower.ends_with("is")
    {
        let stem = &lower[..lower.len() - 1];
        if let Some(ps) = lexicon.suffix_s(&core(lexicon, stem)) {
            return ps;
        }
    }
    let ps = core(lexicon, &letters);
    if ps.contains(PRIMARY) {
        ps
    } else {
        apply_stress(&ps, Some(2.0))
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexicon::tests::sample;
    use super::super::lexicon::Lexicon;
    use super::*;

    const US_VOCAB: &str = "AIOWYbdfhijklmnpstuvwzæðŋɑɔəɛɜɡɪɹɾʃʊʌʒʤʧˈˌθᵊᵻʔ ";
    const GB_VOCAB: &str = "AIQWYabdfhijklmnpstuvwzðŋɑɒɔəɛɜɡɪɹʃʊʌʒʤʧˈˌːθᵊ ";

    const WORDS: &[&str] = &[
        "Kokoro",
        "Priya",
        "snarfblat",
        "Kubernetes",
        "zorbled",
        "quixotical",
        "Anthropic",
        "Nguyen",
        "Siobhan",
        "flibbertigibbet",
        "Xavier",
        "yttrium",
        "npm",
        "rhythm",
        "Joaquin",
        "Tchaikovsky",
        "Wrexham",
        "gnocchi",
        "knightly",
        "Ouagadougou",
        "Zoë",
        "o'Brien",
        "Dmitri's",
        "blorping",
        "Krystle",
    ];

    #[test]
    fn known_shapes_come_out_as_expected() {
        let lex = sample();
        assert_eq!(guess(&lex, "Kokoro"), "kˈOkəɹO");
        assert_eq!(guess(&lex, "snarfblat"), "snˈɑɹfblæt");
        assert_eq!(guess(&lex, "Priya"), "pɹˈijə");
        assert_eq!(guess(&lex, "zorbled"), "zˈɔɹbᵊld");
        assert_eq!(guess(&lex, "npm"), "ˌɛnpˌiˈɛm");
        assert_eq!(guess(&lex, "Anthropic"), "ænθɹˈɑpɪk");
        assert_eq!(guess(&lex, "Xavier"), "zˈAviəɹ");
        assert_eq!(guess(&lex, "VSCode"), "viˌɛs kˈOd");
    }

    #[test]
    fn every_guess_stays_inside_the_accent_vocabulary() {
        let us = sample();
        let gb =
            Lexicon::from_json(true, super::super::lexicon::tests::US_GOLD_SAMPLE, "{}").unwrap();
        for word in WORDS {
            let ps = guess(&us, word);
            assert!(!ps.is_empty(), "{word}");
            assert!(ps.chars().all(|c| US_VOCAB.contains(c)), "{word}: {ps}");
            let ps = guess(&gb, word);
            assert!(ps.chars().all(|c| GB_VOCAB.contains(c)), "gb {word}: {ps}");
        }
    }

    #[test]
    fn guesses_are_deterministic_and_stressed_once() {
        let lex = sample();
        for word in WORDS {
            let a = guess(&lex, word);
            assert_eq!(a, guess(&lex, word));
            assert!(a.matches(PRIMARY).count() >= 1, "{word}: {a}");
        }
    }

    #[test]
    fn digits_inside_a_word_are_read_as_numbers() {
        let lex = Lexicon::from_json(
            false,
            r#"{"two": "tˈu", "v": "vˈi", "V": "vˈi", "twenty": "twˈɛnti", "three": "θɹˈi"}"#,
            "{}",
        )
        .unwrap();
        assert_eq!(guess(&lex, "23"), "twˈɛnti θɹˈi");
        assert!(guess(&lex, "v2").ends_with("tˈu"));
    }
}
