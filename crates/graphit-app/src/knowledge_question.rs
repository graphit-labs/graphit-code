//! One authorized PROJECT corpus and one caller-owned native execution.
use graphit_ai::{
    cli_codex::{CodexFailure, CodexReceipt},
    cli_codex_wire::CodexWireLimits,
    codex_completion::{
        CodexCompletionRun, CompletionFailure, CompletionLimits, CompletionOutcome,
    },
};
use graphit_auth::{AuthError, ProjectReadAdmission, ProjectReadModule};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::{
    KnowledgeDocument, KnowledgeGeneration, KnowledgeHead, KnowledgeIndexError,
    KnowledgeIndexReadLimits,
};
use graphit_data::knowledge_search::{
    KnowledgeSearchError, KnowledgeSearchRead, KnowledgeSelectedDocuments,
    KnowledgeSelectedDocumentsState, capture_native_knowledge_search,
};
use serde::Serialize;
use std::{
    collections::TryReserveError, error::Error, fmt, future::Future, io, num::NonZeroUsize,
    sync::Arc,
};

pub struct KnowledgeQuestionModel<'n> {
    pub requested_model: &'n str,
    pub wire_limits: &'n CodexWireLimits,
    pub completion_limits: CompletionLimits,
}

#[derive(Clone, Copy)]
pub struct KnowledgeQuestionLimits {
    pub read: KnowledgeIndexReadLimits,
    pub max_query_bytes: NonZeroUsize,
    pub max_selected_pages: NonZeroUsize,
    pub max_prompt_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeQuestionDisposition {
    Answer,
    EmptyAnswer,
    EvidenceUnavailable,
    Failed,
    NoCompletionObservation,
}

/// `provided_pages` records input, never model citations or permission to read more pages.
pub enum KnowledgeQuestionResult<'n> {
    NoCorpus,
    PublishedEmpty {
        generation: KnowledgeGeneration,
    },
    EmptySelection {
        generation: KnowledgeGeneration,
    },
    Executed {
        generation: KnowledgeGeneration,
        provided_pages: Vec<KnowledgeHead>,
        receipt: Box<CodexReceipt<'n>>,
        disposition: KnowledgeQuestionDisposition,
    },
}

pub enum KnowledgeQuestionError {
    Request(io::Error),
    Auth(AuthError),
    Data(Box<KnowledgeIndexError>),
    Search(Box<KnowledgeSearchError>),
    Completion(Box<CompletionFailure>),
    Encoding(serde_json::Error),
    Instruction(Box<graphit_runtime::instructions::InstructionError>),
    Allocation(TryReserveError),
    Limit(&'static str),
    InvalidAdmission,
    ForeignRequest,
    InvalidSelection,
    InvalidQuestion,
    InvalidAttemptLimit,
    ExecutionRequired,
    Codex,
}
impl KnowledgeQuestionError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Request(_) => "request",
            Self::Auth(_) => "authorization",
            Self::Data(_) => "corpus",
            Self::Search(_) => "selected pages",
            Self::Completion(_) => "completion policy",
            Self::Encoding(_) => "prompt encoding",
            Self::Instruction(_) => "instruction rendering",
            Self::Allocation(_) => "allocation",
            Self::Limit(_) => "limit",
            Self::InvalidAdmission => "admission class",
            Self::ForeignRequest => "foreign request",
            Self::InvalidSelection => "page selection",
            Self::InvalidQuestion => "question",
            Self::InvalidAttemptLimit => "attempt limit",
            Self::ExecutionRequired => "prepared execution required",
            Self::Codex => "Codex execution",
        }
    }
}
impl fmt::Debug for KnowledgeQuestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}
impl fmt::Display for KnowledgeQuestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "knowledge question: {}", self.kind())
    }
}
impl Error for KnowledgeQuestionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Data(e) => Some(e.as_ref()),
            Self::Search(e) => Some(e.as_ref()),
            Self::Completion(e) => Some(e.as_ref()),
            Self::Encoding(e) => Some(e),
            Self::Instruction(e) => Some(e.as_ref()),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}

