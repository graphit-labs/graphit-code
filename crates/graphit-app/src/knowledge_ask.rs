//! Authenticated PROJECT question using the existing Codex and Process owners.
#![cfg(target_os = "linux")]
use crate::knowledge_question::{
    KnowledgeQuestionFailure, KnowledgeQuestionLimits, KnowledgeQuestionModel,
    KnowledgeQuestionResult, ask_knowledge_codex,
};
use graphit_ai::{
    cli_codex::{CodexSink, CodexStreamLimits, run_codex},
    codex_completion::AttemptPurpose,
};
use graphit_auth::{
    AuthError, AuthenticatedCaller, ProjectReadModule, ProjectRecordAuthority, ScopeInput,
};
use graphit_hub::policy::RequestContext;
use graphit_process::{ProcessHost, Request};
use std::{error::Error, fmt};

/// Borrow the actual issuer selector, authenticated caller and original captures.
pub struct AuthenticatedKnowledgeQuestion<'a, 'i> {
    pub authority: &'a ProjectRecordAuthority<'i>,
    pub caller: &'a AuthenticatedCaller<'a>,
    pub scope: ScopeInput<'a>,
    pub original: &'a RequestContext<'a>,
}

/// The requested model is caller evidence, rather than a native model confirmation.
pub struct KnowledgeCodexQuestion<'n, 'q> {
    pub query: &'q str,
    pub selected_slugs: &'q [&'q str],
    pub model: KnowledgeQuestionModel<'n>,
    pub limits: KnowledgeQuestionLimits,
}

/// The service owner captures the request and retains the host and borrowed sink.
/// `request.stdin` must be absent; only the authorized prompt supplies it.
pub struct CodexQuestionExecution<'a, S> {
    pub host: &'a ProcessHost,
    pub request: Request,
    pub stream_limits: CodexStreamLimits,
    pub sink: &'a mut S,
}

pub enum KnowledgeAskFailure<'n> {
    Auth(AuthError),
    InputAlreadyPresent,
    Question(KnowledgeQuestionFailure<'n>),
}
impl<'n> KnowledgeAskFailure<'n> {
    pub fn auth(&self) -> Option<&AuthError> {
        match self {
            Self::Auth(error) => Some(error),
            _ => None,
        }
    }
    pub fn question(&self) -> Option<&KnowledgeQuestionFailure<'n>> {
        match self {
            Self::Question(error) => Some(error),
            _ => None,
        }
    }
    pub fn question_mut(&mut self) -> Option<&mut KnowledgeQuestionFailure<'n>> {
        match self {
            Self::Question(error) => Some(error),
            _ => None,
        }
    }
    fn category(&self) -> &'static str {
        match self {
            Self::Auth(_) => "authentication or permission",
            Self::InputAlreadyPresent => "stdin already present",
            Self::Question(_) => "question execution",
        }
    }
}
impl fmt::Debug for KnowledgeAskFailure<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.category())
    }
}
impl fmt::Display for KnowledgeAskFailure<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "knowledge ask: {}", self.category())
    }
}
impl Error for KnowledgeAskFailure<'_> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Auth(error) => Some(error),
            Self::Question(error) => error.source(),
            Self::InputAlreadyPresent => None,
        }
    }
}

/// Admit the original caller, then run the real transport exactly once if needed.
/// Results and failures retain the existing receipt and any remaining Process owner.
pub async fn ask_knowledge_codex_authenticated<'n, S: CodexSink>(
    auth: AuthenticatedKnowledgeQuestion<'n, '_>,
    question: KnowledgeCodexQuestion<'n, '_>,
    execution: CodexQuestionExecution<'_, S>,
) -> Result<KnowledgeQuestionResult<'n>, KnowledgeAskFailure<'n>> {
    let admission = auth
        .authority
        .admit_authenticated_read(
            auth.caller,
            auth.original,
            auth.scope,
            ProjectReadModule::Knowledge,
        )
        .map_err(KnowledgeAskFailure::Auth)?;
    if execution.request.stdin.is_some() {
        return Err(KnowledgeAskFailure::InputAlreadyPresent);
    }
    let CodexQuestionExecution {
        host,
        mut request,
        stream_limits,
        sink,
    } = execution;
    ask_knowledge_codex(
        auth.original.snapshot(),
        &admission,
        question.query,
        question.selected_slugs,
        question.model,
        question.limits,
        |mut run, prompt| async move {
            request.stdin = Some(prompt.into_bytes().into());
            run_codex(
                &mut run,
                host,
                request,
                AttemptPurpose::Request,
                None,
                stream_limits,
                sink,
            )
            .await
        },
    )
    .await
    .map_err(KnowledgeAskFailure::Question)
}
