//! Finite PROJECT Memory browser reads over the existing authenticated request.
use crate::task_server::browser::codec::{self, AdmittedSelection, Body, encode};
use crate::task_server::{TaskPolicy, request::Arguments};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_hub::native::ContentLimits;
use graphit_records::{
    memory_attributes::MemoryClass,
    memory_store::{
        Memory, MemoryGeneration, MemoryId, MemoryListFilter, MemoryListRead, MemoryStoreError,
        MemoryStoreReadLimits, NativeMemoryStore,
    },
};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, sync::Arc};

pub(crate) mod mutation;
pub(crate) mod search;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Class {
    Fact,
    Skill,
    Tension,
    Decision,
    Convention,
    Correction,
}
impl Class {
    pub(crate) fn native(self) -> MemoryClass {
        match self {
            Self::Fact => MemoryClass::Fact,
            Self::Skill => MemoryClass::Skill,
            Self::Tension => MemoryClass::Tension,
            Self::Decision => MemoryClass::Decision,
            Self::Convention => MemoryClass::Convention,
            Self::Correction => MemoryClass::Correction,
        }
    }
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter {
    classification: Option<Class>,
    tag: Option<String>,
    mandatory: Option<bool>,
    important: Option<bool>,
    #[serde(default)]
    exclude_mandatory: bool,
}
impl Filter {
    pub(crate) fn validate(&self, limits: ContentLimits) -> Result<(), &'static str> {
        if self
            .tag
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.len() > limits.max_scalar_bytes)
        {
            return Err("invalid_arguments");
        }
        Ok(())
    }
    pub(crate) fn into_native(self) -> MemoryListFilter {
        MemoryListFilter {
            classification: self.classification.map(Class::native),
            tag: self.tag,
            mandatory: self.mandatory,
            important: self.important,
            exclude_mandatory: self.exclude_mandatory,
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) id: MemoryId,
}
#[derive(Deserialize, Serialize)]
pub(crate) enum Query {
    List(Filter),
    Get(Record),
    History(Record),
    Mutation(Box<mutation::Input>),
    Search(search::Input),
}

pub(crate) fn parse(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    let (selection, query) = match operation {
        "memory_list" => {
            let body: Body<Filter> = codec::parse_body(bytes, limits)?;
            body.input.validate(limits)?;
            (body.selection, Query::List(body.input))
        }
        "memory_get" => {
            let body: Body<Record> = codec::parse_body(bytes, limits)?;
            (body.selection, Query::Get(body.input))
        }
        "memory_history" => {
            let body: Body<Record> = codec::parse_body(bytes, limits)?;
            (body.selection, Query::History(body.input))
        }
        "memory_create" | "memory_update" | "memory_delete" => {
            return mutation::parse_project(bytes, limits, operation);
        }
        "memory_search" => {
            let body: Body<search::Input> = codec::parse_body(bytes, limits)?;
            body.input.validate(limits)?;
            (body.selection, Query::Search(body.input))
        }
        _ => return Err("invalid_arguments"),
    };
    codec::arguments(selection, query)
}

pub(crate) fn limits(policy: &TaskPolicy) -> Result<MemoryStoreReadLimits, MemoryStoreError> {
    let positive =
        |v| NonZeroUsize::new(v).ok_or(MemoryStoreError::Invalid("invalid Memory read policy"));
    Ok(MemoryStoreReadLimits {
        native: policy.read.native,
        batch_rows: policy.read.batch_rows,
        max_query_bytes: positive(policy.content.max_input_bytes)?,
    })
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Catalogue {
    NoCorpus,
    Published {
        generation: MemoryGeneration,
        memories: Vec<Memory>,
    },
}
pub(crate) async fn read(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    policy: &TaskPolicy,
    selection: &AdmittedSelection,
    query: Query,
) -> Result<Vec<u8>, MemoryStoreError> {
    let limits = limits(policy)?;
    let maximum = policy.content.max_output_bytes;
    let output = match query {
        Query::List(filter) => {
            let native =
                match NativeMemoryStore::list(original, admission, filter.into_native(), limits)
                    .await?
                {
                    MemoryListRead::NoCorpus => Catalogue::NoCorpus,
                    MemoryListRead::Published {
                        generation,
                        memories,
                    } => Catalogue::Published {
                        generation,
                        memories,
                    },
                };
            encode(selection, &native, maximum)?
        }
        Query::Get(record) => encode(
            selection,
            &NativeMemoryStore::get(original, admission, record.id, limits).await?,
            maximum,
        )?,
        Query::History(record) => encode(
            selection,
            &NativeMemoryStore::history(original, admission, record.id, limits).await?,
            maximum,
        )?,
        Query::Mutation(_) => return Err(MemoryStoreError::WrongAdmission),
        Query::Search(_) => return Err(MemoryStoreError::WrongAdmission),
    };
    original.check()?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::native_path::NativePath;
    #[test]
    fn direct_serializer_accepts_exact_cap_rejects_first_extra_and_preserves_u64() {
        let selection = AdmittedSelection {
            project_dir: NativePath::new(std::env::temp_dir()).unwrap(),
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            project_version: "main".into(),
        };
        let native = MemoryGeneration {
            sdk_version: 9007199254740993,
            project_id: selection.project_id,
            project_version: "main".into(),
            physical_branch: None,
            inherited_target: None,
        };
        let expected = serde_json::to_vec(&codec::Response {
            selection: &selection,
            native: &native,
        })
        .unwrap();
        assert_eq!(
            encode(&selection, &native, expected.len()).unwrap(),
            expected
        );
        assert!(
            String::from_utf8(expected.clone())
                .unwrap()
                .contains("9007199254740993")
        );
        let error = encode(&selection, &native, expected.len() - 1).unwrap_err();
        assert_eq!(
            error.io_error_kind(),
            Some(std::io::ErrorKind::FileTooLarge)
        );
    }
}
