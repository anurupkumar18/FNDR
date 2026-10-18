//! misaki's English lexicon (`misaki/en.py`, class `Lexicon`, v0.9.4), ported
//! rule for rule: gold then silver dictionaries, the capitalised and
//! lowercased forms `grow_dictionary` adds, stress rules, the special cases
//! for function words, and the `-s`, `-ed` and `-ing` stems. Numbers are not
//! here: `normalize` has already turned them into words.
//!
//! The dictionaries are misaki's `us_gold.json`, `us_silver.json`,
//! `gb_gold.json` and `gb_silver.json` (Apache-2.0), downloaded with the model.

use std::collections::HashMap;

pub const PRIMARY: char = 'ˈ';
pub const SECONDARY: char = 'ˌ';
pub const VOWELS: &str = "AIOQWYaiuæɑɒɔəɛɜɪʊʌᵻ";
pub const CONSONANTS: &str = "bdfhjklmnpstvwzðŋɡɹɾʃʒʤʧθ";
const DIPHTHONGS: &str = "AIOQWYʤʧ";
const US_TAUS: &str = "AIOWYiuæɑəɛɪɹʊʌ";
const SYMBOLS: &[(&str, &str)] = &[("%", "percent"), ("&", "and"), ("+", "plus"), ("@", "at")];

pub fn is_vowel(c: char) -> bool {
    VOWELS.contains(c)
}

pub fn stress_weight(ps: &str) -> usize {
    ps.chars()
        .map(|c| if DIPHTHONGS.contains(c) { 2 } else { 1 })
        .sum()
}

/// One dictionary value: a pronunciation, or pronunciations by part of speech
/// with a `DEFAULT` (a tag may map to nothing).
#[derive(Debug, Clone)]
pub enum Entry {
    Plain(String),
    Tagged(HashMap<String, Option<String>>),
}

/// What the next word starts with, which picks "the" /ðə/ or /ði/, "to"
/// /tə/ or /tʊ/, and so on. `future_vowel` is `None` before punctuation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    pub future_vowel: Option<bool>,
    pub future_to: bool,
}

/// Python's `str.capitalize`: first character upper, the rest lower.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

fn is_lower(word: &str) -> bool {
    word == word.to_lowercase()
}

fn is_upper(word: &str) -> bool {
    word == word.to_uppercase()
}

fn is_alpha(word: &str) -> bool {
    !word.is_empty() && word.chars().all(char::is_alphabetic)
}

/// `LEXICON_ORDS`: apostrophe, hyphen, and ASCII letters.
fn lexicon_char(c: char) -> bool {
    c == '\'' || c == '-' || c.is_ascii_alphabetic()
}

/// misaki's `apply_stress`. `None` leaves stress alone; below -1 strips it;
/// -1 (or 0 and -0.5 when primary stress is present) demotes primary to
/// secondary; 0 to 1 adds secondary stress to an unstressed word; at least 1
/// promotes secondary to primary; above 1 adds primary stress.
pub fn apply_stress(ps: &str, stress: Option<f32>) -> String {
    let Some(stress) = stress else {
        return ps.to_string();
    };
    let has_primary = ps.contains(PRIMARY);
    let has_secondary = ps.contains(SECONDARY);
    let has_any = has_primary || has_secondary;
    if stress < -1.0 {
        return ps.replace([PRIMARY, SECONDARY], "");
    }
    if stress == -1.0 || ((stress == 0.0 || stress == -0.5) && has_primary) {
        return ps
            .replace(SECONDARY, "")
            .replace(PRIMARY, &SECONDARY.to_string());
    }
    if (stress == 0.0 || stress == 0.5 || stress == 1.0) && !has_any {
        if !ps.chars().any(is_vowel) {
            return ps.to_string();
        }
        return restress(&format!("{SECONDARY}{ps}"));
    }
    if stress >= 1.0 && !has_primary && has_secondary {
        return ps.replace(SECONDARY, &PRIMARY.to_string());
    }
    if stress > 1.0 && !has_any {
        if !ps.chars().any(is_vowel) {
            return ps.to_string();
        }
        return restress(&format!("{PRIMARY}{ps}"));
    }
    ps.to_string()
}

