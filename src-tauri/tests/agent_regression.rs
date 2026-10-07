#[cfg(test)]
mod agent_regression {
    use fndr_lib::agent::audit::list_agent_audit_runs;
    use fndr_lib::agent::context::run_agent_request;
    use fndr_lib::agent::{
        build_agent_context_pack, validate_command, AgentContextRequest, AgentMode, PermissionScope,
    };
    use fndr_lib::config::Config;
    use fndr_lib::graph::GraphStore;
    use fndr_lib::storage::{MemoryRecord, StateStore, Store};
    use fndr_lib::AppState;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    /// A temporary store with two synthetic memories. Route budgets are
    /// lifted so a loaded test machine cannot drop a hit between two runs.
    fn setup_test_state(rt: &Runtime) -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let store = Arc::new(Store::new(&data_dir).expect("store"));
        let state_store = Arc::new(StateStore::new(&data_dir).expect("state store"));
        let mut config = Config::default();
        config.search.semantic_timeout_ms = 10_000;
        config.search.snippet_timeout_ms = 10_000;
        config.search.keyword_timeout_ms = 10_000;
        config.search.keyword_variant_timeout_ms = 5_000;
        let now = chrono::Utc::now().timestamp_millis();
        let memories = [
            (
                "roadmap",
                "Safari",
                "Roadmap review: Beta demo stays on Wednesday.",
            ),
            (
                "tests",
                "Terminal",
                "cargo test --lib capture: 42 passed, 0 failed.",
            ),
        ]
        .iter()
        .enumerate()
        .map(|(index, (id, app, text))| MemoryRecord {
            id: id.to_string(),
            timestamp: now - 60_000 * (index as i64 + 1),
            app_name: app.to_string(),
            window_title: text.to_string(),
            text: text.to_string(),
            clean_text: text.to_string(),
            snippet: text.to_string(),
            ..MemoryRecord::default()
        })
        .collect::<Vec<_>>();
        rt.block_on(store.add_batch_preserving_ids(&memories))
            .expect("seed");
        let graph = GraphStore::new(store.clone());
        let state = Arc::new(AppState::new(
            data_dir,
            config,
            store,
            state_store,
            graph,
            None,
        ));
        (dir, state)
    }

    fn request(mode: AgentMode) -> AgentContextRequest {
        AgentContextRequest {
            user_goal: "roadmap review beta demo".to_string(),
            mode,
            ..Default::default()
        }
    }

    #[test]
    fn agent_context_pack_is_deterministic() {
        let rt = Runtime::new().unwrap();
        let (_dir, state) = setup_test_state(&rt);
        let pack1 = rt
            .block_on(build_agent_context_pack(&state, request(AgentMode::Ask)))
            .unwrap();
        let pack2 = rt
            .block_on(build_agent_context_pack(&state, request(AgentMode::Ask)))
            .unwrap();

        let ids = |pack: &fndr_lib::agent::AgentContextPack| {
            pack.relevant_memories
                .iter()
                .map(|card| card.memory_id.clone())
                .collect::<Vec<_>>()
        };
        assert!(
            ids(&pack1).contains(&"roadmap".to_string()),
            "{:?}",
            ids(&pack1)
        );
        assert_eq!(ids(&pack1), ids(&pack2), "same memories in the same order");
        assert_eq!(
            serde_json::to_value(&pack1.allowed_tools).unwrap(),
            serde_json::to_value(&pack2.allowed_tools).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&pack1.privacy_scope).unwrap(),
            serde_json::to_value(&pack2.privacy_scope).unwrap()
        );
        // Each pack is its own record: the id is fresh every time.
        assert_ne!(pack1.task_id, pack2.task_id);
    }

    #[test]
    fn ask_mode_has_no_proposed_actions() {
        let rt = Runtime::new().unwrap();
        let (_dir, state) = setup_test_state(&rt);
        let response = rt
            .block_on(run_agent_request(&state, request(AgentMode::Ask)))
            .unwrap();
        assert!(response.proposed_actions.is_empty());
        // The pack the run used forbids every write, command, and message.
        let writes = [
            PermissionScope::WriteFile,
            PermissionScope::RunMutatingCommand,
            PermissionScope::SendExternalMessage,
        ];
        let denied = response
            .context_pack
            .allowed_tools
            .iter()
            .filter(|policy| writes.contains(&policy.scope))
            .collect::<Vec<_>>();
        assert_eq!(denied.len(), writes.len());
        assert!(denied.iter().all(|policy| !policy.allowed));
    }

    #[test]
    fn plan_mode_proposes_but_does_not_execute() {
        let rt = Runtime::new().unwrap();
        let (_dir, state) = setup_test_state(&rt);
        let response = rt
            .block_on(run_agent_request(&state, request(AgentMode::Plan)))
            .unwrap();
        assert!(!response.proposed_actions.is_empty());
        // Plan only proposes reading: anything that could run needs approval.
        for action in &response.proposed_actions {
            assert!(
                matches!(
                    action.scope,
                    PermissionScope::ReadMemory | PermissionScope::ReadProjectMemory
                ) || action.requires_approval,
                "{action:?}"
            );
        }
        let act = rt
            .block_on(run_agent_request(&state, request(AgentMode::Act)))
            .unwrap();
        assert!(act
            .proposed_actions
            .iter()
            .all(|action| action.requires_approval));
    }

    #[test]
    fn audit_record_is_created_for_every_run() {
        let rt = Runtime::new().unwrap();
        let (_dir, state) = setup_test_state(&rt);
        let mut run_ids = Vec::new();
        for mode in [AgentMode::Ask, AgentMode::Plan, AgentMode::Act] {
            run_ids.push(
                rt.block_on(run_agent_request(&state, request(mode)))
                    .unwrap()
                    .run_id,
            );
        }
        let mut audited = list_agent_audit_runs(&state.app_data_dir, 100, None, None)
            .unwrap()
            .into_iter()
            .map(|record| record.run_id)
            .collect::<Vec<_>>();
        audited.sort();
        run_ids.sort();
        assert_eq!(audited, run_ids);
    }

    #[test]
    fn policy_defaults_deny_dangerous_actions() {
        for (cmd, args) in [
            ("rm", vec!["-rf", "/tmp/x"]),
            ("sudo", vec!["ls"]),
            ("git", vec!["push", "origin", "main"]),
            ("git", vec!["commit", "-m", "x"]),
            ("git", vec!["reset", "--hard"]),
            ("git", vec!["branch", "-D", "main"]),
            ("git", vec!["branch", "new-branch"]),
            ("git", vec!["diff", "--output=/tmp/x.patch"]),
            ("git", vec!["log", "--output", "/tmp/x.log"]),
            ("git", vec!["branch", "-av", "-d", "x"]),
            ("git", vec!["branch", "-vD", "x"]),
            ("git", vec!["diff", "--no-index", "/etc/hosts", "/dev/null"]),
            ("git", vec!["diff", "--ext-diff"]),
            ("git", vec!["show", "--textconv", "HEAD"]),
            ("cargo", vec!["install", "ripgrep"]),
            ("npm", vec!["install"]),
            ("curl", vec!["https://example.com"]),
        ] {
            assert!(
                validate_command(cmd, &args).is_err(),
                "{cmd} {args:?} must be refused"
            );
        }
        for (cmd, args) in [
            ("git", vec!["status"]),
            ("git", vec!["branch"]),
            ("git", vec!["branch", "-a", "-v"]),
            ("git", vec!["branch", "-av"]),
            ("git", vec!["branch", "--list", "feat/*"]),
            ("git", vec!["branch", "--contains", "HEAD"]),
            (
                "git",
                vec!["branch", "--merged", "main", "--sort=-committerdate"],
            ),
            ("git", vec!["diff", "--output-indicator-new=+"]),
            ("ls", vec!["-la"]),
            ("cargo", vec!["test"]),
            ("npm", vec!["run", "typecheck"]),
        ] {
            assert!(
                validate_command(cmd, &args).is_ok(),
                "{cmd} {args:?} should be allowed"
            );
        }
    }
}

