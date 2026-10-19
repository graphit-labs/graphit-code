//! Metadata-only selection through the real S4 first consumer and Data SDK.
#![cfg(target_os = "linux")]
use graphit_knowledge as knowledge;
#[allow(dead_code)] // Shared real fixture also serves lifecycle/denial targets.
#[path = "support/desired_reconciliation_fixture.rs"]
mod fixture;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::file_lock::Cancellation;
use knowledge::{
    desired_corpus::capture_knowledge_source_metadata,
    publication_metadata::{PublicationMetadataPolicy, compile_captured_file_bytes},
};
use std::{error::Error, path::Path};

#[tokio::test]
async fn metadata_factory_has_no_retained_bodies_and_external_compilation_uses_actual_file_buffer()
-> Result<(), Box<dyn Error>> {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx)?;
    let auth = ProjectRecordAuthority::new(&issuer);
    let read =
        auth.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)?;
    let body = "\u{feff}---\r\ntitle: Ω\r\ndescription: ''\r\ntype: null\r\n---\r\nWhole 世界\r\n";
    put(&f, "docs/a.md", body);
    put(&f, "docs/empty.md", b"");
    let config = config(&[("knowledge.include_readme", serde_json::json!(false))]);
    let captured =
        capture_knowledge_source_metadata(ctx.snapshot(), &read, &config, None, capture_limits())?;
    let reader = captured.reader(ctx.snapshot(), &read)?;
    assert_eq!(reader.retained_bytes(), 0);
    assert_eq!(
        reader
            .nodes()
            .filter(|n| n.kind() == graphit_fs::NodeKind::File)
            .count(),
        2
    );
    for relative in ["docs/a.md", "docs/empty.md"] {
        assert!(reader.retained(Path::new(relative)).is_none());
        let file = reader.open_file(Path::new(relative))?;
        let buffer = file.read_range(0, usize::try_from(file.len())?)?;
        let caps = preparation();
        let compiled = compile_captured_file_bytes(
            reader,
            Path::new(relative),
            buffer.as_ref(),
            ctx.snapshot(),
            PublicationMetadataPolicy {
                fallback_title: "fallback".into(),
                kind_on_absent: "guide".into(),
            },
            caps.parser,
            caps.metadata,
        )?;
        assert_eq!(compiled.whole_source().as_bytes(), buffer.as_ref());
        assert_eq!(compiled.whole_source().as_ptr(), buffer.as_ref().as_ptr());
        if relative == "docs/a.md" {
            assert_eq!(compiled.description(), Some(""));
            assert_eq!(compiled.title(), "Ω");
        }
    }
    assert!(!f.project.runtime_root().exists());
    Ok(())
}
