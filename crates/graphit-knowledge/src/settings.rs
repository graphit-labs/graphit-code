//! Pure resolution of captured Knowledge configuration.
//!
//! These settings are data, not filesystem authority. Limits cover admitted
//! strings and criteria; they do not measure allocator RSS or preempt Core calls.

use graphit_core::{
    config::{ConfigKey, ConfigSnapshot, ConfigSource},
    file_io::SnapshotContext,
};
use std::{collections::TryReserveError, error::Error, fmt, io, num::NonZeroUsize};

const DEFAULT_SUFFIXES: [&str; 16] = [
    "md", "markdown", "mdx", "txt", "adoc", "rst", "puml", "plantuml", "yaml", "yml", "json",
    "proto", "graphql", "gql", "wsdl", "xml",
];

#[derive(Clone, Copy, Debug)]
pub struct KnowledgeSettingsLimits {
    pub max_input_bytes: NonZeroUsize,
    pub max_docs_dir_bytes: NonZeroUsize,
    pub max_path_components: NonZeroUsize,
    pub max_extension_tokens: NonZeroUsize,
    pub max_extensions: NonZeroUsize,
    pub max_extension_bytes: NonZeroUsize,
    pub max_projection_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeSetting {
    DocsDir,
    IncludeReadme,
    Extensions,
}

impl KnowledgeSetting {
    pub const fn name(self) -> &'static str {
        match self {
            Self::DocsDir => "knowledge.docs_dir",
            Self::IncludeReadme => "knowledge.include_readme",
            Self::Extensions => "knowledge.extensions",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::DocsDir => 0,
            Self::IncludeReadme => 1,
            Self::Extensions => 2,
        }
    }

    fn key(self) -> ConfigKey {
        ConfigKey::new(self.name()).expect("built-in Knowledge keys are valid")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingOrigin {
    Default,
    Config(ConfigSource),
    IndexPath,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettingsStats {
    pub input_bytes: usize,
    pub path_components: usize,
    pub extension_tokens: usize,
    pub extensions: usize,
    pub projection_bytes: usize,
}

#[derive(Debug)]
pub struct KnowledgeSettings {
    docs_dir: String,
    include_readme: bool,
    extensions: Vec<String>,
    origins: [SettingOrigin; 3],
    stats: SettingsStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsProblem {
    ExpectedText,
    ExpectedBoolean,
    Empty,
    InvalidPath,
    InvalidExtension,
}

impl SettingsProblem {
    const fn message(self) -> &'static str {
        match self {
            Self::ExpectedText => "requires text",
            Self::ExpectedBoolean => "requires a boolean",
            Self::Empty => "must not be empty",
            Self::InvalidPath => "requires a relative slash-separated path or '.'",
            Self::InvalidExtension => "requires single extension suffixes",
        }
    }
}

#[derive(Debug)]
pub enum KnowledgeSettingsError {
    Context(io::Error),
    InvalidValue {
        field: KnowledgeSetting,
        origin: SettingOrigin,
        reason: SettingsProblem,
        source: Option<io::Error>,
    },
    LimitExceeded(&'static str),
    Allocation(TryReserveError),
}

impl KnowledgeSettingsError {
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Context(error) => error.kind(),
            Self::InvalidValue { origin, .. } => match origin {
                SettingOrigin::Config(ConfigSource::Project | ConfigSource::Global) => {
                    io::ErrorKind::InvalidData
                }
                _ => io::ErrorKind::InvalidInput,
            },
            Self::LimitExceeded(_) => io::ErrorKind::FileTooLarge,
            Self::Allocation(_) => io::ErrorKind::OutOfMemory,
        }
    }
}

impl fmt::Display for KnowledgeSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(_) => f.write_str("Knowledge settings request is not live"),
            Self::InvalidValue { field, reason, .. } => {
                write!(f, "{} {}", field.name(), reason.message())
            }
            Self::LimitExceeded(label) => write!(f, "Knowledge settings exceed {label}"),
            Self::Allocation(_) => f.write_str("cannot allocate Knowledge settings"),
        }
    }
}

impl Error for KnowledgeSettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::InvalidValue { source, .. } => source.as_ref().map(|e| e as &dyn Error),
            Self::Allocation(error) => Some(error),
            Self::LimitExceeded(_) => None,
        }
    }
}

type Result<T> = std::result::Result<T, KnowledgeSettingsError>;

