use graphit_core::{
    artifact_store::PayloadMode, brand::Brand, file_io::SnapshotContext, file_lock::Cancellation,
};
use graphit_data::{
    knowledge_document_projection::CompilationDigest32,
    knowledge_index::{KnowledgeDocument, KnowledgeRecipeParameters},
};
use graphit_knowledge::{
    document::DocumentParseLimits,
    markdown_export::{
        MarkdownExportError as E, MarkdownExportLimits, MarkdownExportModes, MarkdownExportPlan,
        MarkdownExportStage,
    },
    native_document_mapping::NativeAuthoredMappingError,
    okf_export::KnowledgeTextExportFormat as Format,
    references::{ReferenceError, ReferenceLimits},
};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    path::{Component, Path},
    time::{Duration, Instant, UNIX_EPOCH},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn formats() -> [Format; 2] {
    let brand = Brand::new("corpus").unwrap();
    [
        Format::Okf {
            brand: brand.clone(),
        },
        Format::Obsidian { brand },
    ]
}
fn document(slug: &str) -> KnowledgeDocument {
    KnowledgeDocument {
        slug: slug.into(),
        title: format!("Title [<html>] 世界: {slug}"),
        body: "\u{feff}---\r\ntitle: Original\r\n---\r\n  # Literal\r\n\r\n[[log]] [authored](original.md)  \r\n".into(),
        kind: "document".into(),
        summary: "editorial preview".into(),
        description: Some("integral description\nsecond paragraph".into()),
        summary_truncated: true,
        source_path: "docs/source.md".into(),
        physical_digest: None,
        compilation_digest: CompilationDigest32::from_bytes([42; 32]),
        recipe: "authored-document-v1".into(),
        recipe_parameters: KnowledgeRecipeParameters {
            fallback_title: "fallback".into(),
            kind_on_absent: "document".into(),
            max_summary_bytes: n(32),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(17)),
        stale_since: Some(UNIX_EPOCH + Duration::from_nanos(9)),
        stale_after: Some(Duration::from_secs(60)),
        resolved_wiki_targets: Vec::new(),
    }
}
fn limits() -> MarkdownExportLimits {
    MarkdownExportLimits {
        max_file_bytes: n(1_000_000),
        document: DocumentParseLimits {
            max_document_bytes: n(65536),
            max_input_bytes: n(131072),
            max_frontmatter_bytes: n(65536),
            max_yaml_events: n(1000),
            max_markdown_events: n(1000),
            max_yaml_depth: n(32),
            max_scalar_bytes: n(65536),
            max_links: n(100),
            max_projection_bytes: n(131072),
            max_summary_bytes: n(32),
        },
        references: ReferenceLimits {
            max_identifier_bytes: n(65536),
        },
    }
}
fn modes() -> MarkdownExportModes {
    let sample = tempfile::tempdir().unwrap();
    let file = sample.path().join("mode");
    std::fs::write(&file, b"observed permissions").unwrap();
    MarkdownExportModes {
        directory: PayloadMode::from_permissions(&sample.path().metadata().unwrap().permissions())
            .unwrap(),
        file: PayloadMode::from_permissions(&file.metadata().unwrap().permissions()).unwrap(),
    }
}
fn finish(
    stage: MarkdownExportStage<'_>,
    original: &SnapshotContext,
) -> Result<MarkdownExportPlan, E> {
    stage.finish(original)
}
fn export(
    original: &SnapshotContext,
    parent: &Path,
    modes: MarkdownExportModes,
    format: Format,
    documents: &[KnowledgeDocument],
) -> MarkdownExportPlan {
    let mut stage = MarkdownExportStage::new_in(original, parent, modes, limits(), format).unwrap();
    for document in documents {
        stage.push(original, document.clone()).unwrap();
    }
    finish(stage, original).unwrap()
}
fn page(slug: &str) -> String {
    format!("p-{}.md", hex::encode(Sha256::digest(slug.as_bytes())))
}
fn header(text: &str) -> (Value, &str) {
    let (yaml, body) = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n\n")
        .unwrap();
    (serde_saphyr::from_str(yaml).unwrap(), body)
}
fn assert_links_exist(text: &str, base: &Path) -> usize {
    let mut count = 0;
    for event in Parser::new(text) {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            let path = Path::new(dest_url.as_ref());
            assert!(!path.is_absolute());
            assert!(path.components().all(|c| matches!(c, Component::Normal(_))));
            assert!(
                base.join(path).is_file(),
                "unresolved generated link {dest_url}"
            );
            count += 1;
        }
    }
    count
}
fn assert_clean(parent: &Path) {
    assert_eq!(
        std::fs::read_dir(parent).unwrap().count(),
        0,
        "private spool must be removed"
    );
}