/// A post-execution denial retains the actual receipt and its remaining Process owner.
pub struct KnowledgeQuestionFailure<'n> {
    error: KnowledgeQuestionError,
    execution: Option<Box<Result<CodexReceipt<'n>, CodexFailure<'n>>>>,
}
impl<'n> KnowledgeQuestionFailure<'n> {
    pub fn error(&self) -> &KnowledgeQuestionError {
        &self.error
    }
    pub fn execution(&self) -> Option<&Result<CodexReceipt<'n>, CodexFailure<'n>>> {
        self.execution.as_deref()
    }
    pub fn execution_mut(&mut self) -> Option<&mut Result<CodexReceipt<'n>, CodexFailure<'n>>> {
        self.execution.as_deref_mut()
    }
}
impl fmt::Debug for KnowledgeQuestionFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeQuestionFailure")
            .field("error", &self.error)
            .field("execution_observed", &self.execution.is_some())
            .finish()
    }
}
impl fmt::Display for KnowledgeQuestionFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl Error for KnowledgeQuestionFailure<'_> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        if matches!(self.error, KnowledgeQuestionError::Codex) {
            self.execution
                .as_deref()
                .and_then(|e| e.as_ref().err())
                .and_then(Error::source)
        } else {
            Some(&self.error)
        }
    }
}
impl<'n> From<KnowledgeQuestionError> for KnowledgeQuestionFailure<'n> {
    fn from(error: KnowledgeQuestionError) -> Self {
        Self {
            error,
            execution: None,
        }
    }
}

pub(crate) fn check_read(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
) -> Result<(), KnowledgeQuestionError> {
    original.check().map_err(KnowledgeQuestionError::Request)?;
    if read.module() != ProjectReadModule::Knowledge {
        return Err(KnowledgeQuestionError::InvalidAdmission);
    }
    read.recheck().map_err(KnowledgeQuestionError::Auth)?;
    if !Arc::ptr_eq(
        original,
        read.request()
            .map_err(KnowledgeQuestionError::Auth)?
            .snapshot(),
    ) {
        return Err(KnowledgeQuestionError::ForeignRequest);
    }
    Ok(())
}

#[derive(Serialize)]
struct Prompt<'a> {
    instruction: &'a str,
    question: &'a str,
    pages: Vec<PromptPage<'a>>,
}
#[derive(Serialize)]
struct PromptPage<'a> {
    slug: &'a str,
    title: &'a str,
    body: &'a str,
}

struct PromptWriter<'a> {
    original: &'a SnapshotContext,
    bytes: Vec<u8>,
    limit: usize,
    first: Option<KnowledgeQuestionError>,
}
impl PromptWriter<'_> {
    fn terminal(&mut self, error: KnowledgeQuestionError) -> io::Error {
        if self.first.is_none() {
            self.first = Some(error);
        }
        // Serde transports the terminal signal; the original typed cause stays here.
        io::ErrorKind::InvalidData.into()
    }
    fn check(&mut self) -> io::Result<()> {
        if self.first.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Err(e) = self.original.check() {
            return Err(self.terminal(KnowledgeQuestionError::Request(e)));
        }
        Ok(())
    }
}
impl io::Write for PromptWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        let Some(end) = self.bytes.len().checked_add(bytes.len()) else {
            return Err(self.terminal(KnowledgeQuestionError::Limit("prompt bytes")));
        };
        if end > self.limit {
            return Err(self.terminal(KnowledgeQuestionError::Limit("prompt bytes")));
        }
        if let Err(e) = self.bytes.try_reserve(bytes.len()) {
            return Err(self.terminal(KnowledgeQuestionError::Allocation(e)));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // All-or-error writes, including empty input. Never retry persistent cancellation.
        self.write(bytes).map(|_| ())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()
    }
}
const QUESTION_TEMPLATE: &str = "knowledge-question.md";
const QUESTION_DEFAULT: graphit_runtime::instructions::CapturedInstruction<'static> =
    graphit_runtime::instructions::CapturedInstruction {
        name: QUESTION_TEMPLATE,
        origin: graphit_runtime::instructions::InstructionOrigin::CompiledDefault,
        markdown: include_str!("../../../templates/agent-instructions/knowledge-question.md"),
    };

