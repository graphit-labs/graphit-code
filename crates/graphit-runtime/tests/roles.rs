use graphit_core::{brand::Brand, file_io::SnapshotContext, file_lock::Cancellation};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use graphit_runtime::{ManagedRole, RoleError, compose_role};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const ROLES: [ManagedRole; 3] = [
    ManagedRole::Scout,
    ManagedRole::Tracker,
    ManagedRole::Scribe,
];
fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

#[test]
fn default_and_alternate_brands_compose_only_three_deterministic_roles() {
    for brand in [Brand::default(), Brand::new("demo-tools").unwrap()] {
        let req = request();
        for role in ROLES {
            let first = compose_role(role, &brand, &req).unwrap();
            let second = compose_role(role, &brand, &req).unwrap();
            assert_eq!(first.role(), role);
            assert_eq!(first.name(), format!("{}-{}", brand.key(), role.key()));
            assert_eq!(first.name(), second.name());
            assert_eq!(first.body(), second.body());
            assert_eq!(first.summary(), role.summary());
            assert_eq!(first.modules(), role.modules());
            assert_eq!(first.read_only(), role != ManagedRole::Scribe);
            assert_eq!(first.allows_shell(), role == ManagedRole::Tracker);
            let guidance = match role {
                ManagedRole::Scout => {
                    "Follow mandatory-memory loading instructions when Memory is enabled"
                }
                ManagedRole::Tracker => "Do not write records or code, and never accept delivery",
                ManagedRole::Scribe => {
                    "preserve the supplied structure and complete typed references"
                }
            };
            assert!(first.body().contains(guidance));
            assert!(
                first
                    .body()
                    .contains(&brand.mcp_tool_name(&["module", "skill"]).unwrap())
            );
            for module in role.modules() {
                assert!(first.body().contains(&brand.skill_name(module).unwrap()));
            }
            for forbidden in [
                "Standing context",
                "ses-c98a",
                "tsk-9bd4",
                "claim your own task",
            ] {
                assert!(!first.body().contains(forbidden));
            }
        }
    }
    assert_eq!(
        ManagedRole::Scout.modules(),
        ["memory", "task", "ast", "knowledge", "hub"]
    );
    assert_eq!(ManagedRole::Tracker.modules(), ["ast", "knowledge", "task"]);
    assert_eq!(ManagedRole::Scribe.modules(), ["task", "memory"]);
}

#[test]
fn all_bodies_reserve_coordination_to_the_coordinator() {
    let req = request();
    for role in ROLES {
        let value = compose_role(role, &Brand::default(), &req).unwrap();
        for required in [
            "The coordinator owns acceptance, checkpoints, Task/session claims, revisions, completion and lifecycle.",
            "do not create, claim, revise, checkpoint, complete, cancel or release coordination state or another agent's Task",
            "return a draft to the coordinator; do not acquire that claim",
            "end your turn without a waiting loop",
            "A performer already in that role works directly, not recursively.",
            "the coordinator performs it directly under its own ordinary Task ownership",
            "that exception gives delegates no claim or lifecycle powers",
            "never substitute the Graphit CLI",
            "ai_optimized: true",
        ] {
            assert!(
                value.body().contains(required),
                "missing generic protocol for {role:?}"
            );
        }
        assert!(!value.body().contains("claim their Tasks"));
        assert!(!value.body().contains("claim your task"));
    }
}

#[test]
fn combined_names_accept_64_and_reject_65_before_output_allocation() {
    let req = request();
    for length in [56, 57] {
        let brand = Brand::new(&"a".repeat(length)).unwrap();
        let result = compose_role(ManagedRole::Tracker, &brand, &req);
        if length == 56 {
            assert_eq!(result.unwrap().name().len(), 64);
        } else {
            assert!(matches!(result, Err(RoleError::InvalidName)));
        }
    }
}

#[test]
fn cancellation_before_and_after_composition_uses_the_original_request() {
    let cancellation = Cancellation::default();
    let req = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    );
    let value = compose_role(ManagedRole::Scout, &Brand::default(), &req).unwrap();
    cancellation.cancel();
    for role in ROLES {
        assert!(matches!(
            compose_role(role, &Brand::default(), &req),
            Err(RoleError::Cancelled)
        ));
    }
    assert_eq!(value.role(), ManagedRole::Scout);
}

#[test]
fn expiration_after_composition_is_not_reset() {
    let req = SnapshotContext::new(
        Instant::now() + Duration::from_millis(50),
        Cancellation::default(),
    );
    let value = compose_role(ManagedRole::Scout, &Brand::default(), &req).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    for role in ROLES {
        assert!(matches!(
            compose_role(role, &Brand::default(), &req),
            Err(RoleError::DeadlineExceeded)
        ));
    }
    assert_eq!(value.role(), ManagedRole::Scout);
}

#[test]
fn actual_hub_descriptors_share_one_captured_request_without_authority_fixtures() {
    let cancellation = Cancellation::default();
    let mut hub = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 8,
            max_nodes: 1000,
            max_scalar_bytes: 8192,
            max_collection_entries: 100,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let snapshot = Arc::clone(hub.snapshot());
    let mut expected_input = 0;
    for role in ROLES {
        let value = compose_role(role, &Brand::default(), &snapshot).unwrap();
        let descriptor = hub
            .managed_agent(
                value.name(),
                value.summary(),
                value.body(),
                value.read_only(),
                value.allows_shell(),
            )
            .unwrap();
        assert_eq!(descriptor.name(), value.name());
        assert_eq!(descriptor.description(), value.summary());
        assert_eq!(descriptor.body(), value.body());
        assert_eq!(descriptor.read_only(), value.read_only());
        assert_eq!(descriptor.allows_shell(), value.allows_shell());
        expected_input += value.name().len() + value.summary().len() + value.body().len();
        assert_eq!(hub.input_bytes(), expected_input);
        assert!(Arc::ptr_eq(&snapshot, hub.snapshot()));
    }
    assert_eq!(hub.output_bytes(), 0);
    cancellation.cancel();
    assert!(matches!(
        compose_role(ManagedRole::Scout, &Brand::default(), &snapshot),
        Err(RoleError::Cancelled)
    ));
}