impl KnowledgeSettings {
    /// Resolve the complete view or fail. `index_path` wins only its own field.
    ///
    /// ```
    /// use graphit_core::{brand::Brand, config::ConfigSnapshot,
    ///     file_io::SnapshotContext, file_lock::Cancellation};
    /// use graphit_knowledge::settings::{KnowledgeSettings, KnowledgeSettingsLimits,
    ///     KnowledgeSetting, SettingOrigin};
    /// use std::{num::NonZeroUsize, time::{Duration, Instant}};
    ///
    /// let config = ConfigSnapshot::new(&Brand::default(), [])?;
    /// let context = SnapshotContext::new(Instant::now() + Duration::from_secs(30), Cancellation::default());
    /// let cap = NonZeroUsize::new(1_000).unwrap();
    /// let settings = KnowledgeSettings::resolve(&context, &config, Some("."),
    ///     KnowledgeSettingsLimits {
    ///         max_input_bytes: cap, max_docs_dir_bytes: cap, max_path_components: cap,
    ///         max_extension_tokens: cap, max_extensions: cap, max_extension_bytes: cap,
    ///         max_projection_bytes: cap,
    ///     })?;
    /// assert_eq!(settings.docs_dir(), ".");
    /// assert_eq!(settings.origin(KnowledgeSetting::DocsDir), SettingOrigin::IndexPath);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn resolve(
        context: &SnapshotContext,
        snapshot: &ConfigSnapshot,
        index_path: Option<&str>,
        limits: KnowledgeSettingsLimits,
    ) -> Result<Self> {
        check(context)?;
        let mut stats = SettingsStats::default();
        let (raw_docs, docs_origin) = match index_path {
            Some(path) => (path, SettingOrigin::IndexPath),
            None => {
                let (value, origin) = text(context, snapshot, KnowledgeSetting::DocsDir)?;
                (value.unwrap_or("docs"), origin)
            }
        };
        if docs_origin != SettingOrigin::Default {
            input(&mut stats, raw_docs, limits)?;
        }
        bounded(
            raw_docs.len(),
            limits.max_docs_dir_bytes,
            "max_docs_dir_bytes",
        )?;
        let docs = trimmed(context, raw_docs)?;
        validate_path(context, docs, docs_origin, &mut stats, limits)?;
        let docs_dir = copy_text(context, docs, &mut stats, limits)?;

        let bool_field = KnowledgeSetting::IncludeReadme;
        let bool_key = bool_field.key();
        let bool_origin = origin(snapshot, &bool_key);
        check(context)?;
        // This borrowed probe only accounts string winners. The typed get below
        // still validates the very same winner; no other layer or type is tried.
        let raw_bool = snapshot.get::<&str>(&bool_key);
        check(context)?;
        if let Ok(Some(raw)) = raw_bool {
            input(&mut stats, raw, limits)?;
        }
        check(context)?;
        let value = snapshot.get::<bool>(&bool_key);
        check(context)?;
        let include_readme = value
            .map_err(|error| {
                invalid(
                    bool_field,
                    bool_origin,
                    SettingsProblem::ExpectedBoolean,
                    Some(error),
                )
            })?
            .unwrap_or(true);

        let (raw_extensions, extensions_origin) =
            text(context, snapshot, KnowledgeSetting::Extensions)?;
        let mut extensions = Vec::new();
        match raw_extensions {
            Some(raw) => {
                input(&mut stats, raw, limits)?;
                stats.extension_tokens = count_criteria(
                    context,
                    raw.split(','),
                    limits.max_extension_tokens,
                    "max_extension_tokens",
                )?;
                for token in raw.split(',') {
                    extension(
                        context,
                        token,
                        extensions_origin,
                        &mut extensions,
                        &mut stats,
                        limits,
                    )?;
                }
            }
            None => {
                stats.extension_tokens = count_criteria(
                    context,
                    DEFAULT_SUFFIXES,
                    limits.max_extension_tokens,
                    "max_extension_tokens",
                )?;
                for token in DEFAULT_SUFFIXES {
                    extension(
                        context,
                        token,
                        extensions_origin,
                        &mut extensions,
                        &mut stats,
                        limits,
                    )?;
                }
            }
        }
        if extensions.is_empty() {
            return Err(invalid(
                KnowledgeSetting::Extensions,
                extensions_origin,
                SettingsProblem::Empty,
                None,
            ));
        }
        check(context)?;
        Ok(Self {
            docs_dir,
            include_readme,
            extensions,
            origins: [docs_origin, bool_origin, extensions_origin],
            stats,
        })
    }

    pub fn docs_dir(&self) -> &str {
        &self.docs_dir
    }

    pub const fn include_readme(&self) -> bool {
        self.include_readme
    }

    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    pub const fn origin(&self, field: KnowledgeSetting) -> SettingOrigin {
        self.origins[field.index()]
    }

    pub const fn stats(&self) -> SettingsStats {
        self.stats
    }
}

fn check(context: &SnapshotContext) -> Result<()> {
    context.check().map_err(KnowledgeSettingsError::Context)
}

fn origin(snapshot: &ConfigSnapshot, key: &ConfigKey) -> SettingOrigin {
    snapshot
        .source(key)
        .map_or(SettingOrigin::Default, SettingOrigin::Config)
}

fn invalid(
    field: KnowledgeSetting,
    origin: SettingOrigin,
    reason: SettingsProblem,
    source: Option<io::Error>,
) -> KnowledgeSettingsError {
    KnowledgeSettingsError::InvalidValue {
        field,
        origin,
        reason,
        source,
    }
}

