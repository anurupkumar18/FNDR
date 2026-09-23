//! Greedy, order-preserving token-budget packing for Resume Work evidence.
//! Each kept item carries its own `memory_id`, which is the citation a
//! `ResumeThread` points back at.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PackItem {
    pub memory_id: String,
    pub text: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PackResult {
    pub items: Vec<PackItem>,
    pub dropped_for_budget: usize,
    pub estimated_tokens: usize,
}

/// Four characters per token, the same rough rule v2's context pack used.
/// There is no tokenizer on this path.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Greedily keep candidates, in the order given, while they fit
/// `budget_tokens`. A candidate that does not fit is dropped (and
/// counted); smaller later ones may still fit.
pub fn pack_within_budget(candidates: Vec<PackItem>, budget_tokens: usize) -> PackResult {
    let mut used = 0usize;
    let mut items = Vec::new();
    let mut dropped = 0usize;
    for candidate in candidates {
        let cost = estimate_tokens(&candidate.text);
        if used + cost <= budget_tokens {
            used += cost;
            items.push(candidate);
        } else {
            dropped += 1;
        }
    }
    PackResult {
        items,
        dropped_for_budget: dropped,
        estimated_tokens: used,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, chars: usize) -> PackItem {
        PackItem {
            memory_id: id.to_string(),
            text: "x".repeat(chars),
            ts_ms: 0,
        }
    }

    #[test]
    fn packing_keeps_order_counts_drops_and_preserves_citations() {
        // 200 chars = 50 tokens each; a budget of 100 tokens fits exactly two.
        let r = pack_within_budget(vec![item("a", 200), item("b", 200), item("c", 200)], 100);
        assert_eq!(
            r.items
                .iter()
                .map(|i| i.memory_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(r.dropped_for_budget, 1);
        assert_eq!(r.estimated_tokens, 100);
    }

    #[test]
    fn a_small_later_item_can_still_fit_after_a_large_one_is_dropped() {
        let r = pack_within_budget(vec![item("big", 4000), item("small", 40)], 100);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].memory_id, "small");
        assert_eq!(r.dropped_for_budget, 1);
    }

    #[test]
    fn zero_budget_keeps_nothing_and_never_panics() {
        let r = pack_within_budget(vec![item("a", 4)], 0);
        assert!(r.items.is_empty());
        assert_eq!(r.dropped_for_budget, 1);
        assert_eq!(pack_within_budget(vec![], 100).estimated_tokens, 0);
    }
}