#[test]
fn both_profiles_emit_observed_metadata_and_navigate_real_files() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for format in formats() {
        let mut first = document("index");
        first.description = Some(String::new());
        first.resolved_wiki_targets = vec!["log".into(), "missing".into()];
        let mut second = document("log");
        second.description = None;
        second.source_path.clear();
        second.updated_at = None;
        second.resolved_wiki_targets = vec!["index".into()];
        let mut third = document("CON:世界");
        third.source_path = " \t".into();
        let documents = vec![first, second, third];
        let plan = export(&original, parent.path(), modes, format.clone(), &documents);
        let directory = plan.directory();

        let index = std::fs::read_to_string(directory.join("index.md")).unwrap();
        let (root, index_body) = header(&index);
        assert_eq!(root, serde_json::json!({"okf_version":"0.2"}));
        assert_eq!(assert_links_exist(index_body, &directory), 3);
        for d in &documents {
            let path = directory.join("pages").join(page(&d.slug));
            let text = std::fs::read_to_string(&path).unwrap();
            let (metadata, body) = header(&text);
            assert_eq!(metadata["type"], d.kind);
            assert_eq!(metadata["id"], d.slug);
            assert_eq!(metadata["title"], d.title);
            assert_eq!(metadata["generated"]["by"], "process:corpus-knowledge-wiki");
            assert_eq!(metadata["graphit"].as_object().unwrap().len(), 15);
            assert_eq!(metadata["graphit"]["summary"], "editorial preview");
            assert_eq!(metadata["graphit"]["source_path"], d.source_path);
            assert_eq!(
                metadata["graphit"]["description"],
                serde_json::json!(d.description)
            );
            match &d.description {
                Some(value) => assert_eq!(metadata["description"], *value),
                None => assert!(metadata.get("description").is_none()),
            }
            assert_eq!(metadata["graphit"]["stale_after_ns"], 60_000_000_000_u64);
            assert_eq!(
                metadata["graphit"]["stale_since"],
                "1970-01-01T00:00:00.000000009Z"
            );
            assert!(metadata.get("okf_version").is_none());
            for absent in ["stale_after", "verified", "status", "revision", "timestamp"] {
                assert!(
                    metadata.get(absent).is_none(),
                    "unexpected inferred {absent}"
                );
            }
            if d.updated_at.is_some() {
                assert_eq!(
                    metadata["generated"]["at"],
                    "1969-12-31T23:59:59.999999983Z"
                );
                assert_eq!(
                    metadata["sources"],
                    serde_json::json!([{"resource":d.source_path}])
                );
            } else {
                assert!(metadata["generated"].get("at").is_none());
                assert!(metadata.get("sources").is_none());
                assert!(metadata.get("description").is_none());
                assert!(metadata["graphit"]["updated_at"].is_null());
            }
            let (navigation, authored) = body.split_once("## Content\n\n").unwrap();
            assert_links_exist(navigation, path.parent().unwrap());
            let expected = "  # Literal\r\n\r\n[[log]] [authored](original.md)  \r\n";
            if matches!(format, Format::Obsidian { .. })
                && d.resolved_wiki_targets.iter().any(|s| s == "log")
            {
                assert_eq!(
                    authored,
                    expected.replace("[[log]]", &format!("[[{}|log]]", page("log")))
                );
            } else {
                assert_eq!(authored, expected);
            }
            assert!(!text.contains("title: Original"));
        }
        let text = std::fs::read_to_string(directory.join("pages").join(page("index"))).unwrap();
        let (_, body) = header(&text);
        let visible: String = Parser::new(body)
            .filter_map(|event| match event {
                Event::Text(text) => Some(text.into_string()),
                _ => None,
            })
            .collect();
        assert!(visible.contains("missing (missing)"));
        assert!(!directory.join("log.md").exists());
        assert!(!directory.join(".obsidian").exists());
        let mut reversed = documents;
        reversed.reverse();
        let again = export(&original, parent.path(), modes, format, &reversed);
        assert_eq!(plan.payload.fingerprint(), again.payload.fingerprint());
        assert_eq!(plan.communities, again.communities);
        again.close().1.unwrap();
        plan.close().1.unwrap();
        assert_clean(parent.path());
    }
}

