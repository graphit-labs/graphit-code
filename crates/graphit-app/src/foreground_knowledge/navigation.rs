//! At most two admitted point reads. Only explicit misses mean unavailable.
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::{
    self, KnowledgeDocument, KnowledgeHead, KnowledgeHeadRead, KnowledgeHeadReadLimits,
    KnowledgeIndexError, KnowledgeIndexReadLimits,
};
use graphit_knowledge::{
    imported_knowledge::{CapturedImportedKnowledge, ImportedKnowledgeReadError},
    navigation::{
        DocumentNavigation, NavigationDestination, NavigationLink, declared_navigation,
        navigation_destination,
    },
};
use std::sync::Arc;

fn prepare(
    original: &SnapshotContext,
    document: &KnowledgeDocument,
) -> Result<DocumentNavigation, KnowledgeIndexError> {
    declared_navigation(original, document).map_err(KnowledgeIndexError::from)
}
fn head_limits(limits: KnowledgeIndexReadLimits) -> KnowledgeHeadReadLimits {
    KnowledgeHeadReadLimits {
        max_slug_bytes: limits.content.max_identifier_bytes,
        native: limits.native,
        batch_rows: limits.batch_rows,
    }
}
fn resolved(link: &mut NavigationLink, head: Option<KnowledgeHead>) {
    if let Some(head) = head {
        link.slug = Some(head.slug);
        link.title = Some(head.title);
    }
}

pub(crate) async fn project(
    original: &Arc<SnapshotContext>,
    admission: &graphit_auth::ProjectReadAdmission<'_>,
    document: &KnowledgeDocument,
    limits: KnowledgeIndexReadLimits,
) -> Result<DocumentNavigation, KnowledgeIndexError> {
    admission.recheck()?;
    let mut navigation = prepare(original, document)?;
    for link in navigation
        .previous
        .iter_mut()
        .chain(navigation.next.iter_mut())
    {
        let target = navigation_destination(
            original,
            &document.slug,
            &document.source_path,
            &link.target,
            limits.content.max_identifier_bytes,
        )?;
        let target = match target {
            Some(NavigationDestination::Slug(slug)) => {
                match knowledge_index::read_native_knowledge_head_by_slug(
                    original,
                    admission,
                    &slug,
                    head_limits(limits),
                )
                .await?
                {
                    KnowledgeHeadRead::Published { mut heads, .. } => heads.pop(),
                    KnowledgeHeadRead::NoCorpus => None,
                }
            }
            Some(NavigationDestination::SourcePath(path)) => {
                match knowledge_index::read_native_knowledge_head_by_source_path(
                    original,
                    admission,
                    &path,
                    head_limits(limits),
                )
                .await?
                {
                    KnowledgeHeadRead::Published { mut heads, .. } => heads.pop(),
                    KnowledgeHeadRead::NoCorpus => None,
                }
            }
            None => None,
        };
        resolved(link, target);
    }
    original.check()?;
    admission.recheck()?;
    Ok(navigation)
}

pub(crate) async fn imported(
    original: &Arc<SnapshotContext>,
    reader: &CapturedImportedKnowledge<'_>,
    document: &KnowledgeDocument,
    limits: KnowledgeIndexReadLimits,
) -> Result<DocumentNavigation, ImportedKnowledgeReadError> {
    reader.recheck()?;
    let mut navigation = prepare(original, document).map_err(ImportedKnowledgeReadError::Data)?;
    for link in navigation
        .previous
        .iter_mut()
        .chain(navigation.next.iter_mut())
    {
        let target = navigation_destination(
            original,
            &document.slug,
            &document.source_path,
            &link.target,
            limits.content.max_identifier_bytes,
        )
        .map_err(|error| ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(error)))?;
        let target = match target {
            Some(NavigationDestination::Slug(slug)) => {
                reader.head_by_slug(&slug, head_limits(limits)).await?
            }
            Some(NavigationDestination::SourcePath(path)) => {
                reader
                    .head_by_source_path(&path, head_limits(limits))
                    .await?
            }
            None => None,
        };
        resolved(link, target);
    }
    original
        .check()
        .map_err(|error| ImportedKnowledgeReadError::Data(KnowledgeIndexError::Request(error)))?;
    reader.recheck()?;
    Ok(navigation)
}

pub(crate) async fn remote(
    opened: &crate::source_server::remote_native::Opened<'_>,
    lease: &graphit_auth::ServiceCredentialLease<'_>,
    document: &KnowledgeDocument,
    limits: KnowledgeIndexReadLimits,
) -> Result<DocumentNavigation, crate::remote_native_mount::ErrorKind> {
    use crate::remote_native_mount::ErrorKind;
    opened.check(lease)?;
    let original = opened.selection.request().snapshot();
    let mut navigation = prepare(original, document).map_err(ErrorKind::Knowledge)?;
    for link in navigation
        .previous
        .iter_mut()
        .chain(navigation.next.iter_mut())
    {
        let target = navigation_destination(
            original,
            &document.slug,
            &document.source_path,
            &link.target,
            limits.content.max_identifier_bytes,
        )
        .map_err(|error| ErrorKind::Knowledge(KnowledgeIndexError::Request(error)))?;
        let target = match target {
            Some(NavigationDestination::Slug(slug)) => {
                opened
                    .head_by_slug(lease, &slug, head_limits(limits))
                    .await?
            }
            Some(NavigationDestination::SourcePath(path)) => {
                opened
                    .head_by_source_path(lease, &path, head_limits(limits))
                    .await?
            }
            None => None,
        };
        resolved(link, target);
    }
    opened.check(lease)?;
    Ok(navigation)
}
