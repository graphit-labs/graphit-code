use graphit_records::gates::{
    CheckFact, CheckKind, CheckStatus, GateError, GateGraph, TaskFact, TaskStatus,
};

const PASSED: [CheckFact<'static>; 2] = [
    CheckFact {
        kind: CheckKind::Acceptance,
        status: CheckStatus::Passed,
        evidence: "observed acceptance",
    },
    CheckFact {
        kind: CheckKind::Test,
        status: CheckStatus::Passed,
        evidence: "observed fixture",
    },
];

fn task<'a>(id: &'a str, status: TaskStatus, dependencies: &'a [&'a str]) -> TaskFact<'a> {
    TaskFact {
        id,
        session_id: "session",
        parent_id: None,
        status,
        flagged: false,
        dependencies,
        checks: &PASSED,
    }
}

#[test]
fn completion_is_conditions_not_authority_in_all_four_states() {
    for status in [
        TaskStatus::Open,
        TaskStatus::InProgress,
        TaskStatus::Completed,
        TaskStatus::Cancelled,
    ] {
        let facts = [task("a", status, &[])];
        let graph = GateGraph::new(&facts).unwrap();
        assert_eq!(graph.completion("a"), Ok(status != TaskStatus::Cancelled));
        assert_eq!(graph.ready("a"), Ok(status == TaskStatus::Open));
        assert_eq!(
            graph.session_closable("session"),
            Ok(matches!(
                status,
                TaskStatus::Completed | TaskStatus::Cancelled
            ))
        );
    }
}

#[test]
fn invalid_historical_dependency_blocks_transitive_consumers() {
    let mut facts = [
        task("a", TaskStatus::Completed, &[]),
        task("b", TaskStatus::Completed, &["a"]),
        task("c", TaskStatus::Open, &["b"]),
    ];
    facts[0].flagged = true;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("c"), Ok(false));
    assert_eq!(graph.completion("b"), Ok(false));
    assert_eq!(graph.session_closable("session"), Ok(false));
    drop(graph);
    facts[0].flagged = false;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("c"), Ok(true));
}

#[test]
fn ready_parent_does_not_require_ready_or_completed_children() {
    let mut facts = [
        task("p", TaskStatus::Open, &[]),
        task("c", TaskStatus::Open, &[]),
    ];
    facts[1].parent_id = Some("p");
    let mut graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("p"), Ok(true));
    assert_eq!(graph.completion("p"), Ok(false));
    for state in [
        TaskStatus::Cancelled,
        TaskStatus::InProgress,
        TaskStatus::Completed,
    ] {
        drop(graph);
        facts[1].status = state;
        graph = GateGraph::new(&facts).unwrap();
        assert_eq!(
            graph.completion("p"),
            Ok(matches!(
                state,
                TaskStatus::Completed | TaskStatus::Cancelled
            ))
        );
    }
}

#[test]
fn active_checks_need_both_kinds_and_nonblank_pass_evidence() {
    for status in [
        CheckStatus::Pending,
        CheckStatus::Failed,
        CheckStatus::Passed,
    ] {
        for evidence in ["", " \n\t", "observed"] {
            let checks = [
                CheckFact {
                    kind: CheckKind::Acceptance,
                    status,
                    evidence,
                },
                CheckFact {
                    kind: CheckKind::Test,
                    status: CheckStatus::Passed,
                    evidence: "test",
                },
            ];
            let mut fact = task("a", TaskStatus::Completed, &[]);
            fact.checks = &checks;
            let facts = [fact];
            let graph = GateGraph::new(&facts).unwrap();
            assert_eq!(
                graph.completion("a"),
                Ok(status == CheckStatus::Passed && evidence == "observed")
            );
        }
    }
    for checks in [&PASSED[..1], &PASSED[1..], &[]] {
        let mut fact = task("a", TaskStatus::Completed, &[]);
        fact.checks = checks;
        let facts = [fact];
        assert_eq!(GateGraph::new(&facts).unwrap().completion("a"), Ok(false));
    }
}