fn text<'a>(
    context: &SnapshotContext,
    snapshot: &'a ConfigSnapshot,
    field: KnowledgeSetting,
) -> Result<(Option<&'a str>, SettingOrigin)> {
    check(context)?;
    let key = field.key();
    let origin = origin(snapshot, &key);
    let value = snapshot.get::<&str>(&key);
    check(context)?;
    value
        .map(|value| (value, origin))
        .map_err(|error| invalid(field, origin, SettingsProblem::ExpectedText, Some(error)))
}

fn bounded(value: usize, cap: NonZeroUsize, label: &'static str) -> Result<usize> {
    if value > cap.get() {
        Err(KnowledgeSettingsError::LimitExceeded(label))
    } else {
        Ok(value)
    }
}

fn added(total: usize, value: usize, cap: NonZeroUsize, label: &'static str) -> Result<usize> {
    let sum = total
        .checked_add(value)
        .ok_or(KnowledgeSettingsError::LimitExceeded(label))?;
    bounded(sum, cap, label)
}

fn input(stats: &mut SettingsStats, raw: &str, limits: KnowledgeSettingsLimits) -> Result<()> {
    stats.input_bytes = added(
        stats.input_bytes,
        raw.len(),
        limits.max_input_bytes,
        "max_input_bytes",
    )?;
    Ok(())
}

fn trimmed<'a>(context: &SnapshotContext, value: &'a str) -> Result<&'a str> {
    check(context)?;
    let trimmed = value.trim();
    check(context)?;
    Ok(trimmed)
}

fn count_criteria<'a>(
    context: &SnapshotContext,
    criteria: impl IntoIterator<Item = &'a str>,
    cap: NonZeroUsize,
    label: &'static str,
) -> Result<usize> {
    let mut count = 0;
    for _ in criteria {
        check(context)?;
        count = added(count, 1, cap, label)?;
    }
    Ok(count)
}

fn validate_path(
    context: &SnapshotContext,
    value: &str,
    origin: SettingOrigin,
    stats: &mut SettingsStats,
    limits: KnowledgeSettingsLimits,
) -> Result<()> {
    if value.is_empty() {
        return Err(invalid(
            KnowledgeSetting::DocsDir,
            origin,
            SettingsProblem::Empty,
            None,
        ));
    }
    stats.path_components = count_criteria(
        context,
        value.split('/'),
        limits.max_path_components,
        "max_path_components",
    )?;
    for part in value.split('/') {
        check(context)?;
        if part.is_empty() || (value != "." && matches!(part, "." | "..")) {
            return Err(invalid(
                KnowledgeSetting::DocsDir,
                origin,
                SettingsProblem::InvalidPath,
                None,
            ));
        }
        for ch in part.chars() {
            check(context)?;
            if matches!(ch, '\\' | ':') || ch.is_control() {
                return Err(invalid(
                    KnowledgeSetting::DocsDir,
                    origin,
                    SettingsProblem::InvalidPath,
                    None,
                ));
            }
        }
    }
    Ok(())
}

fn copy_text(
    context: &SnapshotContext,
    value: &str,
    stats: &mut SettingsStats,
    limits: KnowledgeSettingsLimits,
) -> Result<String> {
    check(context)?;
    stats.projection_bytes = added(
        stats.projection_bytes,
        value.len(),
        limits.max_projection_bytes,
        "max_projection_bytes",
    )?;
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(KnowledgeSettingsError::Allocation)?;
    check(context)?;
    output.push_str(value);
    check(context)?;
    Ok(output)
}