/// Moves each stress mark to just before the next vowel.
fn restress(ps: &str) -> String {
    let chars: Vec<char> = ps.chars().collect();
    let mut keyed: Vec<(f64, char)> = chars
        .iter()
        .enumerate()
        .map(|(i, c)| (i as f64, *c))
        .collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == PRIMARY || *c == SECONDARY {
            if let Some(j) = (i..chars.len()).find(|j| is_vowel(chars[*j])) {
                keyed[i].0 = j as f64 - 0.5;
            }
        }
    }
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    keyed.into_iter().map(|(_, c)| c).collect()
}

fn parent_tag(tag: &str) -> &str {
    if tag.starts_with("VB") {
        "VERB"
    } else if tag.starts_with("NN") {
        "NOUN"
    } else if tag.starts_with("ADV") || tag.starts_with("RB") {
        "ADV"
    } else if tag.starts_with("ADJ") || tag.starts_with("JJ") {
        "ADJ"
    } else {
        tag
    }
}

pub struct Lexicon {
    british: bool,
    golds: HashMap<String, Entry>,
    silvers: HashMap<String, Entry>,
}

pub type Lookup = Option<(String, u8)>;

impl Lexicon {
    /// Builds a lexicon from misaki's JSON dictionaries.
    pub fn from_json(british: bool, gold: &str, silver: &str) -> Result<Self, String> {
        Ok(Self {
            british,
            golds: parse_dictionary(gold)?,
            silvers: parse_dictionary(silver)?,
        })
    }

    pub fn british(&self) -> bool {
        self.british
    }