#[test]
fn superseded_checks_neither_block_nor_supply_a_required_kind() {
    let checks = [
        CheckFact {
            kind: CheckKind::Acceptance,
            status: CheckStatus::Passed,
            evidence: "proof",
        },
        CheckFact {
            kind: CheckKind::Test,
            status: CheckStatus::Passed,
            evidence: "test",
        },
        CheckFact {
            kind: CheckKind::Test,
            status: CheckStatus::Superseded,
            evidence: "",
        },
    ];
    let mut fact = task("a", TaskStatus::Completed, &[]);
    fact.checks = &checks;
    let facts = [fact];
    assert_eq!(GateGraph::new(&facts).unwrap().completion("a"), Ok(true));
    let mut fact = task("a", TaskStatus::Completed, &[]);
    let no_active_test = [
        CheckFact {
            kind: CheckKind::Acceptance,
            status: CheckStatus::Passed,
            evidence: "proof",
        },
        CheckFact {
            kind: CheckKind::Test,
            status: CheckStatus::Superseded,
            evidence: "",
        },
    ];
    fact.checks = &no_active_test;
    let facts = [fact];
    assert_eq!(GateGraph::new(&facts).unwrap().completion("a"), Ok(false));
}

#[test]
fn cancelled_is_terminal_for_session_but_not_a_prerequisite() {
    let facts = [
        task("a", TaskStatus::Cancelled, &[]),
        task("b", TaskStatus::Cancelled, &["a"]),
    ];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.session_closable("session"), Ok(true));
    assert_eq!(graph.completion("b"), Ok(false));
    let facts = [
        task("a", TaskStatus::Cancelled, &[]),
        task("b", TaskStatus::Open, &["a"]),
    ];
    assert_eq!(GateGraph::new(&facts).unwrap().ready("b"), Ok(false));
}

#[test]
fn empty_session_id_is_invalid_but_nonempty_session_without_tasks_is_conditional_true() {
    let graph = GateGraph::new(&[]).unwrap();
    assert_eq!(graph.session_closable(""), Err(GateError::InvalidFacts));
    assert_eq!(graph.session_closable("unobserved-session"), Ok(true));
    let facts = [task("a", TaskStatus::Open, &[])];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.session_closable("other-session"), Ok(true));
    assert_eq!(graph.session_closable("session"), Ok(false));
}

#[test]
fn diamond_and_duplicate_combined_edge_preserve_closure() {
    let mut facts = [
        task("a", TaskStatus::Completed, &[]),
        task("b", TaskStatus::Completed, &["a"]),
        task("c", TaskStatus::Completed, &["a"]),
        task("d", TaskStatus::Completed, &["b", "c"]),
    ];
    assert_eq!(GateGraph::new(&facts).unwrap().completion("d"), Ok(true));
    facts[1].parent_id = Some("d");
    assert_eq!(GateGraph::new(&facts).unwrap().completion("d"), Ok(true));
}

#[test]
fn cycles_include_dependency_and_parent_edges() {
    let mut facts = [
        task("a", TaskStatus::Completed, &[]),
        task("b", TaskStatus::Completed, &["a"]),
    ];
    facts[1].parent_id = Some("a");
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
    facts[1].parent_id = None;
    facts[0].dependencies = &["b"];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
    facts[0].dependencies = &[];
    facts[0].parent_id = Some("b");
    facts[1].parent_id = Some("a");
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
}

#[test]
fn missing_duplicates_self_edges_and_cross_session_parent_fail_closed() {
    for dependencies in [&["missing"][..], &["a"][..], &["b", "b"][..]] {
        let facts = [
            task("a", TaskStatus::Open, dependencies),
            task("b", TaskStatus::Completed, &[]),
        ];
        assert!(matches!(
            GateGraph::new(&facts),
            Err(GateError::InvalidFacts)
        ));
    }
    let facts = [
        task("a", TaskStatus::Open, &[]),
        task("a", TaskStatus::Completed, &[]),
    ];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
    for parent in ["missing", "a", "b"] {
        let mut facts = [
            task("a", TaskStatus::Open, &[]),
            task("b", TaskStatus::Completed, &[]),
        ];
        facts[0].parent_id = Some(parent);
        facts[1].session_id = "other";
        assert!(matches!(
            GateGraph::new(&facts),
            Err(GateError::InvalidFacts)
        ));
    }
}

