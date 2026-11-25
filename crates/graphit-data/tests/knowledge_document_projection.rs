use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::knowledge_document_projection::*;
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant, SystemTime},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn limits() -> KnowledgeDocumentProjectionLimits {
    let large = n(1_000_000);
    KnowledgeDocumentProjectionLimits {
        max_documents: large,
        max_observations: large,
        max_rows: large,
        max_field_bytes: large,
        max_text_bytes: large,
    }
}
fn request() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn recipe() -> RecipeFields<'static> {
    RecipeFields {
        id: AUTHORED_RECIPE_ID,
        parameters: Some(AuthoredParameters {
            fallback_title: "Fallback",
            kind_on_absent: "default",
            max_summary_bytes: n(8),
        }),
    }
}
fn page<'data>(
    slug: &'data str,
    path: &'data str,
    targets: &'data [&'data str],
) -> AuthoredPageFields<'data> {
    AuthoredPageFields {
        slug: Some(slug),
        title: Some(""),
        body: Some("\u{feff}# Título\r\n\r\n linha\n"),
        kind: Some("custom editorial"),
        summary: Some(""),
        description: None,
        summary_truncated: true,
        source_path: path,
        physical_digest: None,
        compilation_digest: CompilationDigest32::from_bytes([2; 32]),
        recipe: recipe(),
        updated_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(100)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}
fn observation(path: &str) -> CompilationObservationFields<'_> {
    CompilationObservationFields {
        path,
        state: CompilationObservationState::Present {
            compilation_digest: CompilationDigest32::from_bytes([2; 32]),
            recipe: recipe(),
        },
    }
}
fn assert_request_error(error: KnowledgeDocumentProjectionError, kind: io::ErrorKind) {
    assert!(
        matches!(error,KnowledgeDocumentProjectionError::Request(ref source) if source.kind()==kind)
    );
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        kind
    );
}

#[test]
fn preserves_full_editorial_values_and_borrowed_target_group() {
    let context = request();
    let targets = ["", "a", "b", "b", "missing"];
    let mut second = page("b", "docs/b.md", &[]);
    second.kind = Some("");
    second.description = Some("");
    second.physical_digest = Some(PhysicalSourceDigest32::from_bytes([1; 32]));
    let mut third = page("c", "docs/c.md", &[]);
    third.description = Some(" \r\n descrição \n ");
    let documents = [page("a", "docs/a.md", &targets), second, third];
    let observations = [
        observation("docs/a.md"),
        CompilationObservationFields {
            path: "docs/removed.md",
            state: CompilationObservationState::Removed,
        },
    ];
    let projection = prepare(&context, &documents, &observations, limits()).unwrap();
    let rows = projection.rows(&context).unwrap();
    assert_eq!(rows.len(), 10);
    let MemberRow::DocumentHeader(first) = &rows[0] else {
        panic!("expected header")
    };
    assert_eq!(first.slug, "a");
    assert_eq!(first.title, "");
    assert_eq!(first.body, documents[0].body.unwrap());
    assert_eq!(first.body.as_ptr(), documents[0].body.unwrap().as_ptr());
    assert_eq!(first.kind, "custom editorial");
    assert_eq!(first.summary, "");
    assert!(first.summary_truncated);
    assert_eq!(first.description, None);
    assert_eq!(first.physical_digest, None);
    assert_eq!(first.compilation_digest.as_bytes(), &[2; 32]);
    assert_eq!(first.recipe.id(), AUTHORED_RECIPE_ID);
    assert_eq!(first.recipe.parameters().max_summary_bytes, n(8));
    assert_eq!(
        first.resolved_wiki_targets.as_slice().as_ptr(),
        targets.as_ptr()
    );
    for (ordinal, &expected) in targets.iter().enumerate() {
        let MemberRow::WikiTarget {
            document_index,
            ordinal: actual,
            target,
        } = rows[ordinal + 1]
        else {
            panic!("expected target")
        };
        assert_eq!((document_index, actual), (0, ordinal));
        assert_eq!(target, expected);
        assert_eq!(target.as_ptr(), expected.as_ptr());
    }
    let MemberRow::DocumentHeader(second) = &rows[6] else {
        panic!("expected second header")
    };
    assert_eq!(second.kind, "");
    assert_eq!(second.description, Some(""));
    assert_eq!(second.physical_digest.unwrap().as_bytes(), &[1; 32]);
    assert_eq!(second.compilation_digest.as_bytes(), &[2; 32]);
    let MemberRow::DocumentHeader(third) = &rows[7] else {
        panic!("expected third header")
    };
    assert_eq!(third.description, documents[2].description);
    assert_eq!(
        third.description.unwrap().as_ptr(),
        documents[2].description.unwrap().as_ptr()
    );
    assert!(matches!(
        rows[8],
        MemberRow::CompilationObservation(CompilationObservationFields {
            state: CompilationObservationState::Present { .. },
            ..
        })
    ));
    assert!(matches!(
        rows[9],
        MemberRow::CompilationObservation(CompilationObservationFields {
            state: CompilationObservationState::Removed,
            ..
        })
    ));
}