    /// A dictionary hit, including the forms `grow_dictionary` adds: the
    /// capitalised form of a lowercase key and the lowercase form of a
    /// capitalised one. Keys shorter than two characters do not grow.
    fn find<'a>(dict: &'a HashMap<String, Entry>, word: &str) -> Option<&'a Entry> {
        if let Some(entry) = dict.get(word) {
            return Some(entry);
        }
        if word.chars().count() < 2 {
            return None;
        }
        let lower = word.to_lowercase();
        if word != lower && word == capitalize(&lower) {
            return dict.get(&lower);
        }
        if word == lower {
            let capital = capitalize(word);
            if capital != word {
                return dict.get(&capital);
            }
        }
        None
    }

    fn gold(&self, word: &str) -> Option<&Entry> {
        Self::find(&self.golds, word)
    }

    fn silver(&self, word: &str) -> Option<&Entry> {
        Self::find(&self.silvers, word)
    }

    fn gold_plain(&self, word: &str) -> String {
        match self.gold(word) {
            Some(Entry::Plain(ps)) => ps.clone(),
            Some(Entry::Tagged(tags)) => tags.get("DEFAULT").cloned().flatten().unwrap_or_default(),
            None => String::new(),
        }
    }

    fn gold_tagged(&self, word: &str, tag: &str) -> String {
        match self.gold(word) {
            Some(Entry::Tagged(tags)) => tags
                .get(tag)
                .cloned()
                .flatten()
                .unwrap_or_else(|| self.gold_plain(word)),
            _ => self.gold_plain(word),
        }
    }

    /// Spells a word letter by letter with the last letter stressed, as for
    /// an initialism. `None` when a letter has no entry.
    pub fn spell(&self, word: &str) -> Lookup {
        let mut joined = String::new();
        for c in word.chars().filter(|c| c.is_alphabetic()) {
            match self.gold(&c.to_uppercase().to_string()) {
                Some(Entry::Plain(ps)) => joined.push_str(ps),
                _ => return None,
            }
        }
        let ps = apply_stress(&joined, Some(0.0));
        let ps = match ps.rfind(SECONDARY) {
            Some(at) => format!("{}{PRIMARY}{}", &ps[..at], &ps[at + SECONDARY.len_utf8()..]),
            None => ps,
        };
        Some((ps, 3))
    }

    fn special_case(&self, word: &str, tag: &str, stress: Option<f32>, ctx: Context) -> Lookup {
        if let Some((_, name)) = SYMBOLS.iter().find(|(symbol, _)| *symbol == word) {
            return self.lookup(name, None, None, Some(ctx));
        }
        let inner = word.trim_matches('.');
        if inner.contains('.')
            && is_alpha(&word.replace('.', ""))
            && word
                .split('.')
                .map(|part| part.chars().count())
                .max()
                .unwrap_or(0)
                < 3
        {
            return self.spell(word);
        }
        match word {
            "a" | "A" => {
                return Some((if tag == "DT" { "ɐ" } else { "ˈA" }.to_string(), 4));
            }
            "am" | "Am" | "AM" => {
                if tag.starts_with("NN") {
                    return self.spell(word);
                }
                if ctx.future_vowel.is_none() || word != "am" || stress.is_some_and(|s| s > 0.0) {
                    return Some((self.gold_plain("am"), 4));
                }
                return Some(("ɐm".to_string(), 4));
            }
            "an" | "An" | "AN" => {
                if word == "AN" && tag.starts_with("NN") {
                    return self.spell(word);
                }
                return Some(("ɐn".to_string(), 4));
            }
            "I" if tag == "PRP" => return Some((format!("{SECONDARY}I"), 4)),
            "by" | "By" | "BY" if parent_tag(tag) == "ADV" => {
                return Some(("bˈI".to_string(), 4));
            }
            _ => {}
        }
        if word == "to" || word == "To" || (word == "TO" && (tag == "TO" || tag == "IN")) {
            let ps = match ctx.future_vowel {
                None => self.gold_plain("to"),
                Some(false) => "tə".to_string(),
                Some(true) => "tʊ".to_string(),
            };
            return Some((ps, 4));
        }
        if word == "in" || word == "In" || (word == "IN" && tag != "NNP") {
            let mark = if ctx.future_vowel.is_none() || tag != "IN" {
                PRIMARY.to_string()
            } else {
                String::new()
            };
            return Some((format!("{mark}ɪn"), 4));
        }
        if word == "the" || word == "The" || (word == "THE" && tag == "DT") {
            let ps = if ctx.future_vowel == Some(true) {
                "ði"
            } else {
                "ðə"
            };
            return Some((ps.to_string(), 4));
        }
        if tag == "IN" && matches!(word.to_lowercase().as_str(), "vs" | "vs.") {
            return self.lookup("versus", None, None, Some(ctx));
        }
        if matches!(word, "used" | "Used" | "USED") {
            if (tag == "VBD" || tag == "JJ") && ctx.future_to {
                return Some((self.gold_tagged("used", "VBD"), 4));
            }
            return Some((self.gold_tagged("used", "DEFAULT"), 4));
        }
        None
    }

    pub fn is_known(&self, word: &str) -> bool {
        if self.gold(word).is_some()
            || SYMBOLS.iter().any(|(s, _)| *s == word)
            || self.silver(word).is_some()
        {
            return true;
        }
        if !is_alpha(word) || !word.chars().all(lexicon_char) {
            return false;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if is_upper(word) && self.gold(&word.to_lowercase()).is_some() {
            return true;
        }
        let rest: String = word.chars().skip(1).collect();
        rest == rest.to_uppercase()
    }

    pub fn lookup(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f32>,
        ctx: Option<Context>,
    ) -> Lookup {
        let mut word = word.to_string();
        let mut tag = tag.map(str::to_string);
        let mut is_nnp = false;
        if is_upper(&word) && self.gold(&word).is_none() {
            word = word.to_lowercase();
            is_nnp = tag.as_deref() == Some("NNP");
        }
        let mut found = self.gold(&word).map(|entry| (entry, 4u8));
        if found.is_none() && !is_nnp {
            found = self.silver(&word).map(|entry| (entry, 3u8));
        }
        let (ps, rating) = match found {
            Some((Entry::Plain(ps), rating)) => (Some(ps.clone()), rating),
            Some((Entry::Tagged(tags), rating)) => {
                let wants_none =
                    ctx.is_some_and(|c| c.future_vowel.is_none()) && tags.contains_key("None");
                let key = if wants_none {
                    "None".to_string()
                } else {
                    let t = tag.take().unwrap_or_default();
                    if tags.contains_key(&t) {
                        t
                    } else {
                        parent_tag(&t).to_string()
                    }
                };
                let ps = match tags.get(&key) {
                    Some(value) => value.clone(),
                    None => tags.get("DEFAULT").cloned().flatten(),
                };
                (ps, rating)
            }
            None => (None, 4),
        };
        if ps.is_none() || (is_nnp && !ps.as_deref().unwrap_or("").contains(PRIMARY)) {
            if let Some(spelled) = self.spell(&word) {
                return Some(spelled);
            }
        }
        ps.map(|ps| (apply_stress(&ps, stress), rating))
    }

    pub fn suffix_s(&self, stem: &str) -> Option<String> {
        let last = stem.chars().last()?;
        if "ptkfθ".contains(last) {
            Some(format!("{stem}s"))
        } else if "szʃʒʧʤ".contains(last) {
            Some(format!("{stem}{}z", if self.british { 'ɪ' } else { 'ᵻ' }))
        } else {
            Some(format!("{stem}z"))
        }
    }

    pub fn stem_s(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f32>,
        ctx: Option<Context>,
    ) -> Lookup {
        let n = word.chars().count();
        if n < 3 || !word.ends_with('s') {
            return None;
        }
        let stem = if !word.ends_with("ss") && self.is_known(&word[..word.len() - 1]) {
            word[..word.len() - 1].to_string()
        } else if (word.ends_with("'s")
            || (n > 4 && word.ends_with("es") && !word.ends_with("ies")))
            && self.is_known(&word[..word.len() - 2])
        {
            word[..word.len() - 2].to_string()
        } else if n > 4
            && word.ends_with("ies")
            && self.is_known(&format!("{}y", &word[..word.len() - 3]))
        {
            format!("{}y", &word[..word.len() - 3])
        } else {
            return None;
        };
        let (ps, rating) = self.lookup(&stem, tag, stress, ctx)?;
        Some((self.suffix_s(&ps)?, rating))
    }

    pub fn suffix_ed(&self, stem: &str) -> Option<String> {
        let chars: Vec<char> = stem.chars().collect();
        let last = *chars.last()?;
        let reduced = if self.british { 'ɪ' } else { 'ᵻ' };
        if "pkfθʃsʧ".contains(last) {
            Some(format!("{stem}t"))
        } else if last == 'd' {
            Some(format!("{stem}{reduced}d"))
        } else if last != 't' {
            Some(format!("{stem}d"))
        } else if self.british || chars.len() < 2 {
            Some(format!("{stem}ɪd"))
        } else if US_TAUS.contains(chars[chars.len() - 2]) {
            let head: String = chars[..chars.len() - 1].iter().collect();
            Some(format!("{head}ɾᵻd"))
        } else {
            Some(format!("{stem}ᵻd"))
        }
    }

    fn stem_ed(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f32>,
        ctx: Option<Context>,
    ) -> Lookup {
        let n = word.chars().count();
        if n < 4 || !word.ends_with('d') {
            return None;
        }
        let stem = if !word.ends_with("dd") && self.is_known(&word[..word.len() - 1]) {
            &word[..word.len() - 1]
        } else if n > 4
            && word.ends_with("ed")
            && !word.ends_with("eed")
            && self.is_known(&word[..word.len() - 2])
        {
            &word[..word.len() - 2]
        } else {
            return None;
        };
        let (ps, rating) = self.lookup(stem, tag, stress, ctx)?;
        Some((self.suffix_ed(&ps)?, rating))
    }

    pub fn suffix_ing(&self, stem: &str) -> Option<String> {
        let chars: Vec<char> = stem.chars().collect();
        let last = *chars.last()?;
        if self.british {
            if last == 'ə' || last == 'ː' {
                return None;
            }
        } else if chars.len() > 1 && last == 't' && US_TAUS.contains(chars[chars.len() - 2]) {
            let head: String = chars[..chars.len() - 1].iter().collect();
            return Some(format!("{head}ɾɪŋ"));
        }
        Some(format!("{stem}ɪŋ"))
    }

    fn stem_ing(
        &self,
        word: &str,
        tag: Option<&str>,
        stress: Option<f32>,
        ctx: Option<Context>,
    ) -> Lookup {
        let n = word.chars().count();
        if n < 5 || !word.ends_with("ing") {
            return None;
        }
        let base = &word[..word.len() - 3];
        let doubled = {
            let chars: Vec<char> = base.chars().collect();
            let k = chars.len();
            (k >= 2 && chars[k - 1] == chars[k - 2] && "bcdgklmnprstvxz".contains(chars[k - 1]))
                || base.ends_with("ck")
        };
        let stem = if n > 5 && self.is_known(base) {
            base.to_string()
        } else if self.is_known(&format!("{base}e")) {
            format!("{base}e")
        } else if n > 5 && doubled && self.is_known(&base[..base.len() - 1]) {
            base[..base.len() - 1].to_string()
        } else {
            return None;
        };
        let (ps, rating) = self.lookup(&stem, tag, stress, ctx)?;
        Some((self.suffix_ing(&ps)?, rating))
    }

    fn get_word(&self, word: &str, tag: &str, stress: Option<f32>, ctx: Context) -> Lookup {
        if let Some(found) = self.special_case(word, tag, stress, ctx) {
            return Some(found);
        }
        let lower = word.to_lowercase();
        let mut word = word.to_string();
        let rest: String = word.chars().skip(1).collect();
        if word.chars().count() > 1
            && is_alpha(&word.replace('\'', ""))
            && word != lower
            && (tag != "NNP" || word.chars().count() > 7)
            && self.gold(&word).is_none()
            && self.silver(&word).is_none()
            && (is_upper(&word) || rest == rest.to_lowercase())
            && (self.gold(&lower).is_some()
                || self.silver(&lower).is_some()
                || self.stem_s(&lower, Some(tag), stress, Some(ctx)).is_some()
                || self.stem_ed(&lower, Some(tag), stress, Some(ctx)).is_some()
                || self
                    .stem_ing(&lower, Some(tag), stress, Some(ctx))
                    .is_some())
        {
            word = lower;
        }
        if self.is_known(&word) {
            return self.lookup(&word, Some(tag), stress, Some(ctx));
        }
        if word.ends_with("s'") {
            let possessive = format!("{}'s", &word[..word.len() - 2]);
            if self.is_known(&possessive) {
                return self.lookup(&possessive, Some(tag), stress, Some(ctx));
            }
        }
        if word.ends_with('\'') && self.is_known(&word[..word.len() - 1]) {
            return self.lookup(&word[..word.len() - 1], Some(tag), stress, Some(ctx));
        }
        if let Some(found) = self.stem_s(&word, Some(tag), stress, Some(ctx)) {
            return Some(found);
        }
        if let Some(found) = self.stem_ed(&word, Some(tag), stress, Some(ctx)) {
            return Some(found);
        }
        self.stem_ing(&word, Some(tag), Some(stress.unwrap_or(0.5)), Some(ctx))
    }

    /// misaki's `Lexicon.__call__` for one word: pronunciation and rating
    /// (4 gold, 3 silver or derived), or `None` when the word is unknown.
    pub fn word(&self, text: &str, tag: &str, ctx: Context) -> Lookup {
        let word = text.replace(['\u{2018}', '\u{2019}'], "'");
        let stress = if is_lower(&word) {
            None
        } else if is_upper(&word) {
            Some(2.0)
        } else {
            Some(0.5)
        };
        if let Some(found) = self.get_word(&word, tag, stress, ctx) {
            return Some(found);
        }
        None
    }
}

