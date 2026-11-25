use graphit_core::file_io::SnapshotContext;
use graphit_records::memory_store::{
    DeleteMemory, FoldMemories, Memory, MemoryFoldKind, MemoryFoldMember, MemoryId,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    num::NonZeroUsize,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    actions: Vec<ProposedAction>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    id: MemoryId,
    expected_revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FoldKind {
    Merge,
    Conflict,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum ProposedAction {
    Fold {
        kind: FoldKind,
        survivor: Member,
        members: Vec<Member>,
        title: Option<String>,
        body: String,
        reason: String,
    },
    Delete {
        id: MemoryId,
        expected_revision: u64,
        reason: String,
    },
}

pub(crate) enum Operation {
    Fold(FoldMemories),
    Delete(MemoryId, DeleteMemory),
}

/// Validated data, never an authorization or a receipt of a durable effect.
pub struct PreparedRecipe {
    pub(crate) operations: Vec<Operation>,
    pub(crate) maximum: NonZeroUsize,
}

impl PreparedRecipe {
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

pub enum RecipeError {
    Json(serde_json::Error),
    Request(std::io::Error),
    Invalid(&'static str),
}

impl fmt::Debug for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(_) => f.write_str("MalformedRecipe"),
            Self::Request(error) => f.debug_tuple("RecipeRequest").field(&error.kind()).finish(),
            Self::Invalid(reason) => f.debug_tuple("InvalidRecipe").field(reason).finish(),
        }
    }
}
impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Dream proposal rejected")
    }
}
impl std::error::Error for RecipeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Request(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

fn text(value: &str) -> Result<(), RecipeError> {
    if value.trim().is_empty() {
        Err(RecipeError::Invalid("empty content"))
    } else {
        Ok(())
    }
}

fn member(
    input: Member,
    corpus: &HashMap<MemoryId, u64>,
    used: &mut HashSet<MemoryId>,
    key: String,
) -> Result<MemoryFoldMember, RecipeError> {
    if input.expected_revision == 0
        || corpus.get(&input.id) != Some(&input.expected_revision)
        || !used.insert(input.id)
    {
        return Err(RecipeError::Invalid("unknown, repeated or stale memory"));
    }
    Ok(MemoryFoldMember {
        id: input.id,
        expected_revision: input.expected_revision,
        idempotency_key: key,
    })
}

fn input_bytes(parts: impl Iterator<Item = usize>, maximum: usize) -> Result<(), RecipeError> {
    let mut total = 0usize;
    for bytes in parts {
        total = total
            .checked_add(bytes)
            .filter(|&value| value <= maximum)
            .ok_or(RecipeError::Invalid("input too large"))?;
    }
    Ok(())
}

/// Validate the complete proposal against the admitted corpus before any effect.
/// Operation keys originate in the caller's real attempt, not in model output.
pub fn prepare(
    json: &str,
    memories: &[Memory],
    operation_prefix: &str,
    maximum: NonZeroUsize,
    original: &SnapshotContext,
) -> Result<PreparedRecipe, RecipeError> {
    original.check().map_err(RecipeError::Request)?;
    text(operation_prefix)?;
    if json.len() > maximum.get() || operation_prefix.len() > maximum.get() {
        return Err(RecipeError::Invalid("input too large"));
    }
    let proposal: Proposal = serde_json::from_str(json).map_err(RecipeError::Json)?;
    if proposal.actions.len() > memories.len() {
        return Err(RecipeError::Invalid("too many actions"));
    }
    let corpus: HashMap<_, _> = memories.iter().map(|m| (m.id, m.revision)).collect();
    let mut used = HashSet::new();
    let mut operations = Vec::with_capacity(proposal.actions.len());
    for (ordinal, action) in proposal.actions.into_iter().enumerate() {
        original.check().map_err(RecipeError::Request)?;
        let operation = match action {
            ProposedAction::Fold {
                kind,
                survivor,
                members,
                title,
                body,
                reason,
            } => {
                if members.is_empty() {
                    return Err(RecipeError::Invalid("empty fold"));
                }
                text(&body)?;
                text(&reason)?;
                if let Some(title) = &title {
                    text(title)?;
                }
                let survivor = member(
                    survivor,
                    &corpus,
                    &mut used,
                    format!("{operation_prefix}:{ordinal}:survivor"),
                )?;
                let mut removed = Vec::with_capacity(members.len());
                for (index, input) in members.into_iter().enumerate() {
                    removed.push(member(
                        input,
                        &corpus,
                        &mut used,
                        format!("{operation_prefix}:{ordinal}:{index}"),
                    )?);
                }
                input_bytes(
                    [
                        body.len(),
                        reason.len(),
                        title.as_ref().map_or(0, String::len),
                    ]
                    .into_iter()
                    .chain(
                        std::iter::once(&survivor)
                            .chain(&removed)
                            .flat_map(|m| [m.idempotency_key.len(), 34]),
                    ),
                    maximum.get(),
                )?;
                Operation::Fold(FoldMemories {
                    kind: match kind {
                        FoldKind::Merge => MemoryFoldKind::Merge,
                        FoldKind::Conflict => MemoryFoldKind::Conflict,
                    },
                    survivor,
                    members: removed,
                    title,
                    body,
                    reason,
                })
            }
            ProposedAction::Delete {
                id,
                expected_revision,
                reason,
            } => {
                text(&reason)?;
                let observed = member(
                    Member {
                        id,
                        expected_revision,
                    },
                    &corpus,
                    &mut used,
                    format!("{operation_prefix}:{ordinal}:delete"),
                )?;
                input_bytes(
                    [reason.len(), observed.idempotency_key.len(), 34].into_iter(),
                    maximum.get(),
                )?;
                Operation::Delete(
                    id,
                    DeleteMemory {
                        expected_revision,
                        idempotency_key: observed.idempotency_key,
                        reason,
                    },
                )
            }
        };
        operations.push(operation);
    }
    original.check().map_err(RecipeError::Request)?;
    Ok(PreparedRecipe {
        operations,
        maximum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use graphit_records::memory_attributes::{MemoryAttributes, MemoryClass};
    use std::time::{Duration, Instant};

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }
    fn memory(id: &str) -> Memory {
        Memory {
            id: id.parse().unwrap(),
            revision: 3,
            title: "Observed title".into(),
            body: "Observed fact".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
            tags: Vec::new(),
            references: Vec::new(),
            reference_generation: 0,
            created_by: "a".repeat(64),
            updated_by: "a".repeat(64),
            created_at: 1,
            updated_at: 1,
        }
    }
    fn corpus() -> Vec<Memory> {
        vec![
            memory("01KSH1CRFFG8Z74B5ZS78WW808"),
            memory("01KSH1CRFFG8Z74B5ZS78WW809"),
        ]
    }
    fn deletion(id: MemoryId, revision: u64) -> serde_json::Value {
        serde_json::json!({"action":"delete","id":id,"expected_revision":revision,"reason":"Observed duplicate"})
    }
    fn parse(value: serde_json::Value, memories: &[Memory]) -> Result<PreparedRecipe, RecipeError> {
        prepare(
            &value.to_string(),
            memories,
            "actual-attempt",
            NonZeroUsize::new(8192).unwrap(),
            &context(),
        )
    }
    #[test]
    fn strict_plan_validation_rejects_later_foreign_stale_duplicate_and_unknown_inputs() {
        let corpus = corpus();
        let valid = deletion(corpus[0].id, 3);
        let foreign = deletion("01KSH1CRFFG8Z74B5ZS78WW810".parse().unwrap(), 3);
        for invalid in [
            foreign,
            deletion(corpus[1].id, 2),
            deletion(corpus[1].id, 0),
            valid.clone(),
        ] {
            assert!(
                parse(
                    serde_json::json!({"actions":[valid.clone(),invalid]}),
                    &corpus
                )
                .is_err()
            );
        }
        assert!(
            parse(
                serde_json::json!({"actions":[],"grant":"untrusted"}),
                &corpus
            )
            .is_err()
        );
        let mut unknown = valid.clone();
        unknown["idempotency_key"] = "model-key".into();
        assert!(parse(serde_json::json!({"actions":[unknown]}), &corpus).is_err());
        for malformed in [
            r#"{"actions":[],"actions":[]}"#,
            r#"{"actions":[]}tail"#,
            r#"{"actions":[{"action":"delete","id":"01KSH1CRFFG8Z74B5ZS78WW808","expected_revision":3,"expected_revision":3,"reason":"x"}]}"#,
        ] {
            assert!(
                prepare(
                    malformed,
                    &corpus,
                    "actual-attempt",
                    NonZeroUsize::new(8192).unwrap(),
                    &context()
                )
                .is_err()
            );
        }
        assert!(parse(serde_json::json!({"actions":[valid]}), &corpus).is_ok());
    }
    #[test]
    fn fold_uses_exact_observations_and_runtime_keys_and_empty_recipe_is_valid() {
        let corpus = corpus();
        let proposal = serde_json::json!({"actions":[{"action":"fold","kind":"conflict",
            "survivor":{"id":corpus[0].id,"expected_revision":3},
            "members":[{"id":corpus[1].id,"expected_revision":3}],
            "body":"Confirmed incompatible facts retained", "reason":"Evidence disagrees"}]});
        let parsed = parse(proposal.clone(), &corpus).unwrap();
        let Operation::Fold(input) = &parsed.operations[0] else {
            panic!("fold")
        };
        assert_eq!(input.kind, MemoryFoldKind::Conflict);
        assert_eq!(input.survivor.idempotency_key, "actual-attempt:0:survivor");
        assert_eq!(input.members[0].idempotency_key, "actual-attempt:0:0");
        for member in [
            serde_json::json!([]),
            serde_json::json!([{"id":corpus[0].id,"expected_revision":3}]),
        ] {
            let mut invalid = proposal.clone();
            invalid["actions"][0]["members"] = member;
            assert!(parse(invalid, &corpus).is_err());
        }
        assert!(
            parse(serde_json::json!({"actions":[]}), &[])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn original_and_complete_input_bounds_are_preserved() {
        let original = context();
        original.cancellation().cancel();
        assert!(matches!(
            prepare(
                r#"{"actions":[]}"#,
                &[],
                "attempt",
                NonZeroUsize::new(8192).unwrap(),
                &original
            ),
            Err(RecipeError::Request(_))
        ));
        assert!(
            prepare(
                r#"{"actions":[]}"#,
                &[],
                "attempt",
                NonZeroUsize::new(1).unwrap(),
                &context()
            )
            .is_err()
        );
        let corpus = corpus();
        let value = serde_json::json!({"actions":[deletion(corpus[0].id, 3)]}).to_string();
        assert!(
            prepare(
                &value,
                &corpus,
                &"x".repeat(4096),
                NonZeroUsize::new(4096).unwrap(),
                &context()
            )
            .is_err()
        );
    }
}