#[test]
fn rejects_missing_required_fields_and_incomplete_recipe_without_defaults() {
    let context = request();
    for field in ["slug", "title", "body", "kind", "summary"] {
        let mut document = page("a", "docs/a.md", &[]);
        match field {
            "slug" => document.slug = None,
            "title" => document.title = None,
            "body" => document.body = None,
            "kind" => document.kind = None,
            "summary" => document.summary = None,
            _ => unreachable!(),
        }
        let documents = [document];
        let error = prepare(&context, &documents, &[], limits()).err().unwrap();
        assert!(
            matches!(error,KnowledgeDocumentProjectionError::RequiredField {field:actual,document_index:0} if actual==field)
        );
    }
    for fields in [
        RecipeFields {
            id: "future-recipe",
            parameters: recipe().parameters,
        },
        RecipeFields {
            id: AUTHORED_RECIPE_ID,
            parameters: None,
        },
    ] {
        let mut document = page("a", "docs/a.md", &[]);
        document.recipe = fields;
        assert!(matches!(
            prepare(&context, &[document], &[], limits()),
            Err(KnowledgeDocumentProjectionError::InvalidRecipe)
        ));
        let observations = [CompilationObservationFields {
            path: "docs/a.md",
            state: CompilationObservationState::Present {
                compilation_digest: CompilationDigest32::from_bytes([2; 32]),
                recipe: fields,
            },
        }];
        assert!(matches!(
            prepare(&context, &[], &observations, limits()),
            Err(KnowledgeDocumentProjectionError::InvalidRecipe)
        ));
    }
    assert!(NonZeroUsize::new(0).is_none());
    let mut document = page("a", "", &[]);
    document.body = Some("PRIVATE BODY");
    let error = prepare(&context, &[document], &[], limits()).err().unwrap();
    assert!(matches!(
        error,
        KnowledgeDocumentProjectionError::InvalidPath
    ));
    assert!(!error.to_string().contains("PRIVATE BODY"));
    assert!(matches!(
        prepare(
            &context,
            &[],
            &[CompilationObservationFields {
                path: "",
                state: CompilationObservationState::Removed
            }],
            limits()
        ),
        Err(KnowledgeDocumentProjectionError::InvalidPath)
    ));
    document.source_path = "docs/a.md";
    document.stale_after = Some(Duration::ZERO);
    assert!(matches!(
        prepare(&context, &[document], &[], limits()),
        Err(KnowledgeDocumentProjectionError::InvalidPolicy)
    ));
    document.stale_after = Some(Duration::from_secs(1));
    document.updated_at = None;
    assert!(matches!(
        prepare(&context, &[document], &[], limits()),
        Err(KnowledgeDocumentProjectionError::InvalidPolicy)
    ));
}

#[test]
fn raw_duplicate_members_and_configuration_drift_are_data() {
    let context = request();
    let targets = ["a", "missing", "missing", ""];
    let documents = [
        page("a", "docs/a.md", &targets),
        page("a", "docs/a.md", &[]),
    ];
    let mut changed = recipe();
    changed.parameters.as_mut().unwrap().max_summary_bytes = n(16);
    let observations = [
        observation("docs/a.md"),
        CompilationObservationFields {
            path: "docs/a.md",
            state: CompilationObservationState::Present {
                compilation_digest: CompilationDigest32::from_bytes([3; 32]),
                recipe: changed,
            },
        },
    ];
    let projection = prepare(&context, &documents, &observations, limits()).unwrap();
    assert_eq!(projection.rows(&context).unwrap().len(), 8);
    let rows = projection.rows(&context).unwrap();
    let MemberRow::CompilationObservation(CompilationObservationFields {
        state:
            CompilationObservationState::Present {
                compilation_digest,
                recipe,
            },
        ..
    }) = rows[7]
    else {
        panic!("expected drift")
    };
    assert_eq!(compilation_digest.as_bytes(), &[3; 32]);
    assert_eq!(recipe.parameters.unwrap().max_summary_bytes, n(16));
}

