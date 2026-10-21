//! Phonemes to Kokoro token ids, and splitting a phoneme string so each piece
//! fits the model's context.
//!
//! The table is Kokoro-82M v1.0's `vocab` (hexgrad/Kokoro-82M `config.json`,
//! identical to the ONNX export's `tokenizer.json`). Id 0 pads both ends.

/// The model's context is 512 ids; two are the padding at each end.
pub const MAX_TOKENS: usize = 510;

pub const PAD_ID: i64 = 0;

const VOCAB: &[(char, i64)] = &[
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('—', 9),
    ('…', 10),
    ('"', 11),
    ('(', 12),
    (')', 13),
    ('“', 14),
    ('”', 15),
    (' ', 16),
    ('\u{0303}', 17),
    ('ʣ', 18),
    ('ʥ', 19),
    ('ʦ', 20),
    ('ʨ', 21),
    ('ᵝ', 22),
    ('ꭧ', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('ᵊ', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('ɑ', 69),
    ('ɐ', 70),
    ('ɒ', 71),
    ('æ', 72),
    ('β', 75),
    ('ɔ', 76),
    ('ɕ', 77),
    ('ç', 78),
    ('ɖ', 80),
    ('ð', 81),
    ('ʤ', 82),
    ('ə', 83),
    ('ɚ', 85),
    ('ɛ', 86),
    ('ɜ', 87),
    ('ɟ', 90),
    ('ɡ', 92),
    ('ɥ', 99),
    ('ɨ', 101),
    ('ɪ', 102),
    ('ʝ', 103),
    ('ɯ', 110),
    ('ɰ', 111),
    ('ŋ', 112),
    ('ɳ', 113),
    ('ɲ', 114),
    ('ɴ', 115),
    ('ø', 116),
    ('ɸ', 118),
    ('θ', 119),
    ('œ', 120),
    ('ɹ', 123),
    ('ɾ', 125),
    ('ɻ', 126),
    ('ʁ', 128),
    ('ɽ', 129),
    ('ʂ', 130),
    ('ʃ', 131),
    ('ʈ', 132),
    ('ʧ', 133),
    ('ʊ', 135),
    ('ʋ', 136),
    ('ʌ', 138),
    ('ɣ', 139),
    ('ɤ', 140),
    ('χ', 142),
    ('ʎ', 143),
    ('ʒ', 147),
    ('ʔ', 148),
    ('ˈ', 156),
    ('ˌ', 157),
    ('ː', 158),
    ('ʰ', 162),
    ('ʲ', 164),
    ('↓', 169),
    ('→', 171),
    ('↗', 172),
    ('↘', 173),
    ('ᵻ', 177),
];

pub fn token_id(c: char) -> Option<i64> {
    VOCAB.iter().find(|(p, _)| *p == c).map(|(_, id)| *id)
}

/// Ids for a phoneme string. Characters outside the vocabulary are dropped,
/// as the export's tokenizer normalizer does.
pub fn encode(phonemes: &str) -> Vec<i64> {
    phonemes.chars().filter_map(token_id).collect()
}

fn token_len(phonemes: &str) -> usize {
    phonemes.chars().filter(|c| token_id(*c).is_some()).count()
}

/// Splits a phoneme string into pieces of at most `MAX_TOKENS` ids. A piece
/// ends after the last clause punctuation that fits, else at the last space,
/// else at the limit itself.
pub fn split_for_model(phonemes: &str) -> Vec<String> {
    split_with_limit(phonemes, MAX_TOKENS)
}

fn split_with_limit(phonemes: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest: Vec<char> = phonemes.trim().chars().collect();
    while token_len(&rest.iter().collect::<String>()) > limit {
        let mut counted = 0;
        let mut end = rest.len();
        for (index, c) in rest.iter().enumerate() {
            if token_id(*c).is_some() {
                counted += 1;
            }
            if counted > limit {
                end = index;
                break;
            }
        }
        let window = &rest[..end];
        let cut = window
            .iter()
            .rposition(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | ',' | '—' | '…'))
            .map(|i| i + 1)
            .or_else(|| window.iter().rposition(|c| *c == ' '))
            .filter(|i| *i > 0)
            .unwrap_or(end);
        let piece: String = rest[..cut].iter().collect();
        if !piece.trim().is_empty() {
            pieces.push(piece.trim().to_string());
        }
        rest = rest[cut..].to_vec();
        while rest.first() == Some(&' ') {
            rest.remove(0);
        }
    }
    let tail: String = rest.iter().collect();
    if !tail.trim().is_empty() {
        pieces.push(tail.trim().to_string());
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token list printed in the ONNX export's model card (its "How could I
    /// know? It's an unanswerable question..." sample, in espeak-style
    /// phonemes).
    const MODEL_CARD_TOKENS: &[i64] = &[
        50, 157, 43, 135, 16, 53, 135, 46, 16, 43, 102, 16, 56, 156, 57, 135, 6, 16, 102, 62, 61,
        16, 70, 56, 16, 138, 56, 156, 72, 56, 61, 85, 123, 83, 44, 83, 54, 16, 53, 65, 156, 86, 61,
        62, 131, 83, 56, 4, 16, 54, 156, 43, 102, 53, 16, 156, 72, 61, 53, 102, 112, 16, 70, 56,
        16, 138, 56, 44, 156, 76, 158, 123, 56, 16, 62, 131, 156, 43, 102, 54, 46, 16, 102, 48, 16,
        81, 47, 102, 54, 16, 54, 156, 51, 158, 46, 16, 70, 16, 92, 156, 135, 46, 16, 54, 156, 43,
        102, 48, 4, 16, 81, 47, 102, 16, 50, 156, 72, 64, 83, 56, 62, 16, 156, 51, 158, 64, 83, 56,
        16, 44, 157, 102, 56, 16, 44, 156, 76, 158, 123, 56, 4,
    ];

    #[test]
    fn model_card_ids_decode_to_their_phonemes_and_encode_back() {
        let decoded: String = MODEL_CARD_TOKENS
            .iter()
            .map(|id| VOCAB.iter().find(|(_, v)| v == id).unwrap().0)
            .collect();
        assert!(decoded.starts_with("hˌaʊ kʊd aɪ nˈoʊ? ɪts ɐn ʌnˈænsɚɹəbəl kwˈɛstʃən."));
        assert_eq!(encode(&decoded), MODEL_CARD_TOKENS);
    }

    #[test]
    fn misaki_phonemes_map_to_known_ids() {
        assert_eq!(
            encode("ðə kˈæts."),
            vec![81, 83, 16, 53, 156, 72, 62, 61, 4]
        );
        assert_eq!(encode("ˈOpᵊn"), vec![156, 31, 58, 42, 56]);
    }

    #[test]
    fn unknown_characters_are_dropped() {
        assert_eq!(encode("a❓b"), vec![43, 44]);
        assert_eq!(token_id('$'), None);
    }

    #[test]
    fn short_text_stays_one_piece() {
        assert_eq!(split_for_model("hˈɛlO."), vec!["hˈɛlO.".to_string()]);
    }

    #[test]
    fn long_text_splits_at_clause_punctuation_within_the_limit() {
        let pieces = split_with_limit("ab cd, ef gh. ij kl", 10);
        assert_eq!(pieces, vec!["ab cd,", "ef gh.", "ij kl"]);
        for piece in &pieces {
            assert!(encode(piece).len() <= 10);
        }
    }

    #[test]
    fn a_run_without_breaks_is_cut_at_the_limit() {
        let run = "a".repeat(1200);
        let pieces = split_for_model(&run);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| encode(p).len() <= MAX_TOKENS));
        assert_eq!(pieces.iter().map(|p| p.len()).sum::<usize>(), 1200);
    }

    #[test]
    fn every_piece_of_a_long_answer_fits_the_model() {
        let sentence = "ðə kwˈɪk bɹˈWn fˈɑks ʤˈʌmps ˈOvəɹ ðə lˈAzi dˈɔɡ, ";
        let long = sentence.repeat(40);
        let pieces = split_for_model(&long);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(encode(piece).len() <= MAX_TOKENS, "{}", piece.len());
            assert!(piece.ends_with(','), "{piece}");
        }
    }
}
