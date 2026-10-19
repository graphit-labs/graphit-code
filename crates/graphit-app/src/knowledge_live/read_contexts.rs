//! Persisted read identities are data; fresh SDK captures and authority belong to Source.
use super::{Access, ArtifactSelection, LiveError, LiveSessionService, Session, SessionState};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadContext {
    pub selection: ArtifactSelection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadContexts {
    pub selected: Vec<ReadContext>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadContextsSnapshot {
    pub session: Session,
    pub contexts: ReadContexts,
    pub last_prompt: Option<String>,
}
impl ReadContexts {
    pub(super) fn validate(&self, session: &Session) -> io::Result<()> {
        let selected = session
            .artifacts
            .iter()
            .filter(|item| matches!(item.artifact_type.as_str(), "knowledge" | "ast"));
        let mut actual = self.selected.iter();
        for expected in selected {
            let context = actual.next().ok_or(io::ErrorKind::InvalidData)?;
            if &context.selection != expected {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if actual.next().is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}
impl LiveSessionService {
    /// One admitted journal read binds the session generation and its prepared read identities.
    pub(crate) fn read_contexts(
        &self,
        access: &Access<'_>,
        id: &str,
        generation: &str,
    ) -> Result<ReadContextsSnapshot, LiveError> {
        access.read()?;
        self.identifier(id)?;
        self.identifier(generation)?;
        let stored = self
            .journal(access)?
            .stored(id, access.original().snapshot(), || access.read())?;
        if stored.session.cursor.generation != generation {
            return Err(LiveError::Conflict);
        }
        // A persisted stamp is readable after a run, failure or explicit Close as well as Ready.
        let last_prompt = stored
            .events
            .iter()
            .rev()
            .find_map(|event| {
                matches!(event.data, super::EventData::Prompt { .. })
                    .then_some(event.turn_id.clone())
            })
            .flatten();
        let contexts = stored.read_contexts.ok_or(LiveError::NotReady)?;
        contexts.validate(&stored.session)?;
        if stored.session.state == SessionState::Preparing {
            return Err(LiveError::NotReady);
        }
        access.read()?;
        Ok(ReadContextsSnapshot {
            session: stored.session,
            contexts,
            last_prompt,
        })
    }
}