#[test]
fn obsidian_body_navigates_only_observed_canonical_targets_and_preserves_other_bytes() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    let literal = concat!(
        "# Literal 世界\r\n\r\n",
        "[[log]] [[log#Heading]] [[log#Heading|rótulo **literal**]] [[name%20%E4%B8%96%E7%95%8C|é &amp;]]\r\n\r\n",
        "[[missing]] [[other]] [[Log]] [[log.md]] [[notes/log]] [[oth%65r]] [[https://host.invalid/log]] [[#Heading]]\r\n\r\n",
        "`[[log]]` [plain](log) ![[log]]\\[[log]]\r\n\r\n",
        "```markdown\r\n[[log]]\r\n```\r\n\r\n",
        "<div>\r\n[[log]]\r\n</div>\r\n\r\n",
        "<a href=\"[[log]]\">tag</a>\r\n"
    );
    let expected = literal
        .replacen("[[log]]", &format!("[[{}|log]]", page("log")), 1)
        .replacen(
            "[[log#Heading]]",
            &format!("[[{}#Heading|log#Heading]]", page("log")),
            1,
        )
        .replacen("[[log#", &format!("[[{}#", page("log")), 1)
        .replacen(
            "[[name%20%E4%B8%96%E7%95%8C|",
            &format!("[[{}|", page("name 世界")),
            1,
        );
    let mut first = document("index");
    first.body = format!("\u{feff}---\r\ntitle: envelope\r\n---\r\n{literal}");
    first.resolved_wiki_targets = vec!["log".into(), "name 世界".into(), "missing".into()];
    let documents = [
        first,
        document("log"),
        document("name 世界"),
        document("other"),
    ];
    for format in [Format::Markdown].into_iter().chain(formats()) {
        let obsidian = matches!(format, Format::Obsidian { .. });
        let plan = export(&original, parent.path(), modes, format, &documents);
        let path = plan.directory().join("pages").join(page("index"));
        let text = std::fs::read_to_string(&path).unwrap();
        let (_, body) = header(&text);
        let (_, authored) = body.split_once("## Content\n\n").unwrap();
        assert_eq!(authored, if obsidian { expected.as_str() } else { literal });
        if obsidian {
            let destinations = Parser::new_ext(authored, Options::ENABLE_WIKILINKS)
                .filter_map(|event| match event {
                    Event::Start(Tag::Link {
                        link_type: LinkType::WikiLink { .. },
                        dest_url,
                        ..
                    }) => Some(dest_url.into_string()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                &destinations[..4],
                &[
                    page("log"),
                    format!("{}#Heading", page("log")),
                    format!("{}#Heading", page("log")),
                    page("name 世界")
                ]
            );
            let labels = |source: &str| {
                let mut labels = Vec::<String>::new();
                let mut wiki = false;
                for event in Parser::new_ext(source, Options::ENABLE_WIKILINKS) {
                    match event {
                        Event::Start(Tag::Link {
                            link_type: LinkType::WikiLink { .. },
                            ..
                        }) => {
                            labels.push(String::new());
                            wiki = true;
                        }
                        Event::End(TagEnd::Link) => wiki = false,
                        Event::Text(text) if wiki => labels.last_mut().unwrap().push_str(&text),
                        _ => {}
                    }
                }
                labels
            };
            assert_eq!(labels(authored), labels(literal));
            for destination in &destinations[..4] {
                let filename = destination
                    .split_once('#')
                    .map_or(destination.as_str(), |(path, _)| path);
                assert!(path.parent().unwrap().join(filename).is_file());
            }
            assert!(destinations.iter().any(|target| target == "missing"));
            assert!(destinations.iter().any(|target| target == "other"));
        }
        plan.close().1.unwrap();
        assert_clean(parent.path());
    }
}

#[test]
fn invalid_percent_utf8_target_keeps_the_producer_cause_and_aborts_the_plan() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    let mut d = document("index");
    d.body = "[[%FF#Heading|label]]".into();
    let mut stage = MarkdownExportStage::new_in(
        &original,
        parent.path(),
        modes,
        limits(),
        Format::Obsidian {
            brand: Brand::new("corpus").unwrap(),
        },
    )
    .unwrap();
    stage.push(&original, d).unwrap();
    let error = finish(stage, &original).err().unwrap();
    assert!(matches!(
        error,
        E::Target(ref source) if matches!(source.as_ref(), NativeAuthoredMappingError::PercentUtf8(_))
    ));
    let cause = error.source().unwrap();
    assert!(cause.downcast_ref::<NativeAuthoredMappingError>().is_some());
    assert!(
        cause
            .source()
            .unwrap()
            .downcast_ref::<std::str::Utf8Error>()
            .is_some()
    );
    assert_clean(parent.path());
}

