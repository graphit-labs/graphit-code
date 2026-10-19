use super::TransportToken;
use crate::task_store::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "operation", content = "result", rename_all = "snake_case")]
pub enum ReadResponse {
    GetTask(Option<Box<TaskGetRead>>),
    GetSession(Option<Box<SelectedSessionRead>>),
    Catalogue(CataloguePage),
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    Search(TaskLexicalSearchRead),
    Query(TaskQueryRead),
    Schema(TaskQuerySchema),
    ReadView(TaskReadView),
    SessionHeads(Vec<SessionHead>),
}
/// A claim response belongs only to the participant; ordinary read models remain unchanged.
#[derive(Serialize)]
pub struct ParticipantClaim<T> {
    pub record: T,
    pub token: TransportToken,
}
impl<T> From<Claim<T>> for ParticipantClaim<T> {
    fn from(claim: Claim<T>) -> Self {
        Self {
            record: claim.record,
            token: TransportToken::from_claim(claim.token),
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum WriteResponse {
    Comment(Box<TaskCommentOutcome>),
    Task(Task),
    Session(Session),
    TaskClaim(ParticipantClaim<Task>),
    SessionClaim(ParticipantClaim<Session>),
}
