//! Append-only Task comments with native identity and server-derived attribution.
use crate::{task_spec::TaskId, task_store::SessionId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::num::{NonZeroU64, NonZeroU128};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskCommentId(NonZeroU128);
impl TaskCommentId {
    pub const fn new(value: NonZeroU128) -> Self {
        Self(value)
    }
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }
}
impl Serialize for TaskCommentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&ulid::Ulid::from(self.0.get()))
    }
}
impl<'de> Deserialize<'de> for TaskCommentId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::task_spec::deserialize_identifier(deserializer).map(Self)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCommentKind {
    Note,
    Decision,
    Problem,
    Lesson,
    Knowledge,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskComment {
    pub id: TaskCommentId,
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub idempotency_key: String,
    pub sequence: NonZeroU64,
    pub kind: TaskCommentKind,
    pub body: String,
    pub actor: String,
    pub at: u64,
    pub revision: u64,
}
impl TaskComment {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.idempotency_key.is_empty() || self.idempotency_key.trim() != self.idempotency_key {
            return Err("comment idempotency key");
        }
        if self.body.trim().is_empty() {
            return Err("blank comment body");
        }
        if self.actor.len() != 64
            || !self
                .actor
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("comment actor");
        }
        if self.revision < 2 {
            return Err("comment revision");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_native_ids_closed_kinds_and_literal_body_roundtrip() {
        let comment = TaskComment {
            id: TaskCommentId::new(NonZeroU128::new(1).unwrap()),
            task_id: TaskId::new(NonZeroU128::new(2).unwrap()),
            session_id: SessionId::new(NonZeroU128::new(3).unwrap()),
            idempotency_key: "key".into(),
            sequence: NonZeroU64::new(1).unwrap(),
            kind: TaskCommentKind::Knowledge,
            body: "  **literal**\n\n    code\n".into(),
            actor: "a".repeat(64),
            at: 10,
            revision: 3,
        };
        comment.validate().unwrap();
        let value = serde_json::to_value(&comment).unwrap();
        assert_eq!(value["id"].as_str().unwrap().len(), 26);
        assert_eq!(
            serde_json::from_value::<TaskComment>(value.clone()).unwrap(),
            comment
        );
        for bad in [
            "00000000000000000000000000",
            "tsk-Go",
            "0000000000000000000000000a",
        ] {
            assert!(serde_json::from_value::<TaskCommentId>(serde_json::json!(bad)).is_err());
        }
        let mut extra = value.clone();
        extra["token"] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<TaskComment>(extra).is_err());
        let mut unknown = value;
        unknown["kind"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<TaskComment>(unknown).is_err());
    }
}