#[test]
fn obsidian_deferred_parser_keeps_yaml_event_link_limits_and_cleans_failed_finish() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for boundary in ["frontmatter", "yaml", "markdown", "links"] {
        let mut bounded = limits();
        match boundary {
            "frontmatter" => bounded.document.max_frontmatter_bytes = n(1),
            "yaml" => bounded.document.max_yaml_events = n(1),
            "markdown" => bounded.document.max_markdown_events = n(1),
            "links" => bounded.document.max_links = n(1),
            _ => unreachable!(),
        }
        let mut d = document("index");
        d.body = "---\ntitle: bounded\n---\n[[log]] [[log#Heading|label]]".into();
        d.resolved_wiki_targets = vec!["log".into()];
        let mut stage = MarkdownExportStage::new_in(
            &original,
            parent.path(),
            modes,
            bounded,
            Format::Obsidian {
                brand: Brand::new("corpus").unwrap(),
            },
        )
        .unwrap();
        stage.push(&original, d).unwrap();
        stage.push(&original, document("log")).unwrap();
        let error = finish(stage, &original).err().unwrap();
        assert!(matches!(error, E::Parse(_)), "{boundary}: {error:?}");
        assert!(error.source().is_some());
        assert_clean(parent.path());
    }
}

#[test]
fn obsidian_discarded_envelope_does_not_consume_the_rendered_file_limit() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    let format = Format::Obsidian {
        brand: Brand::new("corpus").unwrap(),
    };
    let mut first = document("index");
    first.body = "[[log]]".into();
    first.resolved_wiki_targets = vec!["log".into()];
    let mut second = document("log");
    second.body.clear();
    let observed = export(
        &original,
        parent.path(),
        modes,
        format.clone(),
        &[first.clone(), second.clone()],
    );
    let directory = observed.directory();
    let mut largest = std::fs::metadata(directory.join("index.md")).unwrap().len();
    for slug in ["index", "log"] {
        largest = largest.max(
            std::fs::metadata(directory.join("pages").join(page(slug)))
                .unwrap()
                .len(),
        );
    }
    let fingerprint = observed.payload.fingerprint();
    observed.close().1.unwrap();
    first.body = format!("---\ntitle: {}\n---\n[[log]]", "x".repeat(8192));
    assert!(first.body.len() > usize::try_from(largest).unwrap());
    let exact = usize::try_from(largest).unwrap();
    for bound in [exact, exact - 1] {
        let mut bounded = limits();
        bounded.max_file_bytes = n(bound);
        let mut stage =
            MarkdownExportStage::new_in(&original, parent.path(), modes, bounded, format.clone())
                .unwrap();
        stage.push(&original, first.clone()).unwrap();
        stage.push(&original, second.clone()).unwrap();
        let result = finish(stage, &original);
        if bound == exact {
            let actual = result.unwrap();
            assert_eq!(actual.payload.fingerprint(), fingerprint);
            actual.close().1.unwrap();
        } else {
            assert!(matches!(result, Err(E::Limit("file bytes"))));
        }
        assert_clean(parent.path());
    }
}

#[test]
fn unrepresentable_type_is_refused_before_page_spool_without_fabricating_fields() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for format in formats() {
        for kind in ["", "  ", "Attested Computation"] {
            let mut stage = MarkdownExportStage::new_in(
                &original,
                parent.path(),
                modes,
                limits(),
                format.clone(),
            )
            .unwrap();
            let mut d = document("unsupported");
            d.kind = kind.into();
            assert!(matches!(
                stage.push(&original, d),
                Err(E::InvalidOkfMetadata(_))
            ));
            assert!(stage.is_empty());
            assert!(matches!(finish(stage, &original), Err(E::NoDocuments)));
            assert_clean(parent.path());
        }
    }
    let mut d = document("ordinary-native");
    d.kind = "Attested Computation".into();
    let plan = export(&original, parent.path(), modes, Format::Markdown, &[d]);
    let text =
        std::fs::read_to_string(plan.directory().join("pages").join(page("ordinary-native")))
            .unwrap();
    let (metadata, _) = header(&text);
    assert_eq!(metadata["type"], "Attested Computation");
    assert!(metadata.get("generated").is_none());
    plan.close().1.unwrap();
    assert_clean(parent.path());
}

