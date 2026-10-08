//! Reading text out of an Accessibility tree under a node and time budget.
//!
//! The walk is generic over `TextTree` so ordering, privacy, and budget
//! behavior are unit-tested without a Mac window. `mod.rs` supplies the
//! AXUIElement-backed tree.

use std::time::{Duration, Instant};

pub const DEFAULT_MAX_NODES: usize = 4_000;
pub const DEFAULT_MAX_TIME: Duration = Duration::from_millis(50);

const TEXT_ROLES: [&str; 4] = ["AXStaticText", "AXTextArea", "AXTextField", "AXHeading"];
const SECURE_MARKER: &str = "AXSecureTextField";
const WEB_AREA: &str = "AXWebArea";

pub trait TextTree {
    type Node;
    fn role(&self, node: &Self::Node) -> Option<String>;
    fn subrole(&self, node: &Self::Node) -> Option<String>;
    /// AXValue, falling back to AXTitle, as plain text.
    fn text(&self, node: &Self::Node) -> Option<String>;
    /// Children in the order the application reports them (reading order).
    fn children(&self, node: &Self::Node) -> Vec<Self::Node>;
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub max_chars: usize,
    pub max_nodes: usize,
    pub max_time: Duration,
}

impl Budget {
    pub fn new(max_chars: usize) -> Self {
        Self {
            max_chars,
            max_nodes: DEFAULT_MAX_NODES,
            max_time: DEFAULT_MAX_TIME,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Complete,
    NodeBudget,
    TimeBudget,
    CharBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOutcome {
    pub text: String,
    pub nodes_visited: usize,
    pub secure_fields_skipped: usize,
    /// A web area was visited, even if it exposed no readable page text.
    pub from_web_area: bool,
    /// Visited-node count per AX role, for diagnosing apps that expose little.
    pub role_counts: std::collections::BTreeMap<String, usize>,
    pub stop: StopReason,
}

#[derive(Default)]
struct Collector {
    all: Vec<String>,
    web: Vec<String>,
    chars: usize,
}

impl Collector {
    fn push(&mut self, text: String, in_web: bool) {
        self.chars += text.chars().count() + 1;
        if in_web && self.web.last() != Some(&text) {
            self.web.push(text.clone());
        }
        if self.all.last() != Some(&text) {
            self.all.push(text);
        }
    }
}

fn is_secure<T: TextTree>(tree: &T, node: &T::Node) -> bool {
    tree.role(node).as_deref() == Some(SECURE_MARKER)
        || tree.subrole(node).as_deref() == Some(SECURE_MARKER)
}

/// Depth-first, pre-order walk. Secure text fields (and everything under them)
/// are never read. When any `AXWebArea` exists only its text is returned, so
/// browser toolbars and tab titles stay out of the page text.
pub fn collect_text<T: TextTree>(tree: &T, root: &T::Node, budget: Budget) -> TextOutcome {
    let started = Instant::now();
    let mut collector = Collector::default();
    let mut nodes_visited = 0usize;
    let mut secure_skipped = 0usize;
    let mut stop = StopReason::Complete;
    let mut role_counts = std::collections::BTreeMap::new();
    // Explicit stack of (node, inside web area); children pushed reversed so
    // they pop in reported order.
    let mut stack: Vec<(T::Node, bool)> = Vec::new();
    for child in tree.children(root).into_iter().rev() {
        stack.push((child, false));
    }
    while let Some((node, in_web)) = stack.pop() {
        if nodes_visited >= budget.max_nodes {
            stop = StopReason::NodeBudget;
            break;
        }
        if started.elapsed() >= budget.max_time {
            stop = StopReason::TimeBudget;
            break;
        }
        if collector.chars >= budget.max_chars {
            stop = StopReason::CharBudget;
            break;
        }
        nodes_visited += 1;
        if is_secure(tree, &node) {
            secure_skipped += 1;
            continue;
        }
        let role = tree.role(&node);
        let role = role.as_deref().unwrap_or("");
        *role_counts.entry(role.to_string()).or_insert(0) += 1;
        let in_web = in_web || role == WEB_AREA;
        if TEXT_ROLES.contains(&role) {
            if let Some(text) = tree.text(&node) {
                let text = text.trim();
                if !text.is_empty() {
                    collector.push(text.to_string(), in_web);
                }
            }
            // Text containers keep their text in the value; do not descend.
            if role != "AXHeading" {
                continue;
            }
        }
        for child in tree.children(&node).into_iter().rev() {
            stack.push((child, in_web));
        }
    }
    let from_web_area = role_counts.contains_key(WEB_AREA);
    let parts = if from_web_area {
        collector.web
    } else {
        collector.all
    };
    let mut text = parts.join("\n");
    if text.chars().count() > budget.max_chars {
        text = text.chars().take(budget.max_chars).collect();
        stop = StopReason::CharBudget;
    }
    TextOutcome {
        text,
        nodes_visited,
        secure_fields_skipped: secure_skipped,
        from_web_area,
        role_counts,
        stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Fake {
        role: &'static str,
        subrole: &'static str,
        text: &'static str,
        children: Vec<Fake>,
    }

    fn n(role: &'static str, text: &'static str, children: Vec<Fake>) -> Fake {
        Fake {
            role,
            subrole: "",
            text,
            children,
        }
    }

    struct FakeTree;
    impl TextTree for FakeTree {
        type Node = Fake;
        fn role(&self, node: &Fake) -> Option<String> {
            Some(node.role.to_string())
        }
        fn subrole(&self, node: &Fake) -> Option<String> {
            (!node.subrole.is_empty()).then(|| node.subrole.to_string())
        }
        fn text(&self, node: &Fake) -> Option<String> {
            (!node.text.is_empty()).then(|| node.text.to_string())
        }
        fn children(&self, node: &Fake) -> Vec<Fake> {
            node.children.clone()
        }
    }

    fn run(root: &Fake, budget: Budget) -> TextOutcome {
        collect_text(&FakeTree, root, budget)
    }

    #[test]
    fn reads_in_reported_order_and_skips_adjacent_repeats() {
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", "First paragraph", vec![]),
                n("AXGroup", "", vec![n("AXStaticText", "Second", vec![])]),
                n("AXStaticText", "Second", vec![]),
                n("AXTextArea", "Third", vec![]),
            ],
        );
        let out = run(&root, Budget::new(1_000));
        assert_eq!(out.text, "First paragraph\nSecond\nThird");
        assert_eq!(out.stop, StopReason::Complete);
    }

    #[test]
    fn secure_fields_and_their_subtrees_are_never_read() {
        let mut secure = n(
            "AXTextField",
            "hunter2",
            vec![n("AXStaticText", "inner", vec![])],
        );
        secure.subrole = SECURE_MARKER;
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", "Sign in", vec![]),
                secure,
                n("AXSecureTextField", "also secret", vec![]),
                n("AXStaticText", "Forgot password", vec![]),
            ],
        );
        let out = run(&root, Budget::new(1_000));
        assert_eq!(out.text, "Sign in\nForgot password");
        assert_eq!(out.secure_fields_skipped, 2);
        assert!(!out.text.contains("hunter2") && !out.text.contains("secret"));
    }

