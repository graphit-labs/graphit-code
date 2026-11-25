//! Native object locations are data; authorization and SDK operations belong to callers.
use graphit_core::{
    artifact_store::UserIdentity, installation_identity::InstallationId,
    project_identity::ProjectId,
};
use std::io;
use url::Url;

const S3_KEY_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpusModule {
    Task,
    Memory,
    Knowledge,
    Ast,
}

impl CorpusModule {
    fn name(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Memory => "memory",
            Self::Knowledge => "knowledge",
            Self::Ast => "ast",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalHubCorpus {
    Registry,
    Rules,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectHubCorpus {
    Metadata,
    Events,
}

#[derive(Clone, Copy)]
pub enum NativeStorageScope<'a> {
    ProjectCorpus {
        project: ProjectId,
        module: CorpusModule,
    },
    UserMemory {
        installation: &'a InstallationId,
        identity: &'a UserIdentity,
    },
    GlobalHub(GlobalHubCorpus),
    ProjectHub {
        project: ProjectId,
        corpus: ProjectHubCorpus,
    },
}

impl NativeStorageScope<'_> {
    fn root(self) -> String {
        match self {
            Self::ProjectCorpus { project, module } => {
                format!("v1/projects/{project}/corpora/{}", module.name())
            }
            Self::UserMemory {
                installation,
                identity,
            } => format!(
                "v1/installations/{}/users/{}/corpora/memory",
                installation.as_str(),
                identity.namespace_key()
            ),
            Self::GlobalHub(corpus) => format!(
                "v1/hub/global/{}",
                match corpus {
                    GlobalHubCorpus::Registry => "registry",
                    GlobalHubCorpus::Rules => "rules",
                }
            ),
            Self::ProjectHub { project, corpus } => format!(
                "v1/projects/{project}/hub/{}",
                match corpus {
                    ProjectHubCorpus::Metadata => "metadata",
                    ProjectHubCorpus::Events => "events",
                }
            ),
        }
    }
}

#[derive(Debug)]
pub struct NativeObjectLocation {
    key: String,
    uri: Url,
}

impl NativeObjectLocation {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn uri(&self) -> &Url {
        &self.uri
    }

    pub fn into_parts(self) -> (String, Url) {
        (self.key, self.uri)
    }
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}

fn relative_key(value: &str) -> io::Result<()> {
    if value.is_empty()
        || value.len() > S3_KEY_BYTES
        || value.contains('\\')
        || value.bytes().any(|byte| byte < 32 || byte == 127)
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid("expected a relative native object key"));
    }
    Ok(())
}

/// Resolve a real object or dataset root. SDK branches are supplied separately.
/// A provider prefix may have one trailing separator; its remaining bytes are literal.
pub fn object_location(
    bucket: &str,
    provider_prefix: &str,
    scope: NativeStorageScope<'_>,
    object: &str,
) -> io::Result<NativeObjectLocation> {
    relative_key(object)?;
    let prefix = provider_prefix.strip_suffix('/').unwrap_or(provider_prefix);
    if !prefix.is_empty() {
        relative_key(prefix)?;
    } else if !provider_prefix.is_empty() {
        return Err(invalid("provider prefix cannot be absolute"));
    }
    if !(3..=63).contains(&bucket.len())
        || !bucket
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte))
        || !bucket.as_bytes()[0].is_ascii_alphanumeric()
        || !bucket.as_bytes()[bucket.len() - 1].is_ascii_alphanumeric()
        || bucket.contains("..")
    {
        return Err(invalid("invalid S3 bucket authority"));
    }
    let mut uri = Url::parse(&format!("s3://{bucket}/"))
        .map_err(|_| invalid("invalid S3 bucket authority"))?;
    let root = scope.root();
    let bytes = prefix.len() + usize::from(!prefix.is_empty()) + root.len() + 1 + object.len();
    if bytes > S3_KEY_BYTES {
        return Err(invalid("native object key exceeds S3's 1024-byte limit"));
    }
    let mut key = String::with_capacity(bytes);
    if !prefix.is_empty() {
        key.push_str(prefix);
        key.push('/');
    }
    key.push_str(&root);
    key.push('/');
    key.push_str(object);
    uri.path_segments_mut()
        .map_err(|_| invalid("invalid S3 bucket authority"))?
        .clear()
        .extend(key.split('/'));
    Ok(NativeObjectLocation { key, uri })
}