#[cfg(test)]
mod action_policy {
    use fndr_lib::agent::actions::{policy_for_action, AgentActionKind};
    use fndr_lib::agent::policy::{AgentMode, RiskLevel};

    #[test]
    fn ask_mode_blocks_all_actions() {
        let decision =
            policy_for_action(&AgentActionKind::OpenUrl, &RiskLevel::Low, &AgentMode::Ask);
        assert!(!decision.allowed, "Ask mode must block all actions");
        assert!(decision.blocked_because.is_some());
    }

    #[test]
    fn plan_mode_proposes_but_requires_approval() {
        let decision =
            policy_for_action(&AgentActionKind::OpenUrl, &RiskLevel::Low, &AgentMode::Plan);
        assert!(decision.allowed, "Plan mode should allow proposals");
        assert!(
            decision.requires_approval,
            "Plan mode requires approval to execute"
        );
    }

    #[test]
    fn high_risk_command_is_blocked_in_act_mode() {
        let decision = policy_for_action(
            &AgentActionKind::RunReadOnlyCommand,
            &RiskLevel::High,
            &AgentMode::Act,
        );
        assert!(!decision.allowed, "High-risk commands must be blocked");
    }

    #[test]
    fn open_url_is_allowed_but_needs_approval_even_when_low_risk() {
        // Opening a URL leaves FNDR, so Act mode asks first (1b68fd5).
        let decision =
            policy_for_action(&AgentActionKind::OpenUrl, &RiskLevel::Low, &AgentMode::Act);
        assert!(decision.allowed);
        assert!(decision.requires_approval);
    }

