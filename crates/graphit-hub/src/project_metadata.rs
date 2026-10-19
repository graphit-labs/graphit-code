//! Canonical current Hub PROJECT metadata; no legacy document decoder.
use graphit_core::project_identity::ProjectId;
use serde::{Deserialize, Serialize};
use std::io;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HubProjectMetadata {
    pub id: ProjectId,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    v: u8,
    project: Project,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    id: ProjectId,
    name: String,
    status: String,
}
pub fn project_metadata_key(id: ProjectId) -> String {
    format!("v1/projects/{id}/hub/metadata/project.json")
}
impl HubProjectMetadata {
    fn validate(&self) -> io::Result<()> {
        if self.name.is_empty()
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8], expected: ProjectId, max_bytes: usize) -> io::Result<Self> {
        if bytes.len() > max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let doc: Document = serde_json::from_slice(bytes)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        if doc.v != 1 || doc.project.id != expected || doc.project.status != "active" {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let project = Self {
            id: doc.project.id,
            name: doc.project.name,
        };
        project.validate()?;
        Ok(project)
    }
    pub fn encode(&self, max_bytes: usize) -> io::Result<Vec<u8>> {
        self.validate()?;
        if self.name.len() > max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let bytes = serde_json::to_vec(&Document {
            v: 1,
            project: Project {
                id: self.id,
                name: self.name.clone(),
                status: "active".into(),
            },
        })
        .map_err(io::Error::other)?;
        if bytes.len() > max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_project_metadata_has_one_key_and_rejects_foreign_or_malformed_identity() {
        let id = "01D39ZY06FGSCTVN4T2V9PKHFZ".parse().unwrap();
        let other = "01D39ZY06FGSCTVN4T2V9PKHG0".parse().unwrap();
        let p = HubProjectMetadata {
            id,
            name: "real-project".into(),
        };
        let bytes = p.encode(1024).unwrap();
        assert_eq!(HubProjectMetadata::decode(&bytes, id, 1024).unwrap(), p);
        assert!(HubProjectMetadata::decode(&bytes, other, 1024).is_err());
        assert!(HubProjectMetadata::decode(&bytes, id, 10).is_err());
        for bad in [
            String::from_utf8(bytes.clone())
                .unwrap()
                .replace("active", "disabled"),
            String::from_utf8(bytes.clone())
                .unwrap()
                .replace(r#""v":1"#, r#""v":2"#),
            String::from_utf8(bytes.clone())
                .unwrap()
                .replace("real-project", " "),
        ] {
            assert!(HubProjectMetadata::decode(bad.as_bytes(), id, 1024).is_err());
        }
        assert_eq!(
            project_metadata_key(id),
            format!("v1/projects/{id}/hub/metadata/project.json")
        );
    }
}
