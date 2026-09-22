/// Character error rate: Levenshtein distance over characters divided by reference length.
pub fn char_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let r: Vec<char> = reference.chars().collect();
    let h: Vec<char> = hypothesis.chars().collect();
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rc) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, hc) in h.iter().enumerate() {
            let cost = if rc == hc { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_zero_and_one_substitution_in_ten_is_a_tenth() {
        assert_eq!(char_error_rate("abcdefghij", "abcdefghij"), 0.0);
        assert!((char_error_rate("abcdefghij", "abcdeXghij") - 0.1).abs() < 1e-9);
    }

    #[test]
    fn empty_reference_and_unicode_are_handled() {
        assert_eq!(char_error_rate("", ""), 0.0);
        assert_eq!(char_error_rate("", "x"), 1.0);
        assert_eq!(char_error_rate("naïve café", "naïve café"), 0.0);
    }
}
