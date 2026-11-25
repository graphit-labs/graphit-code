use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::project_identity::ProjectId;
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, ResourceIdentity, policy::HubResourcePolicy,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, Path},
};

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    List,
    Review,
    Content,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct List {
    pub project_id: ProjectId,
    pub project_version: String,
    #[serde(default, deserialize_with = "non_null")]
    pub artifact_type: Option<ArtifactType>,
    #[serde(default, deserialize_with = "non_null")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    pub cursor: Option<String>,
    pub limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    pub project_id: ProjectId,
    pub project_version: String,
    pub artifact_type: ArtifactType,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Content {
    pub project_id: ProjectId,
    pub project_version: String,
    pub artifact_type: ArtifactType,
    pub id: String,
    pub version: String,
    pub path: String,
}
pub(crate) enum Query {
    List(List),
    Review(Review),
    Content(Content),
}
fn invalid(_: impl std::fmt::Display) -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
fn non_null<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn scalar(value: &str, maximum: usize) -> io::Result<()> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub(super) fn relative_path(value: &str, maximum: usize) -> io::Result<()> {
    scalar(value, maximum)?;
    if (cfg!(windows) && (value.contains('\\') || value.contains(':')))
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
fn identity(
    id: &str,
    version: &str,
    policy: &HubResourcePolicy,
) -> io::Result<(ArtifactId, ConcreteVersion)> {
    scalar(id, policy.content().max_scalar_bytes)?;
    scalar(version, policy.content().max_scalar_bytes)?;
    Ok((
        ArtifactId::new(id, policy.projection()).map_err(invalid)?,
        ConcreteVersion::new(version, policy.projection()).map_err(invalid)?,
    ))
}
fn page(offset: usize, limit: usize, policy: &HubResourcePolicy) -> io::Result<()> {
    if limit == 0
        || limit > policy.content().max_collection_entries
        || offset.checked_add(limit).is_none()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigSnapshot};
    use serde_json::{Value, json};

    fn policy() -> HubResourcePolicy {
        let brand = Brand::default();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        HubResourcePolicy::resolve(&config).unwrap()
    }
    fn input(operation: Operation) -> Value {
        let mut value = json!({"project_id":"01KSH1CRFFG8Z74B5ZS78WW808","project_version":"main"});
        match operation {
            Operation::List => value["limit"] = json!(2),
            Operation::Review | Operation::Content => {
                value["artifact_type"] = json!("skill");
                value["id"] = json!("fixture/authored");
                value["version"] = json!("release/one");
                if matches!(operation, Operation::Review) {
                    value["limit"] = json!(2);
                } else {
                    value["path"] = json!("references/guide.md");
                }
            }
        }
        value
    }
    fn decode(value: &Value, operation: Operation) -> io::Result<Query> {
        Query::decode(&serde_json::to_vec(value).unwrap(), operation)
    }
    #[test]
    fn strict_authored_bodies_reject_unknown_duplicate_null_and_wrong_types() {
        let policy = policy();
        for operation in [Operation::List, Operation::Review, Operation::Content] {
            let base = input(operation);
            decode(&base, operation).unwrap().validate(&policy).unwrap();
            for field in [
                "project_dir",
                "source_dir",
                "provider",
                "scope",
                "binding",
                "metadata",
            ] {
                let mut value = base.clone();
                value[field] = json!("not authority");
                assert!(decode(&value, operation).is_err());
            }
            for field in base.as_object().unwrap().keys() {
                for bad in [Value::Null, json!([]), json!({})] {
                    let mut value = base.clone();
                    value[field] = bad;
                    assert!(decode(&value, operation).is_err());
                }
                let mut duplicate = serde_json::to_string(&base).unwrap();
                duplicate.pop();
                duplicate.push_str(&format!(
                    ",{0}:{1}}}",
                    serde_json::to_string(field).unwrap(),
                    base[field]
                ));
                assert!(Query::decode(duplicate.as_bytes(), operation).is_err());
            }
        }
        for field in ["artifact_type", "id", "version", "cursor"] {
            let mut value = input(Operation::List);
            value[field] = Value::Null;
            assert!(decode(&value, Operation::List).is_err());
        }
        let mut review = input(Operation::Review);
        review["offset"] = Value::Null;
        assert!(decode(&review, Operation::Review).is_err());
    }
    #[test]
    fn literal_all_artifact_kinds_and_positive_bounded_pages_are_validated_without_io() {
        let policy = policy();
        for kind in ArtifactType::ALL {
            let mut value = input(Operation::Review);
            value["artifact_type"] = json!(kind);
            decode(&value, Operation::Review)
                .unwrap()
                .validate(&policy)
                .unwrap();
        }
        for limit in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("1"),
            json!(policy.content().max_collection_entries + 1),
        ] {
            let mut value = input(Operation::List);
            value["limit"] = limit;
            assert!(
                decode(&value, Operation::List)
                    .and_then(|q| q.validate(&policy))
                    .is_err()
            );
        }
        let mut review = input(Operation::Review);
        review["offset"] = json!(usize::MAX);
        assert!(
            decode(&review, Operation::Review)
                .unwrap()
                .validate(&policy)
                .is_err()
        );
        for version in ["", "\0", &"x".repeat(policy.content().max_scalar_bytes + 1)] {
            let mut value = input(Operation::List);
            value["project_version"] = json!(version);
            assert!(
                decode(&value, Operation::List)
                    .unwrap()
                    .validate(&policy)
                    .is_err()
            );
        }
    }
    #[test]
    fn cursor_binds_exact_project_version_filters_and_literal_boundary() {
        let policy = policy();
        let mut value = input(Operation::List);
        value["artifact_type"] = json!("skill");
        value["id"] = json!("fixture/authored");
        value["version"] = json!("release/one");
        let Query::List(list) = decode(&value, Operation::List).unwrap() else {
            unreachable!()
        };
        let (id, version) = identity("fixture/authored", "release/one", &policy).unwrap();
        let after = ResourceIdentity {
            artifact_type: ArtifactType::Skill,
            id,
            version,
        };
        let cursor = list.next_cursor(after.clone(), &policy).unwrap();
        value["cursor"] = json!(cursor);
        let Query::List(list) = decode(&value, Operation::List).unwrap() else {
            unreachable!()
        };
        assert_eq!(list.after(&policy).unwrap(), Some(after));
        for (field, changed) in [
            ("project_id", json!("01M1YEAE1WYMWG9KJKV1XQC4T4")),
            ("project_version", json!("other")),
            ("artifact_type", json!("agent")),
            ("id", json!("other")),
            ("version", json!("latest")),
        ] {
            let mut changed_value = value.clone();
            changed_value[field] = changed;
            assert!(
                decode(&changed_value, Operation::List)
                    .unwrap()
                    .validate(&policy)
                    .is_err()
            );
        }
        for cursor in [
            "",
            "%%%",
            "e30",
            &"A".repeat(policy.content().max_scalar_bytes + 1),
        ] {
            let mut changed_value = value.clone();
            changed_value["cursor"] = json!(cursor);
            assert!(
                decode(&changed_value, Operation::List)
                    .unwrap()
                    .validate(&policy)
                    .is_err()
            );
        }
    }
    #[test]
    fn content_paths_reject_host_paths_traversal_and_noncanonical_components() {
        for path in [
            "",
            "/absolute",
            "../outside",
            "a/../outside",
            "./file",
            "a//file",
            "a/",
            "a\0file",
        ] {
            assert!(relative_path(path, 128).is_err(), "rejected path");
        }
        #[cfg(windows)]
        for path in ["C:/file", "a\\file", "file:stream"] {
            assert!(relative_path(path, 128).is_err(), "Windows path syntax");
        }
        relative_path("references/guia-á.md", 128).unwrap();
        assert!(relative_path("file", 3).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn unix_literal_colon_and_backslash_names_remain_relative() {
        for path in ["assets/a:b", "assets/a\\b", "C:/file"] {
            relative_path(path, 128).unwrap();
        }
    }
}

/// The cursor is bounded observation data. It carries no scope or read proof.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    project_id: ProjectId,
    project_version: String,
    artifact_type: Option<ArtifactType>,
    id: Option<String>,
    version: Option<String>,
    after: Boundary,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    artifact_type: ArtifactType,
    id: String,
    version: String,
}
impl List {
    pub(super) fn after(&self, policy: &HubResourcePolicy) -> io::Result<Option<ResourceIdentity>> {
        let Some(encoded) = &self.cursor else {
            return Ok(None);
        };
        scalar(encoded, policy.content().max_scalar_bytes)?;
        let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(invalid)?;
        let cursor: Cursor = serde_json::from_slice(&bytes).map_err(invalid)?;
        if cursor.project_id != self.project_id
            || cursor.project_version != self.project_version
            || cursor.artifact_type != self.artifact_type
            || cursor.id != self.id
            || cursor.version != self.version
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (id, version) = identity(&cursor.after.id, &cursor.after.version, policy)?;
        if self
            .artifact_type
            .is_some_and(|kind| kind != cursor.after.artifact_type)
            || self
                .id
                .as_ref()
                .is_some_and(|selected| selected != id.as_str())
            || self
                .version
                .as_ref()
                .is_some_and(|selected| selected != version.as_str())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Some(ResourceIdentity {
            artifact_type: cursor.after.artifact_type,
            id,
            version,
        }))
    }
    pub(super) fn next_cursor(
        &self,
        after: ResourceIdentity,
        policy: &HubResourcePolicy,
    ) -> io::Result<String> {
        let cursor = Cursor {
            project_id: self.project_id,
            project_version: self.project_version.clone(),
            artifact_type: self.artifact_type,
            id: self.id.clone(),
            version: self.version.clone(),
            after: Boundary {
                artifact_type: after.artifact_type,
                id: after.id.as_str().into(),
                version: after.version.as_str().into(),
            },
        };
        let encoded =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).map_err(io::Error::other)?);
        if encoded.len() > policy.content().max_scalar_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(encoded)
    }
}
impl Query {
    pub(crate) fn decode(bytes: &[u8], operation: Operation) -> io::Result<Self> {
        match operation {
            Operation::List => serde_json::from_slice(bytes).map(Self::List),
            Operation::Review => serde_json::from_slice(bytes).map(Self::Review),
            Operation::Content => serde_json::from_slice(bytes).map(Self::Content),
        }
        .map_err(invalid)
    }
    pub(crate) fn selection(&self) -> (ProjectId, &str) {
        match self {
            Self::List(q) => (q.project_id, &q.project_version),
            Self::Review(q) => (q.project_id, &q.project_version),
            Self::Content(q) => (q.project_id, &q.project_version),
        }
    }
    pub(crate) fn validate(&self, policy: &HubResourcePolicy) -> io::Result<()> {
        scalar(self.selection().1, policy.content().max_scalar_bytes)?;
        match self {
            Self::List(q) => {
                page(0, q.limit, policy)?;
                if let Some(id) = &q.id {
                    scalar(id, policy.content().max_scalar_bytes)?;
                    ArtifactId::new(id, policy.projection()).map_err(invalid)?;
                }
                if let Some(version) = &q.version {
                    scalar(version, policy.content().max_scalar_bytes)?;
                    ConcreteVersion::new(version, policy.projection()).map_err(invalid)?;
                }
                q.after(policy)?;
            }
            Self::Review(q) => {
                identity(&q.id, &q.version, policy)?;
                page(q.offset, q.limit, policy)?;
            }
            Self::Content(q) => {
                identity(&q.id, &q.version, policy)?;
                relative_path(&q.path, policy.content().max_scalar_bytes)?;
            }
        }
        Ok(())
    }
}
