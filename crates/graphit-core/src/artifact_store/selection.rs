use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadSelection {
    relative: tree::RelativePath,
    kind: codec::Kind,
}
impl PayloadSelection {
    pub fn new(relative: PathBuf, kind: NodeKind) -> io::Result<Self> {
        Ok(Self {
            relative: tree::RelativePath(snapshot::canonical_relative(&relative)?),
            kind: kind.into(),
        })
    }
    pub fn relative(&self) -> &Path {
        &self.relative.0
    }
    pub fn kind(&self) -> NodeKind {
        self.kind.into()
    }
}
impl fmt::Debug for PayloadSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PayloadSelection { .. }")
    }
}
