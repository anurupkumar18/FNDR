//! Policy helpers for knowledge pages and the append-only decision posture.
//!
//! Contradictions **do not** delete historical decision rows; they mark affected
//! wiki pages as contradicted so downstream surfaces can surface both claims.

use crate::storage::{KnowledgePage, KnowledgeStability};

/// When new evidence contradicts a compiled page, move stability to contradicted.
#[allow(dead_code)]
pub fn apply_contradiction_signal(page: &mut KnowledgePage) {
    page.stability = KnowledgeStability::Contradicted;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{KnowledgePage, KnowledgePageType, KnowledgeStability};

    #[test]
    fn contradiction_marks_page() {
        let mut page = KnowledgePage {
            page_type: KnowledgePageType::DecisionPage,
            stability: KnowledgeStability::Stable,
            ..Default::default()
        };
        apply_contradiction_signal(&mut page);
        assert_eq!(page.stability, KnowledgeStability::Contradicted);
    }
}
