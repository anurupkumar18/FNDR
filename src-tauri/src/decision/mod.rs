//! Bounded decisions with confidence and an escalation cascade across tiers.
//!
//! A decider picks one option from a fixed list and reports probabilities.
//! The cascade accepts the first tier that reaches its configured confidence
//! threshold. Every other outcome remains unresolved for review or a person.

pub mod laya_bridge;

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionOutcome {
    pub choice: usize,
    pub probs: Vec<f32>,
    pub confidence: f32,
    pub tier: &'static str,
}

pub trait Decider: Send + Sync {
    fn name(&self) -> &'static str;
    fn options(&self) -> &'static [&'static str];
    fn decide(&self, evidence: &str) -> DecisionOutcome;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// A tier was confident enough to accept.
    Accepted {
        outcome: DecisionOutcome,
        escalated_from: Vec<&'static str>,
    },
    /// No tier was confident enough. Preserve the best guess for review.
    Unresolved {
        best_guess: DecisionOutcome,
        tried: Vec<&'static str>,
    },
}

pub struct Tier {
    pub decider: Box<dyn Decider>,
    pub accept_at: f32,
}

pub fn cascade(tiers: &[Tier], evidence: &str) -> Option<Resolution> {
    let mut escalated = Vec::new();
    let mut best: Option<DecisionOutcome> = None;

    for tier in tiers {
        let outcome = tier.decider.decide(evidence);
        if outcome.confidence >= tier.accept_at {
            return Some(Resolution::Accepted {
                outcome,
                escalated_from: escalated,
            });
        }
        escalated.push(tier.decider.name());
        if best
            .as_ref()
            .map_or(true, |current| outcome.confidence > current.confidence)
        {
            best = Some(outcome);
        }
    }

    best.map(|best_guess| Resolution::Unresolved {
        best_guess,
        tried: escalated,
    })
}

pub struct LayaDecider {
    pub question_key: &'static str,
    pub instructions: &'static str,
    pub criteria: &'static [(&'static str, &'static str)],
    pub options: &'static [&'static str],
}

impl Decider for LayaDecider {
    fn name(&self) -> &'static str {
        "laya"
    }

    fn options(&self) -> &'static [&'static str] {
        self.options
    }

    fn decide(&self, evidence: &str) -> DecisionOutcome {
        let state = serde_json::json!({"text": evidence});
        let criteria: serde_json::Map<String, serde_json::Value> = self
            .criteria
            .iter()
            .map(|(key, value)| {
                (
                    (*key).to_string(),
                    serde_json::Value::String((*value).to_string()),
                )
            })
            .collect();
        let questions = serde_json::json!({
            self.question_key: {
                "type": "choice",
                "instructions": self.instructions,
                "criteria": criteria,
            }
        });

        match laya_bridge::run_laya_predict(&state, &questions) {
            Ok(result) => {
                let answer = &result["answers"][self.question_key];
                let choice_label = answer["choice"].as_str().unwrap_or("");
                let confidence = answer["confidence"].as_f64().unwrap_or(0.0) as f32;
                let choice = self
                    .options
                    .iter()
                    .position(|option| *option == choice_label)
                    .unwrap_or(0);
                let mut probs = vec![0.0; self.options.len()];
                if choice < probs.len() {
                    probs[choice] = confidence;
                }
                DecisionOutcome {
                    choice,
                    probs,
                    confidence,
                    tier: "laya",
                }
            }
            Err(_) => DecisionOutcome {
                choice: 0,
                probs: vec![0.0; self.options.len()],
                confidence: 0.0,
                tier: "laya",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        name: &'static str,
        pick: usize,
        conf: f32,
    }

    impl Decider for Fixed {
        fn name(&self) -> &'static str {
            self.name
        }

        fn options(&self) -> &'static [&'static str] {
            &["merge", "append", "new"]
        }

        fn decide(&self, _: &str) -> DecisionOutcome {
            let mut probs = vec![0.0; 3];
            probs[self.pick] = self.conf;
            DecisionOutcome {
                choice: self.pick,
                probs,
                confidence: self.conf,
                tier: self.name,
            }
        }
    }

    fn tier(name: &'static str, pick: usize, conf: f32, accept_at: f32) -> Tier {
        Tier {
            decider: Box::new(Fixed { name, pick, conf }),
            accept_at,
        }
    }

    #[test]
    fn the_first_confident_tier_wins_and_later_tiers_are_never_called() {
        let tiers = vec![tier("rules", 0, 0.97, 0.9), tier("llm", 2, 0.99, 0.5)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Accepted {
                outcome,
                escalated_from,
            } => {
                assert_eq!(outcome.tier, "rules");
                assert!(escalated_from.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_unsure_tier_escalates_and_records_who_was_skipped() {
        let tiers = vec![tier("rules", 0, 0.60, 0.9), tier("classifier", 1, 0.95, 0.9)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Accepted {
                outcome,
                escalated_from,
            } => {
                assert_eq!(outcome.choice, 1);
                assert_eq!(escalated_from, vec!["rules"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn when_nobody_is_confident_the_best_guess_is_kept_for_review() {
        let tiers = vec![tier("rules", 0, 0.55, 0.9), tier("llm", 2, 0.70, 0.9)];
        match cascade(&tiers, "x").unwrap() {
            Resolution::Unresolved { best_guess, tried } => {
                assert_eq!(best_guess.choice, 2);
                assert_eq!(tried, vec!["rules", "llm"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn no_tiers_means_no_resolution() {
        assert!(cascade(&[], "x").is_none());
    }
}
