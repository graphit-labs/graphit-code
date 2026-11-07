//! Guide destinations are data resolved by the page's existing admitted reader.
use crate::{
    document::{DocumentParseError, document_navigation},
    native_document_mapping::{
        NativeAuthoredMappingError, NativeAuthoredMappingLimits, local_target,
        relative_markdown_target,
    },
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{
    knowledge_document_lifecycle::{KnowledgeSourceIdentity, KnowledgeSourceIdentityError},
    knowledge_index::{KnowledgeDocument, validate_slug},
};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::num::NonZeroUsize;

impl From<DocumentParseError> for graphit_data::knowledge_index::KnowledgeIndexError {
    fn from(error: DocumentParseError) -> Self {
        match error {
            DocumentParseError::Request(error) => Self::Request(error),
            error => Self::Request(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NavigationLink {
    pub target: String,
    pub slug: Option<String>,
    pub title: Option<String>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DocumentNavigation {
    pub previous: Option<NavigationLink>,
    pub next: Option<NavigationLink>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavigationDestination {
    Slug(String),
    SourcePath(String),
}

/// Read declarations without claiming destination access or availability.
pub fn declared_navigation(
    original: &SnapshotContext,
    document: &KnowledgeDocument,
) -> Result<DocumentNavigation, DocumentParseError> {
    let authored = document_navigation(original, &document.body)?;
    let link = |target| NavigationLink {
        target,
        slug: None,
        title: None,
    };
    Ok(DocumentNavigation {
        previous: authored.previous.map(link),
        next: authored.next.map(link),
    })
}

fn destination(raw: &str) -> Option<(String, LinkType)> {
    let raw = raw.trim();
    let mut events = Parser::new_ext(raw, Options::ENABLE_WIKILINKS);
    if matches!(events.next(), Some(Event::Start(Tag::Paragraph)))
        && let Some(Event::Start(Tag::Link {
            link_type,
            dest_url,
            ..
        })) = events.next()
    {
        let destination = dest_url.into_string();
        let mut ended = false;
        for event in events.by_ref() {
            if matches!(event, Event::End(TagEnd::Link)) {
                ended = true;
                break;
            }
        }
        return (ended
            && matches!(events.next(), Some(Event::End(TagEnd::Paragraph)))
            && events.next().is_none())
        .then_some((destination, link_type));
    }
    Some((raw.to_owned(), LinkType::Inline))
}

/// Reuse the native wiki/Markdown resolver. An invalid, external or escaping
/// destination is unavailable data, never permission to select another source.
pub fn navigation_destination(
    original: &SnapshotContext,
    slug: &str,
    source_path: &str,
    raw: &str,
    maximum: NonZeroUsize,
) -> Result<Option<NavigationDestination>, std::io::Error> {
    original.check()?;
    if raw.len() > maximum.get() {
        return Ok(None);
    }
    let Some((target, kind)) = destination(raw) else {
        return Ok(None);
    };
    let limits = NativeAuthoredMappingLimits {
        max_field_bytes: maximum,
        max_uri_bytes: maximum,
        max_target_bytes: maximum,
    };
    let normalized = (|| -> Result<_, NativeAuthoredMappingError> {
        if let Some(relative) = relative_markdown_target(&target, kind, source_path, 0, limits)? {
            let identity = KnowledgeSourceIdentity::from_relative_path(&relative, None, maximum)
                .map_err(NativeAuthoredMappingError::Identity)?;
            Ok(Some(NavigationDestination::SourcePath(
                identity.source_path().to_owned(),
            )))
        } else if !matches!(kind, LinkType::WikiLink { .. })
            && target
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .ends_with(".md")
        {
            Ok(None)
        } else {
            Ok(local_target(&target, kind, slug, 0)?
                .filter(|slug| slug.len() <= maximum.get() && validate_slug(slug).is_ok())
                .map(|slug| NavigationDestination::Slug(slug.into_owned())))
        }
    })();
    let result = match normalized {
        Ok(value) => value,
        Err(
            NativeAuthoredMappingError::InvalidTarget { .. }
            | NativeAuthoredMappingError::PercentUtf8(_)
            | NativeAuthoredMappingError::LimitExceeded(_),
        ) => None,
        Err(NativeAuthoredMappingError::Identity(
            KnowledgeSourceIdentityError::InvalidRelativePath
            | KnowledgeSourceIdentityError::NonUnicodePath
            | KnowledgeSourceIdentityError::InvalidAuthoredId
            | KnowledgeSourceIdentityError::InvalidManagedSourcePath
            | KnowledgeSourceIdentityError::LimitExceeded(_),
        )) => None,
        Err(NativeAuthoredMappingError::Request(error)) => return Err(error),
        Err(error) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    };
    original.check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        path::Path,
        time::{Duration, Instant},
    };
    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }
    #[test]
    fn destinations_reuse_literal_wiki_ids_relative_paths_and_existing_encoding() {
        let original = context();
        let limit = NonZeroUsize::new(4096).unwrap();
        let source =
            KnowledgeSourceIdentity::from_relative_path(Path::new("docs/guide.md"), None, limit)
                .unwrap();
        for raw in [
            "next-page",
            "[[next-page]]",
            "[Continue](next-page)",
            " next-page#section ",
            "next%2dpage",
        ] {
            assert_eq!(
                navigation_destination(&original, "guide", source.source_path(), raw, limit)
                    .unwrap(),
                Some(NavigationDestination::Slug("next-page".into())),
                "{raw}"
            );
        }
        let destination = KnowledgeSourceIdentity::from_relative_path(
            Path::new("docs/nested/next.md"),
            None,
            limit,
        )
        .unwrap();
        for raw in ["nested/next.md", "[Continue](nested/next.md#section)"] {
            assert_eq!(
                navigation_destination(&original, "guide", source.source_path(), raw, limit)
                    .unwrap(),
                Some(NavigationDestination::SourcePath(
                    destination.source_path().into()
                ))
            );
        }
        for raw in [
            "https://example.com/next.md",
            "../../../outside.md",
            "/absolute.md",
            "[[next-page]] [[another]]",
            "nested\\next.md",
        ] {
            assert_eq!(
                navigation_destination(&original, "guide", source.source_path(), raw, limit)
                    .unwrap(),
                None,
                "{raw}"
            );
        }
    }
    #[test]
    fn cancelled_destination_normalization_preserves_original_error() {
        let cancellation = Cancellation::default();
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation.clone(),
        );
        cancellation.cancel();
        assert_eq!(
            navigation_destination(
                &original,
                "guide",
                "docs/guide.md",
                "next",
                NonZeroUsize::new(4096).unwrap()
            )
            .unwrap_err()
            .kind(),
            std::io::ErrorKind::Interrupted
        );
    }
}