    #[test]
    fn web_area_text_wins_over_browser_chrome() {
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", "Tab title", vec![]),
                n(
                    "AXWebArea",
                    "",
                    vec![
                        n("AXHeading", "What is Mitosis?", vec![]),
                        n("AXStaticText", "Cells divide.", vec![]),
                    ],
                ),
            ],
        );
        let out = run(&root, Budget::new(1_000));
        assert!(out.from_web_area);
        assert_eq!(out.text, "What is Mitosis?\nCells divide.");
    }

    #[test]
    fn empty_web_area_never_substitutes_browser_chrome_for_page_text() {
        let chrome = concat!(
            "Browser tab and toolbar labels. Browser tab and toolbar labels. ",
            "Browser tab and toolbar labels. Browser tab and toolbar labels. ",
            "Browser tab and toolbar labels. Browser tab and toolbar labels. ",
            "Browser tab and toolbar labels. Browser tab and toolbar labels. ",
        );
        assert!(chrome.chars().count() >= 200);
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", chrome, vec![]),
                n("AXWebArea", "", vec![]),
            ],
        );
        let out = run(&root, Budget::new(1_000));
        assert!(out.text.is_empty(), "empty page must fall back to OCR");
        assert!(out.from_web_area);
        assert_eq!(out.stop, StopReason::Complete);
    }

    #[test]
    fn secure_only_web_area_never_substitutes_browser_chrome() {
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", "Toolbar text", vec![]),
                n(
                    "AXWebArea",
                    "",
                    vec![n("AXSecureTextField", "private secret", vec![])],
                ),
            ],
        );
        let out = run(&root, Budget::new(1_000));
        assert!(out.text.is_empty());
        assert!(out.from_web_area);
        assert_eq!(out.secure_fields_skipped, 1);
    }

    #[test]
    fn without_a_web_area_the_whole_window_is_read() {
        let root = n(
            "AXWindow",
            "",
            vec![n("AXTextArea", "Document body", vec![])],
        );
        let out = run(&root, Budget::new(1_000));
        assert!(!out.from_web_area);
        assert_eq!(out.text, "Document body");
    }

    #[test]
    fn node_budget_stops_the_walk() {
        let kids = (0..50)
            .map(|_| n("AXGroup", "", vec![]))
            .collect::<Vec<_>>();
        let mut budget = Budget::new(1_000);
        budget.max_nodes = 10;
        let out = run(&n("AXWindow", "", kids), budget);
        assert_eq!(out.stop, StopReason::NodeBudget);
        assert_eq!(out.nodes_visited, 10);
    }

    #[test]
    fn char_budget_truncates_and_reports_it() {
        let root = n(
            "AXWindow",
            "",
            vec![
                n("AXStaticText", "aaaaaaaaaa", vec![]),
                n("AXStaticText", "bbbbbbbbbb", vec![]),
            ],
        );
        let out = run(&root, Budget::new(15));
        assert_eq!(out.text.chars().count(), 15);
        assert_eq!(out.stop, StopReason::CharBudget);
    }

    #[test]
    fn time_budget_stops_the_walk() {
        let mut budget = Budget::new(1_000);
        budget.max_time = Duration::ZERO;
        let out = run(
            &n("AXWindow", "", vec![n("AXStaticText", "late", vec![])]),
            budget,
        );
        assert_eq!(out.stop, StopReason::TimeBudget);
        assert!(out.text.is_empty());
    }
}