    #[test]
    fn unsupported_action_always_blocked() {
        let decision = policy_for_action(
            &AgentActionKind::Unsupported,
            &RiskLevel::Low,
            &AgentMode::Act,
        );
        assert!(!decision.allowed);
        assert!(decision.blocked_because.is_some());
    }
}

#[cfg(test)]
mod audit_persistence {
    use fndr_lib::agent::audit::{
        append_agent_audit_record, list_agent_audit_runs, AgentAuditRecord, AgentRunStatus,
    };
    use fndr_lib::agent::policy::AgentMode;

    fn make_record(run_id: &str, mode: AgentMode, status: AgentRunStatus) -> AgentAuditRecord {
        AgentAuditRecord {
            run_id: run_id.to_string(),
            user_goal: format!("goal for {run_id}"),
            mode,
            result_status: status,
            ..Default::default()
        }
    }

    #[test]
    fn audit_record_persistence() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let app_data_dir = dir.path();

        let record = make_record("run-persist-1", AgentMode::Ask, AgentRunStatus::Success);
        append_agent_audit_record(app_data_dir, &record).expect("append audit record");

        let rows = list_agent_audit_runs(app_data_dir, 10, None, None).expect("list audit runs");

        assert_eq!(rows.len(), 1, "Expected exactly one audit record");
        assert_eq!(rows[0].run_id, "run-persist-1", "run_id must round-trip");
    }

    #[test]
    fn audit_filtering_by_mode() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let app_data_dir = dir.path();

        append_agent_audit_record(
            app_data_dir,
            &make_record("run-ask-1", AgentMode::Ask, AgentRunStatus::Success),
        )
        .expect("append ask record 1");
        append_agent_audit_record(
            app_data_dir,
            &make_record("run-ask-2", AgentMode::Ask, AgentRunStatus::Partial),
        )
        .expect("append ask record 2");
        append_agent_audit_record(
            app_data_dir,
            &make_record("run-plan-1", AgentMode::Plan, AgentRunStatus::Success),
        )
        .expect("append plan record");

        let ask_rows = list_agent_audit_runs(app_data_dir, 100, Some(AgentMode::Ask), None)
            .expect("list ask audit runs");

        assert_eq!(
            ask_rows.len(),
            2,
            "Filtering by Ask mode should return exactly 2 records"
        );
        assert!(
            ask_rows.iter().all(|r| r.mode == AgentMode::Ask),
            "All returned records should have Ask mode"
        );
    }

    #[test]
    fn audit_filtering_by_status() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let app_data_dir = dir.path();

        append_agent_audit_record(
            app_data_dir,
            &make_record("run-ok-1", AgentMode::Ask, AgentRunStatus::Success),
        )
        .expect("append success record 1");
        append_agent_audit_record(
            app_data_dir,
            &make_record("run-fail-1", AgentMode::Plan, AgentRunStatus::Failed),
        )
        .expect("append failed record");
        append_agent_audit_record(
            app_data_dir,
            &make_record("run-ok-2", AgentMode::Ask, AgentRunStatus::Success),
        )
        .expect("append success record 2");

        let failed_rows =
            list_agent_audit_runs(app_data_dir, 100, None, Some(AgentRunStatus::Failed))
                .expect("list failed audit runs");

        assert_eq!(
            failed_rows.len(),
            1,
            "Filtering by Failed status should return exactly 1 record"
        );
        assert_eq!(
            failed_rows[0].run_id, "run-fail-1",
            "Failed record run_id must match"
        );
        assert_eq!(
            failed_rows[0].result_status,
            AgentRunStatus::Failed,
            "Status must be Failed"
        );
    }
}

#[cfg(test)]
mod command_validation {
    use fndr_lib::agent::validate_command;

    #[test]
    fn git_status_is_allowed() {
        assert!(validate_command("git", &["status"]).is_ok());
    }

    #[test]
    fn git_commit_is_blocked() {
        assert!(validate_command("git", &["commit", "-m", "hack"]).is_err());
    }

    #[test]
    fn rm_is_blocked() {
        assert!(validate_command("rm", &["-rf", "/"]).is_err());
    }

    #[test]
    fn sudo_is_blocked() {
        assert!(validate_command("sudo", &["ls"]).is_err());
    }

    #[test]
    fn cargo_check_is_allowed() {
        assert!(validate_command("cargo", &["check"]).is_ok());
    }

    #[test]
    fn cargo_install_is_blocked() {
        assert!(validate_command("cargo", &["install", "ripgrep"]).is_err());
    }

    #[test]
    fn npm_typecheck_is_allowed() {
        assert!(validate_command("npm", &["run", "typecheck"]).is_ok());
    }

    #[test]
    fn npm_install_is_blocked() {
        assert!(validate_command("npm", &["install"]).is_err());
    }

    #[test]
    fn unknown_command_is_blocked() {
        assert!(validate_command("curl", &["https://example.com"]).is_err());
    }
}