fn parse_dictionary(raw: &str) -> Result<HashMap<String, Entry>, String> {
    let value: HashMap<String, serde_json::Value> = serde_json::from_str(raw)
        .map_err(|e| format!("pronunciation dictionary is not valid JSON: {e}"))?;
    let mut out = HashMap::with_capacity(value.len());
    for (key, entry) in value {
        let entry = match entry {
            serde_json::Value::String(ps) => Entry::Plain(ps),
            serde_json::Value::Object(map) => {
                let tags = map
                    .into_iter()
                    .map(|(tag, ps)| (tag, ps.as_str().map(str::to_string)))
                    .collect::<HashMap<_, _>>();
                if !tags.contains_key("DEFAULT") {
                    return Err(format!("dictionary entry {key} has no DEFAULT"));
                }
                Entry::Tagged(tags)
            }
            _ => continue,
        };
        out.insert(key, entry);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Entries copied from misaki's `us_gold.json` (Apache-2.0) for the words
    /// these tests and `g2p`'s use, so they run without the download.
    pub const US_GOLD_SAMPLE: &str = r#"{
        "A": "ˈA", "B": "bˈi", "C": "sˈi", "D": "dˈi", "E": "ˈi", "G": "ʤˈi", "M": "ˈɛm",
        "N": "ˈɛn", "P": "pˈi", "R": "ˈɑɹ", "S": "ˈɛs", "V": "vˈi", "Q": "kjˈu", "L": "ˈɛl",
        "a": "A", "am": "æm", "an": "æn", "and": "ænd", "apple": "ˈæpᵊl", "box": "bˈɑks",
        "chocolate": "ʧˈɑkᵊlət", "children": "ʧˈɪldɹᵊn",
        "dog": "dˈɔɡ", "eat": "ˈit", "for": "fɔɹ", "get": {"DEFAULT": "ɡɛt", "None": "ɡˈɛt"}, "gonna": "ɡˈʌnə",
        "he": "hi", "hello": "həlˈO", "how": "hˌW", "I": "ˈI", "is": "ɪz", "it's": "ɪts",
        "James": "ʤˈAmz", "jump": "ʤˈʌmp", "keep": "kˈip", "kept": "kˈɛpt", "know": "nˈO",
        "known": "nˈOn", "life": "lˈIf", "like": "lˈIk", "live": {"DEFAULT": "lˈIv", "VERB": "lˈɪv"},
        "model": "mˈɑdᵊl", "never": "nˈɛvəɹ", "of": "ʌv", "read": {"ADJ": "ɹˈɛd", "DEFAULT": "ɹˈid", "VBD": "ɹˈɛd", "VBN": "ɹˈɛd", "VBP": "ɹˈɛd"},
        "record": {"DEFAULT": "ɹˈɛkəɹd", "VERB": "ɹəkˈɔɹd"}, "she": "ʃi", "state": "stˈAt",
        "art": "ˈɑɹt", "system": "sˈɪstəm", "talk": "tˈɔk", "the": "ði", "there": {"DEFAULT": "ðɛɹ", "None": "ðˈɛɹ"},
        "to": "tu", "used": {"DEFAULT": "jˈuzd", "VBD": "jˈust"}, "walk": "wˈɔk", "wait": "wˈAt",
        "want": "wˈɑnt", "well": "wˈɛl", "what": "wˌʌt", "you": "ju", "you're": "jʊɹ",
        "Tuesday": "tˈuzdˌA", "cat": "kˈæt", "versus": "vˈɜɹsəs", "SQL": "sˈikwəl",
        "code": "kˈOd"
    }"#;

    pub fn sample() -> Lexicon {
        Lexicon::from_json(false, US_GOLD_SAMPLE, "{}").unwrap()
    }

    fn ctx(future_vowel: Option<bool>) -> Context {
        Context {
            future_vowel,
            future_to: false,
        }
    }

    #[test]
    fn stress_rules_match_misaki() {
        assert_eq!(apply_stress("ʃˌi", Some(-1.0)), "ʃi");
        assert_eq!(apply_stress("wˈɛl", Some(-0.5)), "wˌɛl");
        assert_eq!(apply_stress("kæt", Some(0.5)), "kˌæt");
        assert_eq!(apply_stress("kæt", Some(2.0)), "kˈæt");
        assert_eq!(apply_stress("bˈɑks", Some(-2.0)), "bɑks");
        assert_eq!(apply_stress("ʃˌi", Some(1.0)), "ʃˈi");
        assert_eq!(apply_stress("pst", Some(2.0)), "pst");
    }

    #[test]
    fn capitalised_and_lowercased_forms_resolve() {
        let lex = sample();
        assert_eq!(lex.word("Life", "NN", ctx(None)).unwrap().0, "lˈIf");
        assert_eq!(lex.word("tuesday", "NN", ctx(None)).unwrap().0, "tˈuzdˌA");
    }

    #[test]
    fn function_words_follow_the_next_sound() {
        let lex = sample();
        assert_eq!(lex.word("the", "DT", ctx(Some(true))).unwrap().0, "ði");
        assert_eq!(lex.word("the", "DT", ctx(Some(false))).unwrap().0, "ðə");
        assert_eq!(lex.word("to", "TO", ctx(Some(true))).unwrap().0, "tʊ");
        assert_eq!(lex.word("to", "TO", ctx(Some(false))).unwrap().0, "tə");
        assert_eq!(lex.word("a", "DT", ctx(Some(false))).unwrap().0, "ɐ");
        assert_eq!(lex.word("an", "DT", ctx(Some(true))).unwrap().0, "ɐn");
        assert_eq!(lex.word("I", "PRP", ctx(Some(false))).unwrap().0, "ˌI");
        assert_eq!(lex.word("in", "IN", ctx(Some(false))).unwrap().0, "ɪn");
        assert_eq!(lex.word("in", "RB", ctx(None)).unwrap().0, "ˈɪn");
    }

    #[test]
    fn heteronyms_follow_the_tag() {
        let lex = sample();
        assert_eq!(lex.word("read", "VB", ctx(Some(false))).unwrap().0, "ɹˈid");
        assert_eq!(lex.word("read", "VBD", ctx(Some(false))).unwrap().0, "ɹˈɛd");
        assert_eq!(
            lex.word("record", "VB", ctx(Some(false))).unwrap().0,
            "ɹəkˈɔɹd"
        );
        assert_eq!(
            lex.word("record", "NN", ctx(Some(false))).unwrap().0,
            "ɹˈɛkəɹd"
        );
        assert_eq!(lex.word("live", "VBP", ctx(Some(false))).unwrap().0, "lˈɪv");
        assert_eq!(lex.word("live", "JJ", ctx(Some(false))).unwrap().0, "lˈIv");
        let used_to = Context {
            future_vowel: Some(false),
            future_to: true,
        };
        assert_eq!(lex.word("used", "VBD", used_to).unwrap().0, "jˈust");
    }

    #[test]
    fn stems_add_s_ed_and_ing() {
        let lex = sample();
        assert_eq!(lex.word("cats", "NNS", ctx(None)).unwrap().0, "kˈæts");
        assert_eq!(lex.word("walked", "VBD", ctx(None)).unwrap().0, "wˈɔkt");
        assert_eq!(lex.word("wanted", "VBD", ctx(None)).unwrap().0, "wˈɑntᵻd");
        assert_eq!(lex.word("jumping", "VBG", ctx(None)).unwrap().0, "ʤˈʌmpɪŋ");
        assert_eq!(lex.word("waiting", "VBG", ctx(None)).unwrap().0, "wˈAɾɪŋ");
        assert_eq!(lex.word("models", "NNS", ctx(None)).unwrap().0, "mˈɑdᵊlz");
        assert_eq!(lex.word("talked", "VBD", ctx(None)).unwrap().0, "tˈɔkt");
    }

    #[test]
    fn possessives_resolve_through_the_stem() {
        let lex = sample();
        assert_eq!(lex.word("James'", "NNP", ctx(None)).unwrap().0, "ʤˈAmz");
        assert_eq!(
            lex.word("children's", "NNS", ctx(None)).unwrap().0,
            "ʧˈɪldɹᵊnz"
        );
    }

    #[test]
    fn initialisms_are_spelled_with_the_last_letter_stressed() {
        let lex = sample();
        assert_eq!(lex.word("VS", "NNP", ctx(None)).unwrap().0, "vˌiˈɛs");
        assert_eq!(lex.spell("G2P"), Some(("ʤˌipˈi".to_string(), 3)));
        assert_eq!(lex.word("AM", "NNP", ctx(None)).unwrap().0, "ˌAˈɛm");
    }

    #[test]
    fn unknown_words_are_none() {
        let lex = sample();
        assert_eq!(lex.word("Kokoro", "NNP", ctx(None)), None);
        assert_eq!(lex.word("snarfblat", "NN", ctx(None)), None);
    }

    #[test]
    fn a_dictionary_without_default_is_refused() {
        assert!(Lexicon::from_json(false, r#"{"x": {"VERB": "x"}}"#, "{}").is_err());
    }
}