fn question_instruction_limits() -> graphit_runtime::instructions::InstructionLimits {
    use graphit_runtime::instructions::InstructionLimits;
    InstructionLimits {
        max_templates: NonZeroUsize::new(4).unwrap(),
        max_template_bytes: NonZeroUsize::new(65_536).unwrap(),
        max_values: NonZeroUsize::new(1).unwrap(),
        max_variable_bytes: NonZeroUsize::new(1).unwrap(),
        max_output_bytes: NonZeroUsize::new(65_536).unwrap(),
        fuel: std::num::NonZeroU64::new(10_000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}

pub(crate) fn capture_question_instruction(
    issuer: &graphit_auth::RoleIssuer,
    paths: &graphit_core::project_paths::ProjectPaths,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> io::Result<String> {
    crate::instruction_overrides::InstructionSources::capture_project(issuer, paths, original)?
        .render(
            QUESTION_TEMPLATE,
            &[QUESTION_DEFAULT],
            &[],
            question_instruction_limits(),
            original,
        )
        .map(|value| value.into_body())
        .map_err(io::Error::other)
}

fn encode_prompt(
    original: &SnapshotContext,
    query: &str,
    documents: &[KnowledgeDocument],
    limit: usize,
    instruction: &str,
) -> Result<String, KnowledgeQuestionError> {
    original.check().map_err(KnowledgeQuestionError::Request)?;
    let mut pages = Vec::new();
    pages
        .try_reserve_exact(documents.len())
        .map_err(KnowledgeQuestionError::Allocation)?;
    for document in documents {
        original.check().map_err(KnowledgeQuestionError::Request)?;
        pages.push(PromptPage {
            slug: &document.slug,
            title: &document.title,
            body: &document.body,
        });
    }
    let prompt = Prompt {
        instruction,
        question: query,
        pages,
    };
    let mut writer = PromptWriter {
        original,
        bytes: Vec::new(),
        limit,
        first: None,
    };
    if let Err(e) = serde_json::to_writer(&mut writer, &prompt) {
        return Err(writer
            .first
            .take()
            .unwrap_or(KnowledgeQuestionError::Encoding(e)));
    }
    original.check().map_err(KnowledgeQuestionError::Request)?;
    // Only serde_json writes this private buffer, so successful encoding is UTF-8.
    Ok(String::from_utf8(writer.bytes).expect("successful JSON encoding is UTF-8"))
}

/// Authorized data and prompt prepared once, before a caller can launch a child.
enum Preparation<R> {
    NoCorpus,
    PublishedEmpty { generation: KnowledgeGeneration },
    EmptySelection { generation: KnowledgeGeneration },
    Ready(Box<PreparedKnowledgeExecution<R>>),
}
struct PreparedKnowledgeExecution<R> {
    generation: KnowledgeGeneration,
    documents: Vec<KnowledgeDocument>,
    prompt: String,
    run: R,
}
pub struct PreparedKnowledgeQuestion<'n, 'r, 'p> {
    original: &'n Arc<SnapshotContext>,
    read: &'r ProjectReadAdmission<'p>,
    preparation: Preparation<CodexCompletionRun<'n>>,
}
pub(crate) struct PreparedDocumentaryQuestion<'n, 'r, 'p> {
    original: &'n Arc<SnapshotContext>,
    read: &'r ProjectReadAdmission<'p>,
    preparation: Preparation<()>,
}
/// Moving these observed fields drops bodies and summaries before native execution.
fn provided_heads(documents: Vec<KnowledgeDocument>) -> Vec<KnowledgeHead> {
    documents
        .into_iter()
        .map(|document| KnowledgeHead {
            slug: document.slug,
            title: document.title,
            updated_at: document.updated_at,
        })
        .collect()
}
pub(crate) enum DocumentaryQuestionContent {
    Empty(KnowledgeQuestionResult<'static>),
    Ready {
        generation: KnowledgeGeneration,
        documents: Vec<KnowledgeHead>,
        prompt: String,
    },
}
impl PreparedDocumentaryQuestion<'_, '_, '_> {
    pub(crate) fn documents(&self) -> Option<&[KnowledgeDocument]> {
        match &self.preparation {
            Preparation::Ready(ready) => Some(&ready.documents),
            _ => None,
        }
    }
    /// Recheck the same authorized read immediately before the caller runs the selected CLI.
    pub(crate) fn into_execution(
        self,
    ) -> Result<DocumentaryQuestionContent, KnowledgeQuestionError> {
        check_read(self.original, self.read)?;
        Ok(match self.preparation {
            Preparation::NoCorpus => {
                DocumentaryQuestionContent::Empty(KnowledgeQuestionResult::NoCorpus)
            }
            Preparation::PublishedEmpty { generation } => {
                DocumentaryQuestionContent::Empty(KnowledgeQuestionResult::PublishedEmpty {
                    generation,
                })
            }
            Preparation::EmptySelection { generation } => {
                DocumentaryQuestionContent::Empty(KnowledgeQuestionResult::EmptySelection {
                    generation,
                })
            }
            Preparation::Ready(ready) => {
                let PreparedKnowledgeExecution {
                    generation,
                    documents,
                    prompt,
                    ..
                } = *ready;
                DocumentaryQuestionContent::Ready {
                    generation,
                    documents: provided_heads(documents),
                    prompt,
                }
            }
        })
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "The question retains its authorized context, existing limits and rendered instruction"
)]
pub(crate) async fn prepare_documentary_question<'n, 'r, 'p>(
    original: &'n Arc<SnapshotContext>,
    read: &'r ProjectReadAdmission<'p>,
    query: &str,
    selected_slugs: &[&str],
    requested_model: &str,
    model_id_bytes: usize,
    limits: KnowledgeQuestionLimits,
    instruction: &str,
) -> Result<PreparedDocumentaryQuestion<'n, 'r, 'p>, KnowledgeQuestionError> {
    check_read(original, read)?;
    original
        .required_deadline()
        .map_err(KnowledgeQuestionError::Request)?;
    if requested_model.is_empty() || requested_model.len() > model_id_bytes {
        return Err(KnowledgeQuestionError::Limit("model id"));
    }
    let preparation =
        prepare_corpus(original, read, query, selected_slugs, limits, instruction).await?;
    Ok(PreparedDocumentaryQuestion {
        original,
        read,
        preparation,
    })
}
impl PreparedKnowledgeQuestion<'_, '_, '_> {
    /// Borrow metadata from the same authorized read without cloning bodies.
    pub fn documents(&self) -> Option<&[KnowledgeDocument]> {
        match &self.preparation {
            Preparation::Ready(ready) => Some(&ready.documents),
            _ => None,
        }
    }
}
async fn prepare_corpus(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    query: &str,
    selected_slugs: &[&str],
    limits: KnowledgeQuestionLimits,
    instruction: &str,
) -> Result<Preparation<()>, KnowledgeQuestionError> {
    check_read(original, read)?;
    if query.trim().is_empty() {
        return Err(KnowledgeQuestionError::InvalidQuestion);
    }
    if query.len() > limits.max_query_bytes.get() {
        return Err(KnowledgeQuestionError::Limit("query bytes"));
    }
    if selected_slugs.len() > limits.max_selected_pages.get() {
        return Err(KnowledgeQuestionError::Limit("selected pages"));
    }
    let mut selection = Vec::new();
    selection
        .try_reserve_exact(selected_slugs.len())
        .map_err(KnowledgeQuestionError::Allocation)?;
    selection.extend_from_slice(selected_slugs);
    selection.sort_unstable();
    if selection.iter().any(|s| s.is_empty()) || selection.windows(2).any(|s| s[0] == s[1]) {
        return Err(KnowledgeQuestionError::InvalidSelection);
    }
    let capture = match capture_native_knowledge_search(original, read, limits.read)
        .await
        .map_err(selection_error)?
    {
        KnowledgeSearchRead::NoCorpus => return Ok(Preparation::NoCorpus),
        KnowledgeSearchRead::Published(capture) => capture,
    };
    let selected = capture
        .read_selected_documents(original, read, &selection)
        .await
        .map_err(selection_error)?;
    selected_preparation(original, read, query, selected, limits, instruction)
}
fn selection_error(error: KnowledgeSearchError) -> KnowledgeQuestionError {
    match error {
        KnowledgeSearchError::InvalidQuery(_) => KnowledgeQuestionError::InvalidSelection,
        KnowledgeSearchError::Data(error) => KnowledgeQuestionError::Data(Box::new(error)),
        error => KnowledgeQuestionError::Search(Box::new(error)),
    }
}
fn selected_preparation(
    original: &Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    query: &str,
    selected: KnowledgeSelectedDocuments,
    limits: KnowledgeQuestionLimits,
    instruction: &str,
) -> Result<Preparation<()>, KnowledgeQuestionError> {
    check_read(original, read)?;
    if query.trim().is_empty() {
        return Err(KnowledgeQuestionError::InvalidQuestion);
    }
    if query.len() > limits.max_query_bytes.get() {
        return Err(KnowledgeQuestionError::Limit("query bytes"));
    }
    let KnowledgeSelectedDocuments {
        generation,
        state,
        documents,
    } = selected;
    if documents.len() > limits.max_selected_pages.get() {
        return Err(KnowledgeQuestionError::Limit("selected pages"));
    }
    match state {
        KnowledgeSelectedDocumentsState::PublishedEmpty => {
            return Ok(Preparation::PublishedEmpty { generation });
        }
        KnowledgeSelectedDocumentsState::EmptySelection => {
            return Ok(Preparation::EmptySelection { generation });
        }
        KnowledgeSelectedDocumentsState::Selected => (),
    }
    let prompt = encode_prompt(
        original,
        query,
        &documents,
        limits.max_prompt_bytes.get(),
        instruction,
    )?;
    check_read(original, read)?;
    Ok(Preparation::Ready(Box::new(PreparedKnowledgeExecution {
        generation,
        documents,
        prompt,
        run: (),
    })))
}
/// Consume only documents selected by the authorized search capture, without reopening latest.
pub(crate) fn prepare_selected_documentary_question<'n, 'r, 'p>(
    original: &'n Arc<SnapshotContext>,
    read: &'r ProjectReadAdmission<'p>,
    query: &str,
    selected: KnowledgeSelectedDocuments,
    limits: KnowledgeQuestionLimits,
    instruction: &str,
) -> Result<PreparedDocumentaryQuestion<'n, 'r, 'p>, KnowledgeQuestionError> {
    Ok(PreparedDocumentaryQuestion {
        original,
        read,
        preparation: selected_preparation(original, read, query, selected, limits, instruction)?,
    })
}
pub async fn prepare_knowledge_codex<'n, 'r, 'p>(
    original: &'n Arc<SnapshotContext>,
    read: &'r ProjectReadAdmission<'p>,
    query: &str,
    selected_slugs: &[&str],
    model: KnowledgeQuestionModel<'n>,
    limits: KnowledgeQuestionLimits,
) -> Result<PreparedKnowledgeQuestion<'n, 'r, 'p>, KnowledgeQuestionFailure<'n>> {
    check_read(original, read)?;
    if model.completion_limits.max_graphit_attempts != 1 {
        return Err(KnowledgeQuestionError::InvalidAttemptLimit.into());
    }
    let instruction = graphit_runtime::instructions::render_instruction(
        QUESTION_TEMPLATE,
        &[QUESTION_DEFAULT],
        &[],
        question_instruction_limits(),
        original,
    )
    .map_err(|error| KnowledgeQuestionError::Instruction(Box::new(error)))?;
    let prepared = PreparedDocumentaryQuestion {
        original,
        read,
        preparation: prepare_corpus(
            original,
            read,
            query,
            selected_slugs,
            limits,
            instruction.body(),
        )
        .await?,
    };
    prepared.into_codex(model)
}
impl<'n, 'r, 'p> PreparedDocumentaryQuestion<'n, 'r, 'p> {
    /// Add the native completion policy to data already read from its authorized pin.
    pub(crate) fn into_codex(
        self,
        model: KnowledgeQuestionModel<'n>,
    ) -> Result<PreparedKnowledgeQuestion<'n, 'r, 'p>, KnowledgeQuestionFailure<'n>> {
        check_read(self.original, self.read)?;
        if model.completion_limits.max_graphit_attempts != 1 {
            return Err(KnowledgeQuestionError::InvalidAttemptLimit.into());
        }
        let preparation = match self.preparation {
            Preparation::NoCorpus => Preparation::NoCorpus,
            Preparation::PublishedEmpty { generation } => {
                Preparation::PublishedEmpty { generation }
            }
            Preparation::EmptySelection { generation } => {
                Preparation::EmptySelection { generation }
            }
            Preparation::Ready(ready) => {
                let PreparedKnowledgeExecution {
                    generation,
                    documents,
                    prompt,
                    ..
                } = *ready;
                let run = CodexCompletionRun::new(
                    self.original,
                    Some(model.requested_model),
                    model.wire_limits,
                    model.completion_limits,
                )
                .map_err(|e| KnowledgeQuestionError::Completion(Box::new(e)))?;
                Preparation::Ready(Box::new(PreparedKnowledgeExecution {
                    generation,
                    documents,
                    prompt,
                    run,
                }))
            }
        };
        Ok(PreparedKnowledgeQuestion {
            original: self.original,
            read: self.read,
            preparation,
        })
    }
}
impl<'n> PreparedKnowledgeQuestion<'n, '_, '_> {
    /// Finish an observed empty state without installing a placeholder transport.
    pub(crate) fn complete_without_execution(
        self,
    ) -> Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>> {
        check_read(self.original, self.read)?;
        match self.preparation {
            Preparation::NoCorpus => Ok(KnowledgeQuestionResult::NoCorpus),
            Preparation::PublishedEmpty { generation } => {
                Ok(KnowledgeQuestionResult::PublishedEmpty { generation })
            }
            Preparation::EmptySelection { generation } => {
                Ok(KnowledgeQuestionResult::EmptySelection { generation })
            }
            Preparation::Ready(_) => Err(KnowledgeQuestionError::ExecutionRequired.into()),
        }
    }
    pub async fn execute<F, Fut>(
        self,
        complete: F,
    ) -> Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>>
    where
        F: FnOnce(CodexCompletionRun<'n>, String) -> Fut,
        Fut: Future<Output = Result<CodexReceipt<'n>, CodexFailure<'n>>>,
    {
        let Self {
            original,
            read,
            preparation,
        } = self;
        check_read(original, read)?;
        let PreparedKnowledgeExecution {
            generation,
            documents,
            prompt,
            run,
        } = match preparation {
            Preparation::NoCorpus => return Ok(KnowledgeQuestionResult::NoCorpus),
            Preparation::PublishedEmpty { generation } => {
                return Ok(KnowledgeQuestionResult::PublishedEmpty { generation });
            }
            Preparation::EmptySelection { generation } => {
                return Ok(KnowledgeQuestionResult::EmptySelection { generation });
            }
            Preparation::Ready(ready) => *ready,
        };
        let documents = provided_heads(documents);
        let execution = complete(run, prompt).await;
        if let Err(error) = check_read(original, read) {
            return Err(KnowledgeQuestionFailure {
                error,
                execution: Some(Box::new(execution)),
            });
        }
        let receipt = match execution {
            Ok(receipt) => receipt,
            Err(error) => {
                return Err(KnowledgeQuestionFailure {
                    error: KnowledgeQuestionError::Codex,
                    execution: Some(Box::new(Err(error))),
                });
            }
        };
        let disposition = match receipt.completion() {
            Some(CompletionOutcome::Answer(answer)) if answer.text().trim().is_empty() => {
                KnowledgeQuestionDisposition::EmptyAnswer
            }
            Some(CompletionOutcome::Answer(_)) => KnowledgeQuestionDisposition::Answer,
            Some(CompletionOutcome::EvidenceUnavailable(_)) => {
                KnowledgeQuestionDisposition::EvidenceUnavailable
            }
            Some(CompletionOutcome::Failed(_)) => KnowledgeQuestionDisposition::Failed,
            None => KnowledgeQuestionDisposition::NoCompletionObservation,
        };
        Ok(KnowledgeQuestionResult::Executed {
            generation,
            provided_pages: documents,
            receipt: Box::new(receipt),
            disposition,
        })
    }
}

/// The stateless path uses the same preparation and execution, without re-reading Data.
pub async fn ask_knowledge_codex<'n, F, Fut>(
    original: &'n Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    query: &str,
    selected_slugs: &[&str],
    model: KnowledgeQuestionModel<'n>,
    limits: KnowledgeQuestionLimits,
    complete: F,
) -> Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>>
where
    F: FnOnce(CodexCompletionRun<'n>, String) -> Fut,
    Fut: Future<Output = Result<CodexReceipt<'n>, CodexFailure<'n>>>,
{
    prepare_knowledge_codex(original, read, query, selected_slugs, model, limits)
        .await?
        .execute(complete)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::{
        io::Write,
        time::{Duration, Instant},
    };
    fn original() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(10),
            Cancellation::default(),
        )
    }
    #[test]
    fn serde_cancel_and_empty_write_preserve_original_cause_without_retry() {
        let original = original();
        original.cancellation().cancel();
        let mut writer = PromptWriter {
            original: &original,
            bytes: Vec::new(),
            limit: 100,
            first: None,
        };
        assert!(serde_json::to_writer(&mut writer, &"literal").is_err());
        assert!(
            matches!(writer.first, Some(KnowledgeQuestionError::Request(ref e)) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert!(writer.bytes.is_empty());
        assert!(writer.write_all(&[]).is_err());
        assert!(writer.flush().is_err());
    }
    #[test]
    fn serde_escaped_bytes_use_exact_limit_and_first_failure_is_sticky() {
        let original = original();
        let text = "a\n\"β";
        let expected = b"\"a\\n\\\"\xce\xb2\"";
        let mut writer = PromptWriter {
            original: &original,
            bytes: Vec::new(),
            limit: expected.len(),
            first: None,
        };
        serde_json::to_writer(&mut writer, &text).unwrap();
        assert_eq!(writer.bytes, expected);
        let mut writer = PromptWriter {
            original: &original,
            bytes: Vec::new(),
            limit: expected.len() - 1,
            first: None,
        };
        assert!(serde_json::to_writer(&mut writer, &text).is_err());
        original.cancellation().cancel();
        assert!(writer.flush().is_err());
        assert!(matches!(
            writer.first,
            Some(KnowledgeQuestionError::Limit("prompt bytes"))
        ));
    }
}