#[test]
fn fact_and_query_identifiers_share_validation_without_normalization() {
    for invalid in ["", " ", " a", "a ", "\na", "a\t"] {
        for field in 0..4 {
            let mut fact = task("a", TaskStatus::Open, &[]);
            let dependency = [invalid];
            match field {
                0 => fact.id = invalid,
                1 => fact.session_id = invalid,
                2 => fact.parent_id = Some(invalid),
                _ => fact.dependencies = &dependency,
            }
            assert!(matches!(
                GateGraph::new(&[fact]),
                Err(GateError::InvalidFacts)
            ));
        }
        let facts = [task("a", TaskStatus::Open, &[])];
        let graph = GateGraph::new(&facts).unwrap();
        assert_eq!(graph.ready(invalid), Err(GateError::InvalidFacts));
        assert_eq!(
            graph.session_closable(invalid),
            Err(GateError::InvalidFacts)
        );
    }
}

#[test]
fn long_chain_is_iterative() {
    let ids: Vec<String> = (0..1_000).map(|i| format!("t{i:04}")).collect();
    let dependencies: Vec<Vec<&str>> = (0..ids.len())
        .map(|i| {
            if i == 0 {
                vec![]
            } else {
                vec![ids[i - 1].as_str()]
            }
        })
        .collect();
    let facts: Vec<TaskFact<'_>> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| task(id, TaskStatus::Completed, &dependencies[i]))
        .collect();
    assert_eq!(
        GateGraph::new(&facts).unwrap().completion("t0999"),
        Ok(true)
    );
}

#[test]
fn nested_child_flag_or_invalid_check_blocks_completed_ancestors_and_consumers() {
    let mut facts = [
        task("leaf", TaskStatus::Completed, &[]),
        task("middle", TaskStatus::Completed, &[]),
        task("root", TaskStatus::Completed, &[]),
        task("consumer", TaskStatus::Open, &["root"]),
    ];
    facts[0].parent_id = Some("middle");
    facts[1].parent_id = Some("root");
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("consumer"), Ok(true));
    drop(graph);
    facts[0].flagged = true;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("middle"), Ok(false));
    assert_eq!(graph.completion("root"), Ok(false));
    assert_eq!(graph.ready("consumer"), Ok(false));
    drop(graph);
    facts[0].flagged = false;
    facts[0].checks = &[];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("root"), Ok(false));
    assert_eq!(graph.ready("consumer"), Ok(false));
}

#[test]
fn open_flagged_task_with_pending_checks_is_ready() {
    let pending = [CheckFact {
        kind: CheckKind::Acceptance,
        status: CheckStatus::Pending,
        evidence: "",
    }];
    let mut fact = task("open", TaskStatus::Open, &[]);
    fact.flagged = true;
    fact.checks = &pending;
    let facts = [fact];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("open"), Ok(true));
    assert_eq!(graph.completion("open"), Ok(false));
    assert_eq!(graph.ready("missing"), Err(GateError::MissingTask));
    assert_eq!(graph.completion("missing"), Err(GateError::MissingTask));
    assert_eq!(graph.session_closable("session"), Ok(false));
    assert!(facts[0].flagged);
    assert_eq!(facts[0].checks[0].status, CheckStatus::Pending);
}
#[test]
fn dependencies_between_sessions_are_valid() {
    let mut facts = [
        task("producer", TaskStatus::Completed, &[]),
        task("consumer", TaskStatus::Open, &["producer"]),
    ];
    facts[0].session_id = "other";
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.ready("consumer"), Ok(true));
    assert_eq!(graph.session_closable("other"), Ok(true));
    assert_eq!(graph.session_closable("session"), Ok(false));
}

#[test]
fn withdrawn_child_does_not_supply_a_cancelled_dependency() {
    let mut facts = [
        task("parent", TaskStatus::Open, &[]),
        task("child", TaskStatus::Cancelled, &["pending"]),
        task("pending", TaskStatus::Open, &[]),
    ];
    facts[1].parent_id = Some("parent");
    facts[1].flagged = true;
    facts[1].checks = &[];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(true));
    assert_eq!(graph.completion("child"), Ok(false));
    drop(graph);
    facts[0].dependencies = &["child"];
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(false));
    assert_eq!(graph.ready("parent"), Ok(false));
    assert_eq!(facts[1].status, TaskStatus::Cancelled);
}

