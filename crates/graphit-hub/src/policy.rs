//! Finite, captured Hub policy. Resolution has no filesystem or environment effects.
use crate::{
    ProjectionLimits,
    materialization::{LineageLimits, MaterializationLimits},
    native::{
        AgentDescriptor, ContentError, ContentLimits, NativeBinding, RenderContext,
        RenderedContent, SkillDescriptor, ToolGrants, render_agent, render_skill,
    },
    registry::{DocumentLimits, HubLimits},
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet,
        SnapshotContext, SnapshotPolicy,
    },
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
};
use std::{
    fmt, io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// One catalog drives resolution, provenance and the configuration reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitSpec {
    pub setting: ResourceSetting,
    pub key: &'static str,
    pub default_value: u64,
    /// An explicit policy ceiling, including existing constructor bounds.
    pub hard_max: Option<u64>,
}
macro_rules! settings {
    ($($name:ident => ($key:literal, $default:expr, $max:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(usize)]
        pub enum ResourceSetting { $($name),+ }
        pub const RESOURCE_LIMITS: &[LimitSpec] = &[$(LimitSpec {
            setting: ResourceSetting::$name,
            key: concat!("hub.limits.", $key),
            default_value: $default,
            hard_max: $max,
        }),+];
        impl ResourceSetting {
            pub fn spec(self) -> &'static LimitSpec { &RESOURCE_LIMITS[self as usize] }
        }
    };
}
settings! {
    RegistryBytes => ("registry.bytes", 16777216, Some(16777216)),
    RegistryEntries => ("registry.entries", 10000, Some(100000)),
    RegistryKeyBytes => ("registry.key_bytes", 65536, Some(65536)),
    RegistryJsonNodes => ("registry.json_nodes", 1000000, Some(1000000)),
    RegistryStringBytes => ("registry.string_bytes", 16777216, Some(16777216)),
    ProjectionIdentityBytes => ("projection.identity_bytes", 65536, Some(65536)),
    ProjectionPathBytes => ("projection.path_bytes", 65536, Some(65536)),
    ProjectionFallbackEntries => ("projection.fallback_entries", 10000, Some(100000)),
    ProjectionNameBytes => ("projection.name_bytes", 16777216, Some(16777216)),
    ProjectionBatchItems => ("projection.batch_items", 1000, Some(100000)),
    ArtifactBytes => ("artifact.bytes", 16777216, Some(16777216)),
    ArtifactKeyBytes => ("artifact.key_bytes", 65536, Some(65536)),
    CopyBytes => ("copy.bytes", 536870912, None),
    CopyNodes => ("copy.nodes", 100000, None),
    CopyPathBytes => ("copy.path_bytes", 16777216, None),
    CopyDepth => ("copy.depth", 128, Some(256)),
    SnapshotBytes => ("snapshot.bytes", 67108864, None),
    SnapshotNodes => ("snapshot.nodes", 10000, None),
    SnapshotPathBytes => ("snapshot.path_bytes", 1048576, None),
    SnapshotDepth => ("snapshot.depth", 128, Some(256)),
    RetainedBytes => ("retained.bytes", 8388608, None),
    RetainedFiles => ("retained.files", 16, None),
    RetainedPathBytes => ("retained.path_bytes", 1048576, None),
    LineageSchemaBytes => ("lineage.schema_bytes", 128, Some(16777216)),
    LineageSelectionBytes => ("lineage.selection_bytes", 256, Some(16777216)),
    LineageTotalBytes => ("lineage.total_bytes", 4096, Some(16777216)),
    ContentInputBytes => ("content.input_bytes", 8388608, None),
    ContentOutputBytes => ("content.output_bytes", 16777216, None),
    ContentDepth => ("content.depth", 32, Some(80)),
    ContentNodes => ("content.nodes", 100000, None),
    ContentScalarBytes => ("content.scalar_bytes", 1048576, None),
    ContentCollectionEntries => ("content.collection_entries", 10000, None),
    SharedInputBytes => ("shared.input_bytes", 1048576, None),
    SharedOutputBytes => ("shared.output_bytes", 2097152, None),
    SharedJournalBytes => ("shared.journal_bytes", 4194304, None),
    SharedNodes => ("shared.nodes", 100000, None),
    SharedDepth => ("shared.depth", 32, Some(80)),
    SharedScalarBytes => ("shared.scalar_bytes", 1048576, None),
    SharedEntries => ("shared.entries", 4096, None),
    SharedHookTuples => ("shared.hook_tuples", 1024, None),
    SharedReferences => ("shared.references", 10000, None),
    SharedDocuments => ("shared.documents", 1000, None),
    RequestDeadlineMs => ("request.deadline_ms", 120000, None),
    TransportBytes => ("transport.bytes", 1048576, None),
    TransportNodes => ("transport.nodes", 100000, None),
    TransportDepth => ("transport.depth", 32, Some(80)),
    TransportScalarBytes => ("transport.scalar_bytes", 1048576, None),
    TransportCollectionEntries => ("transport.collection_entries", 10000, None),
    TransportInflight => ("transport.inflight", 32, None),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitOrigin {
    Configuration(ConfigSource),
    /// The built-in explicit profile; no Distribution layer was claimed present.
    DistributionDefault,
}
impl LimitOrigin {
    pub fn source(self) -> ConfigSource {
        match self {
            Self::Configuration(source) => source,
            Self::DistributionDefault => ConfigSource::Distribution,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedLimit {
    pub setting: ResourceSetting,
    pub value: u64,
    pub origin: LimitOrigin,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyErrorKind {
    InvalidInteger,
    NonPositive,
    HardBound,
    Overflow,
    Incoherent,
}
/// Errors identify a declared public setting and its origin, never the input value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyError {
    pub setting: ResourceSetting,
    pub origin: LimitOrigin,
    pub kind: PolicyErrorKind,
}
impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid Hub policy {} ({:?}, {:?})",
            self.setting.spec().key,
            self.origin,
            self.kind
        )
    }
}
impl std::error::Error for PolicyError {}

/// Validated packet for the shared document producer. It is not a writer API.
#[derive(Clone, Copy, Debug)]
pub struct SharedWriteLimits {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub journal_bytes: usize,
    pub nodes: usize,
    pub depth: usize,
    pub scalar_bytes: usize,
    pub entries: usize,
    pub hook_tuples: usize,
    pub references: usize,
    pub documents: usize,
}
/// Independent decoder/admission packet; dispatchers still own enforcement.
#[derive(Clone, Copy, Debug)]
pub struct TransportLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub depth: usize,
    pub scalar_bytes: usize,
    pub collection_entries: usize,
    pub inflight: usize,
}

#[derive(Clone, Debug)]
pub struct HubResourcePolicy {
    hub: HubLimits,
    document: DocumentLimits,
    copy: CopyLimits,
    retained: RetainedBytesLimits,
    content: ContentLimits,
    shared: SharedWriteLimits,
    transport: TransportLimits,
    resolved: Vec<ResolvedLimit>,
}
impl HubResourcePolicy {
    pub fn distribution_defaults(brand: &Brand) -> io::Result<ConfigDocument> {
        ConfigDocument::new(
            brand,
            ConfigSource::Distribution,
            RESOURCE_LIMITS
                .iter()
                .map(|spec| {
                    Ok((
                        ConfigKey::new(spec.key)?,
                        serde_json::Value::from(spec.default_value),
                    ))
                })
                .collect::<io::Result<Vec<_>>>()?,
        )
    }
    pub fn resolve(config: &ConfigSnapshot) -> Result<Self, PolicyError> {
        let mut values = Vec::with_capacity(RESOURCE_LIMITS.len());
        for spec in RESOURCE_LIMITS {
            let key = ConfigKey::new(spec.key).expect("static canonical Hub setting");
            let source = config.source(&key);
            let origin =
                source.map_or(LimitOrigin::DistributionDefault, LimitOrigin::Configuration);
            let value = if source.is_some() {
                config
                    .get::<u64>(&key)
                    .map_err(|_| PolicyError {
                        setting: spec.setting,
                        origin,
                        kind: PolicyErrorKind::InvalidInteger,
                    })?
                    .ok_or(PolicyError {
                        setting: spec.setting,
                        origin,
                        kind: PolicyErrorKind::InvalidInteger,
                    })?
            } else {
                spec.default_value
            };
            let error = |kind| PolicyError {
                setting: spec.setting,
                origin,
                kind,
            };
            if value == 0 {
                return Err(error(PolicyErrorKind::NonPositive));
            }
            if spec.hard_max.is_some_and(|cap| value > cap) {
                return Err(error(PolicyErrorKind::HardBound));
            }
            // Only streamed bytes and the millisecond clock budget remain u64.
            if !matches!(
                spec.setting,
                ResourceSetting::CopyBytes
                    | ResourceSetting::SnapshotBytes
                    | ResourceSetting::RequestDeadlineMs
            ) {
                usize::try_from(value).map_err(|_| error(PolicyErrorKind::Overflow))?;
            }
            values.push(ResolvedLimit {
                setting: spec.setting,
                value,
                origin,
            });
        }
        let v = Values(&values);
        v.coherence()?;
        use ResourceSetting::*;
        let copy = v.copy(CopyBytes, CopyNodes, CopyPathBytes, CopyDepth)?;
        let snapshots = v.copy(
            SnapshotBytes,
            SnapshotNodes,
            SnapshotPathBytes,
            SnapshotDepth,
        )?;
        let artifacts = ArtifactLimits::new(v.us(ArtifactBytes), v.us(ArtifactKeyBytes), copy)
            .map_err(|_| v.error(ArtifactBytes, PolicyErrorKind::Incoherent))?;
        let projection = ProjectionLimits::new(
            v.us(ProjectionIdentityBytes),
            v.us(ProjectionPathBytes),
            v.us(ProjectionFallbackEntries),
            v.us(ProjectionNameBytes),
            v.us(ProjectionBatchItems),
        )
        .map_err(|_| v.error(ProjectionPathBytes, PolicyErrorKind::HardBound))?;
        let lineage = LineageLimits::new(
            v.us(LineageSchemaBytes),
            v.us(LineageSelectionBytes),
            v.us(LineageTotalBytes),
        )
        .map_err(|_| v.error(LineageTotalBytes, PolicyErrorKind::Incoherent))?;
        let materialization = MaterializationLimits {
            output: copy,
            snapshots,
            lineage,
        };
        let document = DocumentLimits {
            bytes: v.us(RegistryBytes),
            entries: v.us(RegistryEntries),
            key_bytes: v.us(RegistryKeyBytes),
            json_nodes: v.us(RegistryJsonNodes),
            json_string_bytes: v.us(RegistryStringBytes),
        };
        let hub = HubLimits::new(document, projection, artifacts, materialization)
            .map_err(|_| v.error(RegistryBytes, PolicyErrorKind::Incoherent))?;
        let retained = RetainedBytesLimits::new(
            v.us(RetainedBytes),
            v.us(RetainedFiles),
            v.us(RetainedPathBytes),
        )
        .map_err(|_| v.error(RetainedBytes, PolicyErrorKind::Incoherent))?;
        let content = ContentLimits {
            max_input_bytes: v.us(ContentInputBytes),
            max_output_bytes: v.us(ContentOutputBytes),
            max_depth: v.us(ContentDepth),
            max_nodes: v.us(ContentNodes),
            max_scalar_bytes: v.us(ContentScalarBytes),
            max_collection_entries: v.us(ContentCollectionEntries),
        };
        let shared = SharedWriteLimits {
            input_bytes: v.us(SharedInputBytes),
            output_bytes: v.us(SharedOutputBytes),
            journal_bytes: v.us(SharedJournalBytes),
            nodes: v.us(SharedNodes),
            depth: v.us(SharedDepth),
            scalar_bytes: v.us(SharedScalarBytes),
            entries: v.us(SharedEntries),
            hook_tuples: v.us(SharedHookTuples),
            references: v.us(SharedReferences),
            documents: v.us(SharedDocuments),
        };
        let transport = TransportLimits {
            bytes: v.us(TransportBytes),
            nodes: v.us(TransportNodes),
            depth: v.us(TransportDepth),
            scalar_bytes: v.us(TransportScalarBytes),
            collection_entries: v.us(TransportCollectionEntries),
            inflight: v.us(TransportInflight),
        };
        Ok(Self {
            hub,
            document,
            copy,
            retained,
            content,
            shared,
            transport,
            resolved: values,
        })
    }
    pub fn resolved(&self) -> &[ResolvedLimit] {
        &self.resolved
    }
    pub fn limit(&self, setting: ResourceSetting) -> ResolvedLimit {
        self.resolved[setting as usize]
    }
    pub fn hub(&self) -> HubLimits {
        self.hub
    }
    pub fn document(&self) -> DocumentLimits {
        self.document
    }
    pub fn projection(&self) -> ProjectionLimits {
        self.hub.projection()
    }
    pub fn artifacts(&self) -> ArtifactLimits {
        self.hub.artifacts()
    }
    pub fn materialization(&self) -> MaterializationLimits {
        self.hub.materialization()
    }
    pub fn copy(&self) -> CopyLimits {
        self.copy
    }
    pub fn retained(&self) -> RetainedBytesLimits {
        self.retained
    }
    pub fn content(&self) -> ContentLimits {
        self.content
    }
    pub fn shared(&self) -> SharedWriteLimits {
        self.shared
    }
    pub fn transport(&self) -> TransportLimits {
        self.transport
    }
    /// Selected descriptor files only. The caller supplies its qualified link policy.
    pub fn snapshot_policy(
        &self,
        links: CopyPolicy,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> io::Result<SnapshotPolicy> {
        SnapshotPolicy::new(
            links,
            self.materialization().snapshots,
            RetainedFileSet::new(paths, self.retained)?,
            self.retained,
        )
    }
    pub fn request_context<'a>(
        &self,
        start: Instant,
        cancellation: &'a Cancellation,
    ) -> Result<RequestContext<'a>, RequestError> {
        let deadline = start
            .checked_add(Duration::from_millis(
                self.limit(ResourceSetting::RequestDeadlineMs).value,
            ))
            .ok_or(RequestError::DeadlineOverflow)?;
        RequestContext::new(self.content, cancellation, deadline)
    }
}