#[test]
fn cancellation_deadline_and_request_identity_keep_real_io_causes_and_cleanup() {
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for format in formats() {
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let cancelled =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
        let error =
            MarkdownExportStage::new_in(&cancelled, parent.path(), modes, limits(), format.clone())
                .err()
                .unwrap();
        assert!(matches!(error, E::Request(ref e) if e.kind() == io::ErrorKind::Interrupted));
        assert!(error.source().is_some());
        assert_clean(parent.path());

        let expired = SnapshotContext::new(
            Instant::now() - Duration::from_secs(1),
            Cancellation::default(),
        );
        let error =
            MarkdownExportStage::new_in(&expired, parent.path(), modes, limits(), format.clone())
                .err()
                .unwrap();
        assert!(matches!(error, E::Request(ref e) if e.kind() == io::ErrorKind::TimedOut));
        assert_clean(parent.path());

        let original = context();
        let foreign = context();
        let mut stage =
            MarkdownExportStage::new_in(&original, parent.path(), modes, limits(), format.clone())
                .unwrap();
        assert!(matches!(
            stage.push(&foreign, document("a")),
            Err(E::References(ReferenceError::DifferentRequest))
        ));
        drop(stage);
        assert_clean(parent.path());

        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        let mut stage =
            MarkdownExportStage::new_in(&original, parent.path(), modes, limits(), format).unwrap();
        stage.push(&original, document("a")).unwrap();
        cancellation.cancel();
        assert!(
            matches!(finish(stage, &original), Err(E::Request(ref e)) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert_clean(parent.path());
    }
}

#[test]
fn file_parser_reference_and_duplicate_limits_abort_owned_spools() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for format in formats() {
        for target in ["file", "parser", "reference"] {
            let mut bounded = limits();
            match target {
                "file" => bounded.max_file_bytes = n(1),
                "parser" => bounded.document.max_document_bytes = n(1),
                "reference" => bounded.references.max_identifier_bytes = n(1),
                _ => unreachable!(),
            }
            let mut stage = MarkdownExportStage::new_in(
                &original,
                parent.path(),
                modes,
                bounded,
                format.clone(),
            )
            .unwrap();
            let error = stage.push(&original, document("bounded")).unwrap_err();
            assert!(
                matches!(
                    (target, &error),
                    ("file", E::Limit(_))
                        | ("parser", E::Parse(_))
                        | ("reference", E::References(_))
                ),
                "{target}: {error:?}"
            );
            drop(stage);
            assert_clean(parent.path());
        }
        let mut stage =
            MarkdownExportStage::new_in(&original, parent.path(), modes, limits(), format).unwrap();
        stage.push(&original, document("a")).unwrap();
        assert!(matches!(
            stage.push(&original, document("a")),
            Err(E::References(ReferenceError::DuplicatePage))
        ));
        drop(stage);
        assert_clean(parent.path());
    }
}

#[test]
fn complete_file_limit_counts_profile_yaml_and_literal_suffix() {
    let original = context();
    let parent = tempfile::tempdir().unwrap();
    let modes = modes();
    for format in formats() {
        let d = document("bounded-output");
        let observed = export(
            &original,
            parent.path(),
            modes,
            format.clone(),
            std::slice::from_ref(&d),
        );
        let directory = observed.directory();
        let page_bytes = std::fs::metadata(directory.join("pages").join(page(&d.slug)))
            .unwrap()
            .len();
        let index_bytes = std::fs::metadata(directory.join("index.md")).unwrap().len();
        let exact = usize::try_from(page_bytes.max(index_bytes)).unwrap();
        observed.close().1.unwrap();
        for bound in [exact, exact - 1] {
            let mut bounded = limits();
            bounded.max_file_bytes = n(bound);
            let mut stage = MarkdownExportStage::new_in(
                &original,
                parent.path(),
                modes,
                bounded,
                format.clone(),
            )
            .unwrap();
            let result = match stage.push(&original, d.clone()) {
                Ok(()) => finish(stage, &original),
                Err(error) => {
                    drop(stage);
                    Err(error)
                }
            };
            if bound == exact {
                result.unwrap().close().1.unwrap();
            } else {
                assert!(matches!(result, Err(E::Limit(_))));
            }
            assert_clean(parent.path());
        }
    }
}
