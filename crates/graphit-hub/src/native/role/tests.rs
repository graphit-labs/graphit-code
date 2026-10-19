use super::*;
use crate::native::ContentLimits;
use graphit_core::file_lock::Cancellation;
use std::time::{Duration, Instant};

fn limits() -> ProjectionLimits {
    ProjectionLimits::new(64, 4096, 16, 64, 16).unwrap()
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 8,
            max_nodes: 64,
            max_scalar_bytes: 256,
            max_collection_entries: 8,
        },
        cancel,
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap()
}
fn reference(role: Role) -> RoleCredentialReference {
    RoleCredentialReference::new(
        role,
        RoleSource::HubArtifact(ArtifactId::new("agents/trusted", limits()).unwrap()),
        [0xab; 32],
    )
}

#[test]
fn canonical_roles_and_exact_wire_boundaries() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    for role in Role::ALL {
        let value = reference(role);
        let bytes = value.encode(1024, &ctx).unwrap();
        assert_eq!(value.encode(bytes.len(), &ctx).unwrap(), bytes);
        assert_eq!(
            value.encode(bytes.len() - 1, &ctx),
            Err(RoleReferenceError::Limit)
        );
        assert_eq!(
            RoleCredentialReference::decode(&bytes, bytes.len(), limits(), &ctx).unwrap(),
            value
        );
        assert_eq!(
            RoleCredentialReference::decode(&bytes, bytes.len() - 1, limits(), &ctx),
            Err(RoleReferenceError::Limit)
        );
        let json = String::from_utf8(bytes).unwrap();
        assert!(json.contains(role.as_str()));
        assert!(json.contains(&"ab".repeat(32)));
        assert!(!json.contains("credential"));
    }
}
#[test]
fn closed_wire_rejects_unknown_duplicate_missing_and_wrong_types() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let good = String::from_utf8(reference(Role::Scout).encode(1024, &ctx).unwrap()).unwrap();
    let bad = [
        good.replace("\"format_version\":1", "\"format_version\":2"),
        good.replace("\"role\":\"scout\"", "\"role\":\"admin\""),
        good.replace(
            "\"role\":\"scout\"",
            "\"role\":\"scout\",\"role\":\"scout\"",
        ),
        good.replace(
            "\"artifact_id\":\"agents/trusted\"",
            "\"artifact_id\":\"../escape\"",
        ),
        good.replace("abab", "ABAB"),
        good.replace("\"role\":\"scout\",", ""),
        good.replace("}", ",\"secret\":\"sentinel\"}"),
        format!("{good}{{}}"),
        "null".into(),
        "[]".into(),
        good.replace("\"artifact_id\":\"agents/trusted\"", "\"artifact_id\":null"),
        good.replace("\"format_version\":1", "\"format_version\":1.0"),
    ];
    for bytes in bad {
        let err =
            RoleCredentialReference::decode(bytes.as_bytes(), 4096, limits(), &ctx).unwrap_err();
        assert_eq!(err, RoleReferenceError::Invalid);
        assert!(!format!("{err:?} {err}").contains("sentinel"));
    }
}
#[test]
fn decoded_identifiers_are_validated_and_escaped_fields_remain_bounded() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let value = reference(Role::Scout);
    let bytes = String::from_utf8(value.encode(1024, &ctx).unwrap())
        .unwrap()
        .replace("agents/trusted", "agents\\u002ftrusted");
    assert_eq!(
        RoleCredentialReference::decode(bytes.as_bytes(), bytes.len(), limits(), &ctx).unwrap(),
        value
    );
    let short = ProjectionLimits::new(4, 4096, 16, 64, 16).unwrap();
    assert_eq!(
        RoleCredentialReference::decode(bytes.as_bytes(), bytes.len(), short, &ctx),
        Err(RoleReferenceError::Invalid)
    );
    assert_eq!(
        RoleCredentialReference::decode(b"{}", 0, limits(), &ctx),
        Err(RoleReferenceError::Limit)
    );
    assert_eq!(value.encode(0, &ctx), Err(RoleReferenceError::Limit));
}
#[test]
fn original_request_cancellation_and_deadline_are_not_renewed() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let value = reference(Role::Dream);
    let bytes = value.encode(1024, &ctx).unwrap();
    cancellation.cancel();
    assert_eq!(value.encode(1024, &ctx), Err(RoleReferenceError::Request));
    assert_eq!(
        RoleCredentialReference::decode(&bytes, 1024, limits(), &ctx),
        Err(RoleReferenceError::Request)
    );
    let fresh = Cancellation::default();
    let expired = RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 8,
            max_nodes: 64,
            max_scalar_bytes: 256,
            max_collection_entries: 8,
        },
        &fresh,
        Instant::now() + Duration::from_millis(5),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(
        value.encode(1024, &expired),
        Err(RoleReferenceError::Request)
    );
}

#[test]
fn tool_owned_source_roundtrip_is_distinct_and_closed() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let source = RoleSource::ToolOwned(Brand::new("custom-tool").unwrap());
    let value = RoleCredentialReference::new(Role::Tracker, source, [0xab; 32]);
    let bytes = value.encode(1024, &ctx).unwrap();
    assert_eq!(
        RoleCredentialReference::decode(&bytes, bytes.len(), limits(), &ctx).unwrap(),
        value
    );
    assert_ne!(value, reference(Role::Tracker));
    let good = String::from_utf8(bytes).unwrap();
    for bad in [
        good.replace("tool_owned", "builtin"),
        good.replace("custom-tool", "Custom-Tool"),
        good.replace("custom-tool", "../escape"),
        good.replace(
            "\"brand\":\"custom-tool\"",
            "\"brand\":\"custom-tool\",\"brand\":\"custom-tool\"",
        ),
        good.replace(
            "\"brand\":\"custom-tool\"",
            "\"brand\":\"custom-tool\",\"artifact_id\":\"agents/trusted\"",
        ),
        good.replace(
            "\"source\":{\"kind\":\"tool_owned\",\"brand\":\"custom-tool\"}",
            "\"artifact_id\":\"agents/trusted\"",
        ),
    ] {
        assert_eq!(
            RoleCredentialReference::decode(bad.as_bytes(), 4096, limits(), &ctx),
            Err(RoleReferenceError::Invalid)
        );
    }
    let hub = String::from_utf8(reference(Role::Tracker).encode(1024, &ctx).unwrap()).unwrap();
    let swapped = hub.replace("hub_artifact", "tool_owned");
    assert_eq!(
        RoleCredentialReference::decode(swapped.as_bytes(), 4096, limits(), &ctx),
        Err(RoleReferenceError::Invalid)
    );
    cancellation.cancel();
    assert_eq!(value.encode(1024, &ctx), Err(RoleReferenceError::Request));
}
