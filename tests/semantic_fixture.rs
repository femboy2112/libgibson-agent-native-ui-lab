use gibson_ui_lab::semantic_fixture::{
    fixture, snapshot, validate_action, Action, SemanticEvent, TimedAction, AGENT_IDS, DURATION_MS,
    MAX_ACTIONS,
};

#[test]
fn fixture_is_finite_ordered_and_contains_every_semantic_family() {
    let trace = fixture();
    assert!(trace.windows(2).all(|pair| pair[0].at_ms <= pair[1].at_ms));
    assert_eq!(trace.last().unwrap().at_ms, DURATION_MS);
    let mut kinds: Vec<_> = trace.iter().map(|step| step.event.kind()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 15);
    assert!(trace.iter().any(|step| matches!(
        step.event,
        SemanticEvent::ToolFinished { success: false, .. }
    )));
    let finished = snapshot(DURATION_MS, &[]);
    assert!(finished.finished);
    assert_eq!(finished.agents.len(), AGENT_IDS.len());
    assert!(finished.agents.iter().all(|agent| agent.finished));
    assert_eq!(finished.artifacts.len(), 1);
    assert_eq!(finished.event_order.len(), trace.len());
}

#[test]
fn replay_is_independent_of_irregular_sampling_and_serializes_exactly() {
    let actions = vec![
        TimedAction {
            at_ms: 5_123,
            action: Action::ChooseScope { expedition: true },
        },
        TimedAction {
            at_ms: 9_901,
            action: Action::Select {
                target: "verify".into(),
            },
        },
        TimedAction {
            at_ms: 11_117,
            action: Action::ResolvePermission { approved: true },
        },
    ];
    let recovered: Vec<TimedAction> =
        serde_json::from_str(&serde_json::to_string(&actions).unwrap()).unwrap();
    for time in [
        0, 13, 201, 1_207, 5_890, 12_200, 16_321, 18_000, 8_222, 18_000,
    ] {
        assert_eq!(snapshot(time, &actions), snapshot(time, &recovered));
    }
    assert_eq!(
        snapshot(u64::MAX, &actions),
        snapshot(DURATION_MS, &actions)
    );
    assert_eq!(
        snapshot(DURATION_MS, &actions).event_order,
        snapshot(DURATION_MS, &[]).event_order
    );
}

#[test]
fn explicit_permission_denial_survives_scripted_default_and_withholds_artifact() {
    let actions = [TimedAction {
        at_ms: 11_000,
        action: Action::ResolvePermission { approved: false },
    }];
    assert!(snapshot(10_900, &actions).permission_pending);
    let denied = snapshot(DURATION_MS, &actions);
    assert_eq!(denied.permission, Some(false));
    assert!(!denied.permission_pending);
    assert!(denied.artifacts.is_empty());
    assert!(denied
        .action_receipts
        .iter()
        .any(|receipt| receipt.contains("SUPPRESSED")));
    assert_eq!(denied.event_order, snapshot(DURATION_MS, &[]).event_order);
    assert!(!snapshot(DURATION_MS, &[]).artifacts.is_empty());
}

#[test]
fn invalid_targets_and_out_of_window_permission_are_rejected() {
    assert!(validate_action(
        &snapshot(8_000, &[]),
        &Action::Select {
            target: "verify".into()
        }
    )
    .is_ok());
    assert!(validate_action(
        &snapshot(1_000, &[]),
        &Action::Select {
            target: "verify".into()
        }
    )
    .is_err());
    let actions = [
        TimedAction {
            at_ms: 1,
            action: Action::ResolvePermission { approved: false },
        },
        TimedAction {
            at_ms: 2_000,
            action: Action::Select {
                target: "made-up".into(),
            },
        },
        // Source events win exact timestamp ties; this is already resolved.
        TimedAction {
            at_ms: 13_200,
            action: Action::ResolvePermission { approved: false },
        },
    ];
    let state = snapshot(DURATION_MS, &actions);
    assert_eq!(state.permission, Some(true));
    assert_eq!(state.selected, None);
    assert_eq!(
        state
            .action_receipts
            .iter()
            .filter(|receipt| receipt.contains("REJECTED"))
            .count(),
        3
    );
}

#[test]
fn action_budget_and_input_order_are_enforced_without_mutating_source() {
    let action = TimedAction {
        at_ms: 2_000,
        action: Action::Select {
            target: "scout".into(),
        },
    };
    for actions in [
        vec![action.clone(); MAX_ACTIONS + 1],
        vec![
            action.clone(),
            TimedAction {
                at_ms: 1_999,
                ..action.clone()
            },
        ],
    ] {
        let state = snapshot(DURATION_MS, &actions);
        assert_eq!(state.selected, None);
        assert_eq!(state.action_receipts.len(), 1);
        assert!(state.action_receipts[0].starts_with("REJECTED INPUT:"));
        assert_eq!(state.event_order, snapshot(DURATION_MS, &[]).event_order);
    }
}

#[test]
fn failure_recovery_and_parallelism_are_real_snapshot_state() {
    let parallel = snapshot(3_200, &[]);
    assert!(parallel
        .agents
        .iter()
        .any(|agent| agent.id == "scout" && agent.progress == 45));
    assert!(parallel
        .agents
        .iter()
        .any(|agent| agent.id == "builder" && agent.progress == 35));
    assert!(snapshot(5_000, &[])
        .agents
        .iter()
        .any(|agent| agent.id == "builder" && agent.failed));
    assert!(snapshot(8_500, &[])
        .agents
        .iter()
        .any(|agent| agent.id == "builder" && !agent.failed && agent.progress == 100));
}