fn equal_suffix(context: &SnapshotContext, normalized: &str, suffix: &str) -> Result<bool> {
    check(context)?;
    let stored = &normalized.as_bytes()[1..];
    if stored.len() != suffix.len() {
        return Ok(false);
    }
    for (left, right) in stored.iter().zip(suffix.bytes()) {
        check(context)?;
        if *left != right.to_ascii_lowercase() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn extension(
    context: &SnapshotContext,
    raw: &str,
    origin: SettingOrigin,
    output: &mut Vec<String>,
    stats: &mut SettingsStats,
    limits: KnowledgeSettingsLimits,
) -> Result<()> {
    check(context)?;
    let token = trimmed(context, raw)?;
    if token.is_empty() {
        return Ok(());
    }
    let suffix = token.strip_prefix('.').unwrap_or(token);
    let bytes = added(
        suffix.len(),
        1,
        limits.max_extension_bytes,
        "max_extension_bytes",
    )?;
    if suffix.is_empty() {
        return Err(invalid(
            KnowledgeSetting::Extensions,
            origin,
            SettingsProblem::InvalidExtension,
            None,
        ));
    }
    for ch in suffix.chars() {
        check(context)?;
        if matches!(ch, '.' | '/' | '\\' | ':') || ch.is_control() || ch.is_whitespace() {
            return Err(invalid(
                KnowledgeSetting::Extensions,
                origin,
                SettingsProblem::InvalidExtension,
                None,
            ));
        }
    }
    for stored in output.iter() {
        if equal_suffix(context, stored, suffix)? {
            return Ok(());
        }
    }
    stats.extensions = added(stats.extensions, 1, limits.max_extensions, "max_extensions")?;
    stats.projection_bytes = added(
        stats.projection_bytes,
        bytes,
        limits.max_projection_bytes,
        "max_projection_bytes",
    )?;
    check(context)?;
    let mut normalized = String::new();
    normalized
        .try_reserve_exact(bytes)
        .map_err(KnowledgeSettingsError::Allocation)?;
    normalized.push('.');
    for ch in suffix.chars() {
        check(context)?;
        normalized.push(ch.to_ascii_lowercase());
    }
    check(context)?;
    output
        .try_reserve(1)
        .map_err(KnowledgeSettingsError::Allocation)?;
    output.push(normalized);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::{brand::Brand, config::ConfigDocument, file_lock::Cancellation};
    use std::{
        ffi::OsString,
        time::{Duration, Instant},
    };

    fn cap(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }

    fn limits() -> KnowledgeSettingsLimits {
        KnowledgeSettingsLimits {
            max_input_bytes: cap(10_000),
            max_docs_dir_bytes: cap(1_000),
            max_path_components: cap(100),
            max_extension_tokens: cap(100),
            max_extensions: cap(100),
            max_extension_bytes: cap(100),
            max_projection_bytes: cap(10_000),
        }
    }

    fn context() -> SnapshotContext {
        SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        )
    }

    fn key(value: &str) -> ConfigKey {
        ConfigKey::new(value).unwrap()
    }

    fn texts(source: ConfigSource, values: &[(&str, &str)]) -> ConfigDocument {
        ConfigDocument::new(
            &Brand::default(),
            source,
            values
                .iter()
                .map(|(name, value)| (key(name), (*value).into())),
        )
        .unwrap()
    }

    fn boolean(source: ConfigSource, value: bool) -> ConfigDocument {
        ConfigDocument::new(
            &Brand::default(),
            source,
            [(key("knowledge.include_readme"), value.into())],
        )
        .unwrap()
    }

    fn snapshot(layers: impl IntoIterator<Item = ConfigDocument>) -> ConfigSnapshot {
        ConfigSnapshot::new(&Brand::default(), layers).unwrap()
    }

    fn resolve(snapshot: &ConfigSnapshot) -> KnowledgeSettings {
        KnowledgeSettings::resolve(&context(), snapshot, None, limits()).unwrap()
    }

    fn invalid_value(
        result: Result<KnowledgeSettings>,
        field: KnowledgeSetting,
        origin: SettingOrigin,
        reason: SettingsProblem,
        kind: io::ErrorKind,
    ) -> KnowledgeSettingsError {
        let error = result.unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(matches!(&error, KnowledgeSettingsError::InvalidValue {
            field: actual_field, origin: actual_origin, reason: actual_reason, ..
        } if *actual_field == field && *actual_origin == origin && *actual_reason == reason));
        error
    }

    fn limit_error(result: Result<KnowledgeSettings>, label: &'static str) {
        let error = result.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert!(matches!(error, KnowledgeSettingsError::LimitExceeded(actual) if actual == label));
    }

    #[test]
    fn absent_values_use_public_defaults_with_real_absent_origins_and_no_io() {
        let context = context();
        let config = snapshot([]);
        let view = KnowledgeSettings::resolve(&context, &config, None, limits()).unwrap();
        assert_eq!(view.docs_dir(), "docs");
        assert!(view.include_readme());
        assert_eq!(
            view.extensions(),
            [
                ".md",
                ".markdown",
                ".mdx",
                ".txt",
                ".adoc",
                ".rst",
                ".puml",
                ".plantuml",
                ".yaml",
                ".yml",
                ".json",
                ".proto",
                ".graphql",
                ".gql",
                ".wsdl",
                ".xml"
            ]
        );
        for field in [
            KnowledgeSetting::DocsDir,
            KnowledgeSetting::IncludeReadme,
            KnowledgeSetting::Extensions,
        ] {
            assert_eq!(view.origin(field), SettingOrigin::Default);
        }
        assert_eq!(view.stats().input_bytes, 0);
        assert_eq!(view.stats().path_components, 1);
        assert_eq!(view.stats().extension_tokens, 16);
        assert_eq!(view.stats().extensions, 16);
        assert_eq!(view.stats().projection_bytes, 88);
    }

    #[test]
    fn each_field_uses_the_highest_present_layer_and_retains_its_origin() {
        let order = [
            ConfigSource::Distribution,
            ConfigSource::Global,
            ConfigSource::Project,
            ConfigSource::Environment,
            ConfigSource::Command,
        ];
        let paths = [
            "distribution",
            "global",
            "project",
            "environment",
            "command",
        ];
        for winner in 0..order.len() {
            let config = snapshot(
                (0..=winner)
                    .map(|index| texts(order[index], &[("knowledge.docs_dir", paths[index])])),
            );
            let view = resolve(&config);
            assert_eq!(view.docs_dir(), paths[winner]);
            assert_eq!(
                view.origin(KnowledgeSetting::DocsDir),
                SettingOrigin::Config(order[winner])
            );
        }
        let config = snapshot([
            texts(ConfigSource::Command, &[("knowledge.docs_dir", "manual")]),
            boolean(ConfigSource::Project, false),
            texts(
                ConfigSource::Global,
                &[("knowledge.extensions", "yaml,json")],
            ),
        ]);
        let view = resolve(&config);
        assert_eq!(view.docs_dir(), "manual");
        assert!(!view.include_readme());
        assert_eq!(view.extensions(), [".yaml", ".json"]);
        assert_eq!(
            view.origin(KnowledgeSetting::IncludeReadme),
            SettingOrigin::Config(ConfigSource::Project)
        );
        assert_eq!(
            view.origin(KnowledgeSetting::Extensions),
            SettingOrigin::Config(ConfigSource::Global)
        );
    }

    #[test]
    fn winner_is_presence_based_and_invalid_values_never_fall_back() {
        let false_wins = snapshot([
            boolean(ConfigSource::Distribution, true),
            boolean(ConfigSource::Command, false),
        ]);
        assert!(!resolve(&false_wins).include_readme());
        let invalid_wins = snapshot([
            boolean(ConfigSource::Global, true),
            texts(
                ConfigSource::Command,
                &[("knowledge.include_readme", "true")],
            ),
        ]);
        invalid_value(
            KnowledgeSettings::resolve(&context(), &invalid_wins, None, limits()),
            KnowledgeSetting::IncludeReadme,
            SettingOrigin::Config(ConfigSource::Command),
            SettingsProblem::ExpectedBoolean,
            io::ErrorKind::InvalidInput,
        );
        let empty_wins = snapshot([
            texts(ConfigSource::Global, &[("knowledge.docs_dir", "valid")]),
            texts(ConfigSource::Command, &[("knowledge.docs_dir", "  ")]),
        ]);
        invalid_value(
            KnowledgeSettings::resolve(&context(), &empty_wins, None, limits()),
            KnowledgeSetting::DocsDir,
            SettingOrigin::Config(ConfigSource::Command),
            SettingsProblem::Empty,
            io::ErrorKind::InvalidInput,
        );
    }

    #[test]
    fn invalid_values_report_field_origin_kind_and_sanitized_core_source() {
        for source in [
            ConfigSource::Command,
            ConfigSource::Environment,
            ConfigSource::Project,
            ConfigSource::Global,
            ConfigSource::Distribution,
        ] {
            let config = texts(
                source,
                &[("knowledge.include_readme", "private-config-marker")],
            );
            let config = snapshot([config]);
            let kind = if matches!(source, ConfigSource::Project | ConfigSource::Global) {
                io::ErrorKind::InvalidData
            } else {
                io::ErrorKind::InvalidInput
            };
            let error = invalid_value(
                KnowledgeSettings::resolve(&context(), &config, None, limits()),
                KnowledgeSetting::IncludeReadme,
                SettingOrigin::Config(source),
                SettingsProblem::ExpectedBoolean,
                kind,
            );
            assert!(!error.to_string().contains("private-config-marker"));
            assert!(
                !error
                    .source()
                    .unwrap()
                    .to_string()
                    .contains("private-config-marker")
            );
        }
        let invalid = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Project,
            [(key("knowledge.docs_dir"), true.into())],
        )
        .unwrap();
        invalid_value(
            KnowledgeSettings::resolve(&context(), &snapshot([invalid]), None, limits()),
            KnowledgeSetting::DocsDir,
            SettingOrigin::Config(ConfigSource::Project),
            SettingsProblem::ExpectedText,
            io::ErrorKind::InvalidData,
        );
    }

    #[test]
    fn index_path_overrides_only_its_field_without_consulting_discarded_values() {
        let invalid_docs = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Project,
            [(key("knowledge.docs_dir"), false.into())],
        )
        .unwrap();
        let config = snapshot([invalid_docs]);
        for path in [".", "文档/café"] {
            let view =
                KnowledgeSettings::resolve(&context(), &config, Some(path), limits()).unwrap();
            assert_eq!(view.docs_dir(), path);
            assert_eq!(
                view.origin(KnowledgeSetting::DocsDir),
                SettingOrigin::IndexPath
            );
            assert_eq!(view.stats().input_bytes, path.len());
        }
        invalid_value(
            KnowledgeSettings::resolve(&context(), &config, Some(" "), limits()),
            KnowledgeSetting::DocsDir,
            SettingOrigin::IndexPath,
            SettingsProblem::Empty,
            io::ErrorKind::InvalidInput,
        );
        let invalid_other = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.include_readme", "false")],
        )]);
        invalid_value(
            KnowledgeSettings::resolve(&context(), &invalid_other, Some("."), limits()),
            KnowledgeSetting::IncludeReadme,
            SettingOrigin::Config(ConfigSource::Project),
            SettingsProblem::ExpectedBoolean,
            io::ErrorKind::InvalidData,
        );
    }

    #[test]
    fn paths_preserve_unicode_case_and_internal_spaces_as_configuration_data() {
        for (raw, expected) in [
            (" \u{2003}.\u{2003} ", "."),
            (" 文档/Guide space/e\u{301} ", "文档/Guide space/e\u{301}"),
            ("CON/trailing. / name ", "CON/trailing. / name"),
        ] {
            let config = snapshot([texts(ConfigSource::Project, &[("knowledge.docs_dir", raw)])]);
            assert_eq!(resolve(&config).docs_dir(), expected);
        }
        for raw in [
            "",
            " \t\n",
            "/docs",
            "../docs",
            "docs/..",
            "docs/./guide",
            "docs//guide",
            "docs/",
            "docs\\guide",
            "C:/docs",
            "\\\\host\\docs",
            "docs\0guide",
            "docs/\u{85}guide",
        ] {
            let config = snapshot([texts(ConfigSource::Project, &[("knowledge.docs_dir", raw)])]);
            let error =
                KnowledgeSettings::resolve(&context(), &config, None, limits()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{raw:?}");
            assert!(!matches!(error, KnowledgeSettingsError::Context(_)));
        }
    }

    #[test]
    fn csv_counts_empty_and_duplicate_tokens_and_keeps_first_normalized_order() {
        let config = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.extensions", ".MD, yaml,MD,,.JSON")],
        )]);
        let view = resolve(&config);
        assert_eq!(view.extensions(), [".md", ".yaml", ".json"]);
        assert_eq!(view.stats().extension_tokens, 5);
        assert_eq!(view.stats().extensions, 3);
        for raw in ["", " , , "] {
            let config = snapshot([texts(
                ConfigSource::Project,
                &[("knowledge.extensions", raw)],
            )]);
            invalid_value(
                KnowledgeSettings::resolve(&context(), &config, None, limits()),
                KnowledgeSetting::Extensions,
                SettingOrigin::Config(ConfigSource::Project),
                SettingsProblem::Empty,
                io::ErrorKind::InvalidData,
            );
        }
    }

    #[test]
    fn extension_suffixes_reject_nontext_and_multicomponent_values() {
        for raw in [
            ".", "..md", "tar.gz", "a/b", "a\\b", "a:b", "m d", "m\td", "m\0d",
        ] {
            let config = snapshot([texts(
                ConfigSource::Project,
                &[("knowledge.extensions", raw)],
            )]);
            invalid_value(
                KnowledgeSettings::resolve(&context(), &config, None, limits()),
                KnowledgeSetting::Extensions,
                SettingOrigin::Config(ConfigSource::Project),
                SettingsProblem::InvalidExtension,
                io::ErrorKind::InvalidData,
            );
        }
        let array = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Command,
            [(key("knowledge.extensions"), vec!["md"].into())],
        )
        .unwrap();
        let number = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Command,
            [(key("knowledge.extensions"), 7.into())],
        )
        .unwrap();
        let boolean = ConfigDocument::new(
            &Brand::default(),
            ConfigSource::Command,
            [(key("knowledge.extensions"), true.into())],
        )
        .unwrap();
        for layer in [array, number, boolean] {
            invalid_value(
                KnowledgeSettings::resolve(&context(), &snapshot([layer]), None, limits()),
                KnowledgeSetting::Extensions,
                SettingOrigin::Config(ConfigSource::Command),
                SettingsProblem::ExpectedText,
                io::ErrorKind::InvalidInput,
            );
        }
    }

    #[test]
    fn suffix_case_fold_is_ascii_only_and_does_not_normalize_unicode() {
        let config = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.extensions", ".MÉ,mÉ,é,É,e\u{301},.MD")],
        )]);
        let view = resolve(&config);
        assert_eq!(view.extensions(), [".mÉ", ".é", ".É", ".e\u{301}", ".md"]);
        assert_eq!(view.stats().extension_tokens, 6);
        assert_eq!(view.stats().extensions, 5);
    }

    #[test]
    fn all_seven_caps_accept_exact_values_and_reject_one_under() {
        let config = snapshot([texts(
            ConfigSource::Environment,
            &[
                ("knowledge.docs_dir", " docs/sub "),
                ("knowledge.include_readme", " false "),
                ("knowledge.extensions", " .MD,md,,.JSON "),
            ],
        )]);
        let exact = KnowledgeSettingsLimits {
            max_input_bytes: cap(32),
            max_docs_dir_bytes: cap(10),
            max_path_components: cap(2),
            max_extension_tokens: cap(4),
            max_extensions: cap(2),
            max_extension_bytes: cap(5),
            max_projection_bytes: cap(16),
        };
        let view = KnowledgeSettings::resolve(&context(), &config, None, exact).unwrap();
        assert!(!view.include_readme());
        assert_eq!(view.docs_dir(), "docs/sub");
        assert_eq!(view.extensions(), [".md", ".json"]);
        assert_eq!(
            view.stats(),
            SettingsStats {
                input_bytes: 32,
                path_components: 2,
                extension_tokens: 4,
                extensions: 2,
                projection_bytes: 16
            }
        );
        for (label, reduced) in [
            (
                "max_input_bytes",
                KnowledgeSettingsLimits {
                    max_input_bytes: cap(31),
                    ..exact
                },
            ),
            (
                "max_docs_dir_bytes",
                KnowledgeSettingsLimits {
                    max_docs_dir_bytes: cap(9),
                    ..exact
                },
            ),
            (
                "max_path_components",
                KnowledgeSettingsLimits {
                    max_path_components: cap(1),
                    ..exact
                },
            ),
            (
                "max_extension_tokens",
                KnowledgeSettingsLimits {
                    max_extension_tokens: cap(3),
                    ..exact
                },
            ),
            (
                "max_extensions",
                KnowledgeSettingsLimits {
                    max_extensions: cap(1),
                    ..exact
                },
            ),
            (
                "max_extension_bytes",
                KnowledgeSettingsLimits {
                    max_extension_bytes: cap(4),
                    ..exact
                },
            ),
            (
                "max_projection_bytes",
                KnowledgeSettingsLimits {
                    max_projection_bytes: cap(15),
                    ..exact
                },
            ),
        ] {
            limit_error(
                KnowledgeSettings::resolve(&context(), &config, None, reduced),
                label,
            );
        }
    }

    #[test]
    fn raw_boolean_and_override_bytes_are_admitted_before_conversion_or_trim() {
        let config = snapshot([texts(
            ConfigSource::Project,
            &[
                ("knowledge.include_readme", "true"),
                ("knowledge.docs_dir", "discarded"),
            ],
        )]);
        let context = context();
        let under = KnowledgeSettingsLimits {
            max_input_bytes: cap(6),
            ..limits()
        };
        // Override contributes three bytes; the invalid string winner contributes
        // four before boolean validation, with no discarded docs contribution.
        limit_error(
            KnowledgeSettings::resolve(&context, &config, Some(" . "), under),
            "max_input_bytes",
        );
        let exact = KnowledgeSettingsLimits {
            max_input_bytes: cap(7),
            ..limits()
        };
        invalid_value(
            KnowledgeSettings::resolve(&context, &config, Some(" . "), exact),
            KnowledgeSetting::IncludeReadme,
            SettingOrigin::Config(ConfigSource::Project),
            SettingsProblem::ExpectedBoolean,
            io::ErrorKind::InvalidData,
        );
    }

    #[test]
    fn default_and_raw_duplicate_criteria_do_not_bypass_caps() {
        let absent = snapshot([]);
        limit_error(
            KnowledgeSettings::resolve(
                &context(),
                &absent,
                None,
                KnowledgeSettingsLimits {
                    max_extension_tokens: cap(15),
                    ..limits()
                },
            ),
            "max_extension_tokens",
        );
        let config = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.extensions", "md,,MD,md,")],
        )]);
        let view = KnowledgeSettings::resolve(
            &context(),
            &config,
            None,
            KnowledgeSettingsLimits {
                max_extension_tokens: cap(5),
                max_extensions: cap(1),
                max_extension_bytes: cap(3),
                max_projection_bytes: cap(7),
                ..limits()
            },
        )
        .unwrap();
        assert_eq!(view.extensions(), [".md"]);
        assert_eq!(view.stats().extension_tokens, 5);
        assert_eq!(view.stats().projection_bytes, 7);
        limit_error(
            KnowledgeSettings::resolve(
                &context(),
                &config,
                None,
                KnowledgeSettingsLimits {
                    max_extension_tokens: cap(4),
                    ..limits()
                },
            ),
            "max_extension_tokens",
        );
        limit_error(
            KnowledgeSettings::resolve(
                &context(),
                &config,
                None,
                KnowledgeSettingsLimits {
                    max_extension_bytes: cap(2),
                    ..limits()
                },
            ),
            "max_extension_bytes",
        );
        assert!(matches!(
            added(usize::MAX, 1, cap(usize::MAX), "max_input_bytes"),
            Err(KnowledgeSettingsError::LimitExceeded("max_input_bytes"))
        ));
        // Raw preflight covers the entire criterion sequence even when its first
        // component/token is invalid; syntax must not hide an over-budget plan.
        let malformed_path = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.docs_dir", "../a/b")],
        )]);
        limit_error(
            KnowledgeSettings::resolve(
                &context(),
                &malformed_path,
                None,
                KnowledgeSettingsLimits {
                    max_path_components: cap(2),
                    ..limits()
                },
            ),
            "max_path_components",
        );
        let malformed_csv = snapshot([texts(
            ConfigSource::Project,
            &[("knowledge.extensions", "a/b,md,txt")],
        )]);
        limit_error(
            KnowledgeSettings::resolve(
                &context(),
                &malformed_csv,
                None,
                KnowledgeSettingsLimits {
                    max_extension_tokens: cap(2),
                    ..limits()
                },
            ),
            "max_extension_tokens",
        );
    }

    #[test]
    fn cancelled_or_expired_original_context_fails_without_io() {
        let config = snapshot([]);
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let cancelled =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancellation);
        let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
        for (context, expected) in [
            (&cancelled, io::ErrorKind::Interrupted),
            (&expired, io::ErrorKind::TimedOut),
        ] {
            let error = KnowledgeSettings::resolve(context, &config, None, limits()).unwrap_err();
            assert_eq!(error.kind(), expected);
            assert!(error.source().is_some());
        }
    }

    #[test]
    fn original_request_cancellation_is_preserved() {
        let original = context();
        let config = snapshot([]);
        KnowledgeSettings::resolve(&original, &config, None, limits()).unwrap();
        original.cancellation().cancel();
        assert_eq!(
            KnowledgeSettings::resolve(&original, &config, None, limits())
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn actual_environment_capture_uses_brand_names_and_core_boolean_types() {
        let brand = Brand::new("acme-code").unwrap();
        let fields = [
            key("knowledge.docs_dir"),
            key("knowledge.include_readme"),
            key("knowledge.extensions"),
        ];
        assert_eq!(
            fields[0].environment_variable(&brand),
            "ACME_CODE_KNOWLEDGE__DOCS_DIR"
        );
        for (raw, expected) in [(" true ", true), (" false\n", false)] {
            let environment = ConfigSnapshot::capture_environment(
                &brand,
                &fields,
                [
                    (
                        fields[0].environment_variable(&brand).into(),
                        OsString::from("文档"),
                    ),
                    (
                        fields[1].environment_variable(&brand).into(),
                        OsString::from(raw),
                    ),
                    (
                        fields[2].environment_variable(&brand).into(),
                        OsString::from("JSON,YAML"),
                    ),
                ],
            )
            .unwrap();
            let config = ConfigSnapshot::new(&brand, [environment]).unwrap();
            let view = resolve(&config);
            assert_eq!(view.docs_dir(), "文档");
            assert_eq!(view.include_readme(), expected);
            assert_eq!(view.extensions(), [".json", ".yaml"]);
            assert_eq!(
                view.origin(KnowledgeSetting::DocsDir),
                SettingOrigin::Config(ConfigSource::Environment)
            );
            assert_eq!(
                view.stats().input_bytes,
                "文档".len() + raw.len() + "JSON,YAML".len()
            );
        }
        for raw in ["TRUE", "1", "yes", "", "\"true\""] {
            let environment = ConfigSnapshot::capture_environment(
                &brand,
                &fields,
                [(
                    fields[1].environment_variable(&brand).into(),
                    OsString::from(raw),
                )],
            )
            .unwrap();
            let config = ConfigSnapshot::new(&brand, [environment]).unwrap();
            invalid_value(
                KnowledgeSettings::resolve(&context(), &config, None, limits()),
                KnowledgeSetting::IncludeReadme,
                SettingOrigin::Config(ConfigSource::Environment),
                SettingsProblem::ExpectedBoolean,
                io::ErrorKind::InvalidInput,
            );
        }
    }

    #[test]
    fn unrelated_semantically_invalid_public_setting_is_not_resolved() {
        let config = snapshot([texts(ConfigSource::Project, &[("modules.ast", "unusable")])]);
        assert_eq!(resolve(&config).docs_dir(), "docs");
    }

    #[cfg(unix)]
    #[test]
    fn requested_non_utf8_environment_fails_in_core_without_lossy_conversion() {
        use std::os::unix::ffi::OsStringExt;
        let brand = Brand::default();
        let fields = [key("knowledge.docs_dir")];
        let error = ConfigSnapshot::capture_environment(
            &brand,
            &fields,
            [(
                fields[0].environment_variable(&brand).into(),
                OsString::from_vec(vec![0xff]),
            )],
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        let environment = ConfigSnapshot::capture_environment(
            &brand,
            &fields,
            [
                ("UNREQUESTED".into(), OsString::from_vec(vec![0xff])),
                (fields[0].environment_variable(&brand).into(), "docs".into()),
            ],
        )
        .unwrap();
        assert_eq!(
            resolve(&ConfigSnapshot::new(&brand, [environment]).unwrap()).docs_dir(),
            "docs"
        );
    }
}
