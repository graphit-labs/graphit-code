use super::{Projects, TaskPolicy};
use axum::http::HeaderMap;
use graphit_core::{native_path::NativePath, project_identity::ProjectId};
use graphit_records::task_protocol::{ReadRequest, WriteRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// Selects a retained project instance; selectors confer no authority.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    pub project_dir: Option<NativePath>,
    pub project_id: Option<ProjectId>,
    pub project_version: Option<String>,
}
impl Selection {
    pub fn select_read(&self, projects: &Projects, current: usize) -> Result<usize, &'static str> {
        if let (Some(root), Some(version)) = (
            self.project_dir.as_ref().map(NativePath::as_path),
            self.project_version.as_deref(),
        ) {
            if version.is_empty() || version.contains('\0') {
                return Err("invalid_selection");
            }
            let selected = crate::live_project_selections::select_project_version(
                projects,
                root,
                Some(version),
            )?;
            if self.project_id.is_some_and(|id| id != projects[selected].1) {
                return Err("selection_unavailable");
            }
            return Ok(selected);
        }
        self.select(projects, current)
    }
    pub fn select(&self, projects: &Projects, current: usize) -> Result<usize, &'static str> {
        if self
            .project_version
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
        {
            return Err("invalid_selection");
        }
        if self.project_dir.is_none() && self.project_id.is_none() && self.project_version.is_none()
        {
            return Ok(current);
        }
        let root = self.project_dir.as_ref().map(NativePath::as_path);
        let mut matches = projects
            .iter()
            .enumerate()
            .filter(|(_, (paths, id, version))| {
                root.is_none_or(|root| root == paths.root())
                    && self.project_id.is_none_or(|requested| requested == *id)
                    && self
                        .project_version
                        .as_deref()
                        .is_none_or(|requested| requested == version.version())
            });
        let (selected, _) = matches.next().ok_or("selection_unavailable")?;
        if matches.next().is_some() {
            return Err("selection_ambiguous");
        }
        Ok(selected)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Arguments {
    #[serde(default)]
    pub selection: Selection,
    pub input: Option<Value>,
}
pub(crate) enum Operation {
    Read(ReadRequest),
    Write(WriteRequest),
}
impl Operation {
    pub fn parse(operation: &str, write: bool, input: Option<Value>) -> Result<Self, &'static str> {
        let mut value = json!({"operation":operation});
        if let Some(input) = input {
            value["input"] = input;
        }
        if write {
            serde_json::from_value(value)
                .map(Self::Write)
                .map_err(|_| "invalid_arguments")
        } else {
            serde_json::from_value(value)
                .map(Self::Read)
                .map_err(|_| "invalid_arguments")
        }
    }
}
pub(crate) enum Origin {
    Runtime,
    Local,
    Role(Zeroizing<String>),
}
pub(crate) const ROLE_BINDING_HEADER: &str = "x-graphit-role-binding";
pub(crate) struct Credential {
    pub role_binding: Option<graphit_hub::NativeBinding>,
    pub bearer: Zeroizing<String>,
    pub origin: Origin,
}
impl Credential {
    pub fn read(headers: &HeaderMap, policy: &TaskPolicy) -> Result<Self, &'static str> {
        let raw = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| !value.is_empty() && value.len() <= policy.content.max_input_bytes)
            .ok_or("credential_required")?;
        let origin = match headers
            .get("x-graphit-auth-origin")
            .and_then(|v| v.to_str().ok())
        {
            Some("runtime") => Origin::Runtime,
            Some("local") => Origin::Local,
            Some("role") => {
                let reference = headers
                    .get("x-graphit-role-reference")
                    .and_then(|v| v.to_str().ok())
                    .filter(|v| !v.is_empty() && v.len() <= policy.content.max_input_bytes)
                    .ok_or("role_reference_required")?;
                Origin::Role(Zeroizing::new(reference.to_owned()))
            }
            _ => return Err("credential_origin_required"),
        };
        let role_binding = if matches!(&origin, Origin::Role(_)) {
            headers
                .get(ROLE_BINDING_HEADER)
                .map(|value| {
                    value
                        .to_str()
                        .map_err(|_| "invalid_role_binding")?
                        .parse()
                        .map_err(|_| "invalid_role_binding")
                })
                .transpose()?
        } else {
            None
        };
        Ok(Self {
            bearer: Zeroizing::new(raw.to_owned()),
            origin,
            role_binding,
        })
    }
}