#[test]
fn corpus_and_text_limits_reject_oversized_inputs_at_exact_boundaries() {
    let context = request();
    let targets = ["", "a", "b", "b", "missing"];
    let documents = [
        page("a", "docs/a.md", &targets),
        page("b", "docs/b.md", &[]),
    ];
    let observations = [
        observation("docs/a.md"),
        CompilationObservationFields {
            path: "docs/removed.md",
            state: CompilationObservationState::Removed,
        },
    ];
    // Literal fixture inventory, independent of the implementation's traversal.
    let document_text = "a".len()
        + documents[0].body.unwrap().len()
        + "custom editorial".len()
        + "docs/a.md".len()
        + AUTHORED_RECIPE_ID.len()
        + "Fallback".len()
        + "default".len();
    let target_text = "a".len() + "b".len() + "b".len() + "missing".len();
    let observation_text = "docs/a.md".len()
        + AUTHORED_RECIPE_ID.len()
        + "Fallback".len()
        + "default".len()
        + "docs/removed.md".len();
    let total_text = 2 * document_text + target_text + observation_text;
    for (kind, quantity) in [
        (ProjectionLimit::Documents, 2),
        (ProjectionLimit::Observations, 2),
        (ProjectionLimit::Rows, 9),
        (
            ProjectionLimit::FieldBytes,
            documents[0].body.unwrap().len(),
        ),
        (ProjectionLimit::TextBytes, total_text),
    ] {
        for (cap, passes) in [(quantity, true), (quantity - 1, false)] {
            let mut bound = limits();
            match kind {
                ProjectionLimit::Documents => bound.max_documents = n(cap),
                ProjectionLimit::Observations => bound.max_observations = n(cap),
                ProjectionLimit::Rows => bound.max_rows = n(cap),
                ProjectionLimit::FieldBytes => bound.max_field_bytes = n(cap),
                ProjectionLimit::TextBytes => bound.max_text_bytes = n(cap),
            }
            let result = prepare(&context, &documents, &observations, bound);
            if passes {
                assert!(result.is_ok(), "{kind:?} at equality");
            } else {
                assert!(
                    matches!(result, Err(KnowledgeDocumentProjectionError::LimitExceeded {kind: actual}) if actual == kind),
                    "{kind:?} above cap"
                );
            }
        }
    }
}

#[test]
fn cancellation_expiry_and_foreign_request_never_refresh_the_owner() {
    let context = request();
    let documents = [page("a", "docs/a.md", &[])];
    let observations = [observation("docs/a.md")];
    let projection = prepare(&context, &documents, &observations, limits()).unwrap();
    let foreign = SnapshotContext::new(
        Instant::now() + Duration::from_secs(600),
        Cancellation::default(),
    );
    assert!(matches!(
        projection.rows(&foreign),
        Err(KnowledgeDocumentProjectionError::DifferentRequest)
    ));
    assert!(matches!(
        projection.supplied_context(&foreign),
        Err(KnowledgeDocumentProjectionError::DifferentRequest)
    ));
    assert_eq!(projection.rows(&context).unwrap().len(), 2);
    assert!(std::ptr::eq(
        projection.supplied_context(&context).unwrap(),
        &context
    ));
    context.cancellation().cancel();
    assert_request_error(
        projection.rows(&foreign).err().unwrap(),
        io::ErrorKind::Interrupted,
    );
    assert_request_error(
        projection.supplied_context(&context).err().unwrap(),
        io::ErrorKind::Interrupted,
    );
    assert_request_error(
        prepare(
            &context,
            &[AuthoredPageFields {
                slug: None,
                ..documents[0]
            }],
            &[],
            limits(),
        )
        .err()
        .unwrap(),
        io::ErrorKind::Interrupted,
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_request_error(
        prepare(&expired, &[], &[], limits()).err().unwrap(),
        io::ErrorKind::TimedOut,
    );
}

#[test]
fn empty_data_is_valid_without_a_published_corpus_claim() {
    let context = request();
    let projection = prepare(&context, &[], &[], limits()).unwrap();
    assert!(projection.rows(&context).unwrap().is_empty());
}

// Positive lifetime witness: DATA views/copies can outlive the prepared owner,
// while their raw input stays alive. This does not export a request capability.
fn original_targets<'data>(
    projection: &PreparedKnowledgeDocumentRows<'data, '_>,
    context: &SnapshotContext,
) -> &'data [&'data str] {
    let MemberRow::DocumentHeader(header) = &projection.rows(context).unwrap()[0] else {
        panic!("expected header")
    };
    header.resolved_wiki_targets.as_slice()
}
#[test]
fn target_group_outlives_temporary_descriptors_and_prepared_owner() {
    let context = request();
    let targets = ["b"];
    let view;
    let retained;
    {
        let projection = {
            let documents = [page("a", "docs/a.md", &targets)];
            let observations = [];
            prepare(&context, &documents, &observations, limits()).unwrap()
        };
        view = original_targets(&projection, &context);
        retained = projection.supplied_context(&context).unwrap();
    }
    assert_eq!(view, &targets);
    assert_eq!(view.as_ptr(), targets.as_ptr());
    assert!(std::ptr::eq(retained, &context));
}
