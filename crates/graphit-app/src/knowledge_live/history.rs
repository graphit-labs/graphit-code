//! The same admitted history operations serve Runtime and USER transports.
use super::{Access, LiveError, LiveSessionService, ReplayCursor};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HistoryOperation {
    List,
    Get {
        id: String,
    },
    Replay {
        id: String,
        cursor: Option<ReplayCursor>,
    },
    Cancel {
        id: String,
    },
    Close {
        id: String,
    },
    Delete {
        id: String,
    },
}
pub(crate) struct HistoryResponse {
    pub value: Value,
    pub effect: graphit_core::native_document_file::NativeDocumentJournalEffect,
}
impl HistoryResponse {
    pub(crate) fn recheck(&self, access: &Access<'_>) -> Result<(), LiveError> {
        access
            .read()
            .map_err(|cause| LiveError::after_effect(cause, self.effect))
    }
}
/// Current Auth is required before dispatch; no owner, credential or scope comes from a DTO.
pub(crate) fn dispatch(
    live: &LiveSessionService,
    access: &Access<'_>,
    input: HistoryOperation,
) -> Result<HistoryResponse, LiveError> {
    use graphit_core::native_document_file::NativeDocumentJournalEffect as Effect;
    access.read()?;
    let (value, effect) = match input {
        HistoryOperation::List => (json!({"sessions":live.list(access)?}), Effect::Unchanged),
        HistoryOperation::Get { id } | HistoryOperation::Replay { id, cursor: None } => {
            let observation = live.observe(access, &id, None)?;
            (
                json!({"session":observation.replay.session,"events":observation.replay.events,"terminal":observation.terminal}),
                Effect::Unchanged,
            )
        }
        HistoryOperation::Replay { id, cursor } => {
            let observation = live.observe(access, &id, cursor.as_ref())?;
            (
                json!({"session":observation.replay.session,"events":observation.replay.events,"terminal":observation.terminal}),
                Effect::Unchanged,
            )
        }
        HistoryOperation::Cancel { id } => {
            let (requested, effect) = live.cancel_with_effect(access, &id)?;
            (json!({"requested":requested}), effect)
        }
        HistoryOperation::Close { id } => {
            let (session, effect) = live.close_with_effect(access, &id)?;
            (json!({"session":session}), effect)
        }
        HistoryOperation::Delete { id } => {
            let effect = live.delete_with_effect(access, &id)?;
            (json!({"deleted":true}), effect)
        }
    };
    let response = HistoryResponse { value, effect };
    response.recheck(access)?;
    Ok(response)
}

impl HistoryOperation {
    pub(crate) fn mutates(&self) -> bool {
        matches!(
            self,
            Self::Cancel { .. } | Self::Close { .. } | Self::Delete { .. }
        )
    }
}