#[test]
fn withdrawn_nested_child_keeps_active_or_invalid_descendants_visible() {
    let mut facts = [
        task("root", TaskStatus::InProgress, &[]),
        task("withdrawn", TaskStatus::Cancelled, &[]),
        task("leaf", TaskStatus::Open, &[]),
    ];
    facts[1].parent_id = Some("root");
    facts[1].checks = &[];
    facts[2].parent_id = Some("withdrawn");
    for status in [TaskStatus::Open, TaskStatus::InProgress] {
        facts[2].status = status;
        assert_eq!(
            GateGraph::new(&facts).unwrap().completion("root"),
            Ok(false)
        );
    }
    facts[2].status = TaskStatus::Completed;
    facts[2].flagged = true;
    assert_eq!(
        GateGraph::new(&facts).unwrap().completion("root"),
        Ok(false)
    );
    facts[2].flagged = false;
    assert_eq!(GateGraph::new(&facts).unwrap().completion("root"), Ok(true));
    facts[2].status = TaskStatus::Cancelled;
    facts[2].flagged = true;
    facts[2].checks = &[];
    assert_eq!(GateGraph::new(&facts).unwrap().completion("root"), Ok(true));
    facts[0].flagged = true;
    assert_eq!(
        GateGraph::new(&facts).unwrap().completion("root"),
        Ok(false)
    );
}

#[test]
fn cancelled_child_dependency_on_parent_does_not_block_parent_or_skip_integrity() {
    let mut facts = [
        task("parent", TaskStatus::Open, &[]),
        task("cancelled", TaskStatus::Cancelled, &["parent"]),
    ];
    facts[1].parent_id = Some("parent");
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(true));
    assert_eq!(graph.completion("cancelled"), Ok(false));
    assert_eq!(facts[1].dependencies, &["parent"]);
    drop(graph);
    facts[0].status = TaskStatus::Cancelled;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(false));
    assert_eq!(graph.session_closable("session"), Ok(true));
    drop(graph);
    facts[1].dependencies = &["missing"];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
    facts[1].dependencies = &["cancelled"];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
    facts[1].dependencies = &["parent", "parent"];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
}

#[test]
fn cancelled_prerequisites_and_pending_descendants_remain_unsatisfied_without_ordering_cycles() {
    let mut facts = [
        task("parent", TaskStatus::Open, &[]),
        task("cancelled", TaskStatus::Cancelled, &["parent"]),
        task("pending", TaskStatus::Open, &["cancelled"]),
        task("consumer", TaskStatus::Open, &["cancelled"]),
    ];
    facts[1].parent_id = Some("parent");
    facts[2].parent_id = Some("cancelled");
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(false));
    assert_eq!(graph.ready("pending"), Ok(false));
    assert_eq!(graph.completion("pending"), Ok(false));
    assert_eq!(graph.ready("consumer"), Ok(false));
    assert_eq!(graph.completion("consumer"), Ok(false));
    drop(graph);
    facts[2].status = TaskStatus::Completed;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(false));
    assert_eq!(graph.completion("pending"), Ok(false));
    drop(graph);
    facts[2].status = TaskStatus::Cancelled;
    let graph = GateGraph::new(&facts).unwrap();
    assert_eq!(graph.completion("parent"), Ok(true));
    assert_eq!(graph.ready("consumer"), Ok(false));
    assert_eq!(facts[1].dependencies, &["parent"]);
    assert_eq!(facts[2].dependencies, &["cancelled"]);
}

#[test]
fn active_dependency_cycle_still_fails() {
    let facts = [
        task("a", TaskStatus::Open, &["b"]),
        task("b", TaskStatus::InProgress, &["a"]),
    ];
    assert!(matches!(
        GateGraph::new(&facts),
        Err(GateError::InvalidFacts)
    ));
}