struct Values<'a>(&'a [ResolvedLimit]);
impl Values<'_> {
    fn get(&self, setting: ResourceSetting) -> u64 {
        self.0[setting as usize].value
    }
    fn us(&self, setting: ResourceSetting) -> usize {
        usize::try_from(self.get(setting)).expect("checked policy integer")
    }
    fn error(&self, setting: ResourceSetting, kind: PolicyErrorKind) -> PolicyError {
        PolicyError {
            setting,
            origin: self.0[setting as usize].origin,
            kind,
        }
    }
    fn copy(
        &self,
        bytes: ResourceSetting,
        nodes: ResourceSetting,
        paths: ResourceSetting,
        depth: ResourceSetting,
    ) -> Result<CopyLimits, PolicyError> {
        CopyLimits::new(
            self.get(bytes),
            self.us(nodes),
            self.us(paths),
            self.us(depth),
        )
        .map_err(|_| self.error(depth, PolicyErrorKind::HardBound))
    }
    fn coherence(&self) -> Result<(), PolicyError> {
        use ResourceSetting::*;
        for (lower, upper) in [
            (RegistryKeyBytes, ArtifactKeyBytes),
            (RegistryKeyBytes, RegistryBytes),
            (ArtifactKeyBytes, ArtifactBytes),
            (RegistryStringBytes, RegistryBytes),
            (ProjectionIdentityBytes, RegistryKeyBytes),
            (SnapshotBytes, CopyBytes),
            (SnapshotNodes, CopyNodes),
            (SnapshotPathBytes, CopyPathBytes),
            (SnapshotDepth, CopyDepth),
            (RetainedBytes, SnapshotBytes),
            (RetainedFiles, SnapshotNodes),
            (RetainedPathBytes, SnapshotPathBytes),
            (ContentScalarBytes, ContentInputBytes),
            (ContentScalarBytes, ContentOutputBytes),
            (ContentCollectionEntries, ContentNodes),
            (SharedScalarBytes, SharedInputBytes),
            (SharedScalarBytes, SharedOutputBytes),
            (SharedEntries, SharedNodes),
            (SharedHookTuples, SharedNodes),
            (SharedReferences, SharedNodes),
            (SharedDocuments, SharedNodes),
            (TransportScalarBytes, TransportBytes),
            (TransportCollectionEntries, TransportNodes),
        ] {
            if self.get(lower) > self.get(upper) {
                return Err(self.error(lower, PolicyErrorKind::Incoherent));
            }
        }
        let lineage = [LineageSchemaBytes, LineageSelectionBytes]
            .into_iter()
            .try_fold(64u64, |sum, field| {
                sum.checked_add(self.get(field))
                    .ok_or_else(|| self.error(field, PolicyErrorKind::Overflow))
            })?;
        if lineage > self.get(LineageTotalBytes) {
            return Err(self.error(LineageTotalBytes, PolicyErrorKind::Incoherent));
        }
        for setting in [
            ContentInputBytes,
            ContentOutputBytes,
            SharedInputBytes,
            SharedOutputBytes,
            SharedJournalBytes,
            TransportBytes,
        ] {
            self.us(setting)
                .checked_add(1)
                .ok_or_else(|| self.error(setting, PolicyErrorKind::Overflow))?;
        }
        for setting in [ContentNodes, SharedNodes, TransportNodes] {
            self.us(setting)
                .checked_mul(4)
                .ok_or_else(|| self.error(setting, PolicyErrorKind::Overflow))?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum RequestError {
    DeadlineOverflow,
    Content(ContentError),
}
impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeadlineOverflow => f.write_str("Hub deadline overflow"),
            Self::Content(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for RequestError {}

/// One operation/batch. Keep this context across capture, render and publication.
/// The renderer is private so a caller cannot replace it and reset its counters.
///
/// ```compile_fail
/// use graphit_hub::{policy::RequestContext, native::RenderContext};
/// fn reset(request: &mut RequestContext<'_>, replacement: RenderContext<'_>) {
///     request.renderer = replacement;
/// }
/// ```
pub struct RequestContext<'a> {
    renderer: RenderContext<'a>,
    snapshot: Arc<SnapshotContext>,
    cancellation: &'a Cancellation,
}
impl<'a> RequestContext<'a> {
    /// Standalone libraries can supply an explicit finite policy without configuration.
    pub fn new(
        limits: ContentLimits,
        cancellation: &'a Cancellation,
        deadline: Instant,
    ) -> Result<Self, RequestError> {
        let renderer =
            RenderContext::new(limits, cancellation, deadline).map_err(RequestError::Content)?;
        let snapshot = Arc::new(SnapshotContext::new(deadline, cancellation.clone()));
        Ok(Self {
            renderer,
            snapshot,
            cancellation,
        })
    }
    pub fn parse_skill<'i>(
        &mut self,
        input: &'i [u8],
    ) -> Result<SkillDescriptor<'i>, ContentError> {
        SkillDescriptor::parse(input, &mut self.renderer)
    }
    pub fn parse_agent<'i>(
        &mut self,
        input: &'i [u8],
    ) -> Result<AgentDescriptor<'i>, ContentError> {
        AgentDescriptor::parse(input, &mut self.renderer)
    }
    pub fn managed_agent<'i>(
        &mut self,
        name: &str,
        description: &str,
        body: &'i str,
        read_only: bool,
        allows_shell: bool,
    ) -> Result<AgentDescriptor<'i>, ContentError> {
        AgentDescriptor::new(
            name,
            description,
            body,
            read_only,
            allows_shell,
            &mut self.renderer,
        )
    }
    pub fn render_skill(
        &mut self,
        binding: NativeBinding,
        descriptor: &SkillDescriptor<'_>,
        alias: Option<&str>,
    ) -> Result<RenderedContent, ContentError> {
        render_skill(binding, descriptor, alias, &mut self.renderer)
    }
    pub fn render_agent(
        &mut self,
        binding: NativeBinding,
        descriptor: &AgentDescriptor<'_>,
        alias: Option<&str>,
        grants: &ToolGrants,
    ) -> Result<RenderedContent, ContentError> {
        render_agent(binding, descriptor, alias, grants, &mut self.renderer)
    }
    /// Captured content capacities; byte counters below describe already rendered content.
    pub fn content_limits(&self) -> ContentLimits {
        self.renderer.limits
    }
    pub fn input_bytes(&self) -> usize {
        self.renderer.input_bytes()
    }
    pub fn output_bytes(&self) -> usize {
        self.renderer.output_bytes()
    }
    pub fn snapshot(&self) -> &Arc<SnapshotContext> {
        &self.snapshot
    }
    pub fn cancellation(&self) -> &Cancellation {
        self.cancellation
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.snapshot.deadline()
    }
    pub fn check(&self) -> io::Result<()> {
        self.snapshot.check()
    }
    pub fn remaining(&self) -> io::Result<Option<Duration>> {
        self.check()?;
        match self.deadline() {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    Err(io::ErrorKind::TimedOut.into())
                } else {
                    Ok(Some(remaining))
                }
            }
            None => Ok(None),
        }
    }
    /// Creates one continuous operation identity; cancellation remains mandatory.
    pub fn service(
        limits: ContentLimits,
        cancellation: &'a Cancellation,
    ) -> Result<Self, RequestError> {
        let renderer = RenderContext::with_deadline(limits, cancellation, None)
            .map_err(RequestError::Content)?;
        let snapshot = Arc::new(SnapshotContext::service(cancellation.clone()));
        snapshot
            .check()
            .map_err(|_| RequestError::Content(ContentError::Cancelled))?;
        Ok(Self {
            renderer,
            snapshot,
            cancellation,
        })
    }
}

/// Checked conversion for callers accepting an explicit MiB display unit.
pub fn mib_to_bytes(value: u64) -> Option<u64> {
    value.checked_mul(1024 * 1024).filter(|bytes| *bytes != 0)
}

#[cfg(test)]
mod tests;
