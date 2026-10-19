//! Actual shipped/user/PROJECT profile layers for the foreground Raw producer.
use crate::foreground_ast_sources::scip::{ScipCompiler, ScipFamily, config::CustomParsers};
use graphit_ast::{
    catalog::{
        CatalogDraft, CatalogLimits, CatalogLimitsInput, FileSelector, ProfileLayer,
        ProfileLayerKind, Selection, SelectionPolicy, SelectionSkip,
    },
    profiles::{
        DecodedProfile, Presence, ProfileDecoder, ProfileId, ProfileInput, ProfileLimits,
        ProfileLimitsInput,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, CopyPolicy, SnapshotContext, SourceReadLimits},
    config::{ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::{HubResourcePolicy, ResourceSetting};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) fn project_config(
    installation: &InstallationPaths,
    project: &ProjectPaths,
    base: &ConfigSnapshot,
    original: &SnapshotContext,
) -> io::Result<ConfigSnapshot> {
    original.check()?;
    let layers = [
        ConfigSource::Command,
        ConfigSource::Environment,
        ConfigSource::Distribution,
    ]
    .into_iter()
    .filter_map(|kind| base.layer(kind).cloned())
    .chain([
        ConfigStore::global(installation)?.load()?,
        ConfigStore::project(project)?.load()?,
    ]);
    let config = ConfigSnapshot::new(base.brand(), layers)?;
    original.check()?;
    Ok(config)
}
pub(super) fn queries_directory(
    project: &ProjectPaths,
    config: &ConfigSnapshot,
) -> io::Result<PathBuf> {
    let configured = config
        .get::<String>(&ConfigKey::new("ast.queries_dir")?)?
        .unwrap_or_else(|| format!("{}/ast/queries", project.brand().directory()));
    let path = PathBuf::from(configured);
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_)))
        || path.as_os_str().as_encoded_bytes().contains(&0)
        || path.as_os_str().as_encoded_bytes().len() > graphit_core::native_path::MAX_PATH_BYTES
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(path)
}
pub(super) struct AstProfileLoader {
    stock: Vec<DecodedProfile>,
    policy: HubResourcePolicy,
}
pub(super) struct LoadedProfiles {
    catalog: CatalogDraft,
    selection: SelectionPolicy,
    overrides: BTreeMap<String, String>,
    native_only: BTreeSet<String>,
    custom: CustomParsers,
    pub selectors: Vec<String>,
    guards: Vec<CapturedSourceReader>,
}
#[derive(Debug)]
struct ProfileDecodeFailure {
    profile: String,
    code: graphit_ast::profiles::ProfileErrorCode,
    span: Option<graphit_ast::scalar_policy::OriginalByteSpan>,
    schema_path: Option<&'static str>,
    indices: [Option<u32>; 4],
}
impl std::fmt::Display for ProfileDecodeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "profile {} {:?}, span {:?}, field {:?}, indices {:?}",
            self.profile, self.code, self.span, self.schema_path, self.indices
        )
    }
}
impl std::error::Error for ProfileDecodeFailure {}
fn err(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(e)
}
impl AstProfileLoader {
    pub fn new(policy: HubResourcePolicy, original: &Arc<SnapshotContext>) -> io::Result<Self> {
        let mut loader = Self {
            stock: Vec::new(),
            policy,
        };
        for (name, bytes) in STOCK {
            original.check()?;
            loader.stock.push(loader.decode(name, bytes, original)?);
        }
        Ok(loader)
    }
    fn decode(
        &self,
        name: &str,
        bytes: &[u8],
        original: &SnapshotContext,
    ) -> io::Result<DecodedProfile> {
        let number = |setting| usize::try_from(self.policy.limit(setting).value).map_err(err);
        let decoder = ProfileDecoder::new(
            ScalarPolicy::new(RegexLimits {
                compiled_size_policy_bytes: number(ResourceSetting::ContentScalarBytes)?,
                dfa_cache_policy_bytes: number(ResourceSetting::ContentScalarBytes)?,
            })
            .map_err(err)?,
        );
        let id = ProfileId::new(name, number(ResourceSetting::ContentScalarBytes)?).map_err(err)?;
        let limits = ProfileLimits::new(ProfileLimitsInput {
            input_bytes: number(ResourceSetting::ContentInputBytes)?,
            nodes: number(ResourceSetting::ContentNodes)?,
            depth: number(ResourceSetting::ContentDepth)?,
            text_bytes: number(ResourceSetting::ContentInputBytes)?,
            scalar_bytes: number(ResourceSetting::ContentScalarBytes)?,
            collection_entries: number(ResourceSetting::ContentCollectionEntries)?,
        })
        .map_err(err)?;
        decoder
            .decode(ProfileInput { id: &id, bytes }, &limits, original)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    ProfileDecodeFailure {
                        profile: id.as_str().to_owned(),
                        code: error.code(),
                        span: error.span(),
                        schema_path: error.schema_path(),
                        indices: error.indices(),
                    },
                )
            })
    }
    fn directory(
        &self,
        path: &Path,
        layer: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<(Vec<DecodedProfile>, Vec<CapturedSourceReader>)> {
        original.check()?;
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
            Err(e) => return Err(e),
        };
        let mut paths = Vec::new();
        for entry in entries {
            original.check()?;
            let entry = entry?;
            if entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("yaml") || s.eq_ignore_ascii_case("yml"))
            {
                if !entry.file_type()?.is_file() {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                paths.push(entry.path());
            }
        }
        paths.sort();
        let max = usize::try_from(self.policy.limit(ResourceSetting::ContentInputBytes).value)
            .map_err(err)?;
        let mut profiles = Vec::new();
        let mut guards = Vec::new();
        for path in paths {
            let reader = CapturedSourceReader::capture(
                &path,
                graphit_fs::NodeKind::File,
                &self
                    .policy
                    .snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
                SourceReadLimits::new(max, 64 * 1024, max as u64)?,
                Arc::clone(original),
            )?;
            let file = reader.open_file(Path::new(""))?;
            let bytes = file.read_range(0, usize::try_from(file.len()).map_err(err)?)?;
            reader.validate_file_bytes(Path::new(""), bytes.as_ref(), original)?;
            let name = format!(
                "{layer}/{}",
                path.file_name()
                    .and_then(|p| p.to_str())
                    .ok_or(io::ErrorKind::InvalidData)?
            );
            match self.decode(&name, bytes.as_ref(), original) {
                Ok(profile) => profiles.push(profile),
                Err(e) => eprintln!("AST profile {name}: {e}"),
            }
            drop(bytes);
            drop(file);
            guards.push(reader);
        }
        original.check()?;
        Ok((profiles, guards))
    }
    pub fn load(
        &self,
        installation: &InstallationPaths,
        project: &ProjectPaths,
        config: &ConfigSnapshot,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<LoadedProfiles> {
        original.check()?;
        let runtime_dir = installation
            .framework_runtime(env!("GRAPHIT_COMPILED_BUILD_VERSION"))?
            .join("ast/queries");
        let (mut runtime, mut guards) = self.directory(&runtime_dir, "runtime", original)?;
        if !runtime.is_empty() {
            // Installed runtime defaults replace the matching compiled stock language.
            for (name, bytes) in STOCK {
                let stock = self.decode(name, bytes, original)?;
                let language_id = language(&stock)?;
                if !runtime
                    .iter()
                    .any(|p| language(p).is_ok_and(|p| p.eq_ignore_ascii_case(language_id)))
                {
                    runtime.push(stock);
                }
            }
        }
        let (user, user_guards) = self.directory(
            &installation.global_path(&["ast", "queries"])?,
            "user",
            original,
        )?;
        let (project_profiles, project_guards) = self.directory(
            &project.root().join(queries_directory(project, config)?),
            "project",
            original,
        )?;
        guards.extend(user_guards);
        guards.extend(project_guards);
        let limits = CatalogLimits::new(CatalogLimitsInput {
            policy_entries: usize::try_from(
                self.policy
                    .limit(ResourceSetting::ContentCollectionEntries)
                    .value,
            )
            .map_err(err)?,
            selector_bytes: usize::try_from(
                self.policy.limit(ResourceSetting::ContentScalarBytes).value,
            )
            .map_err(err)?,
        })
        .map_err(err)?;
        let catalog = CatalogDraft::build(
            &[
                ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: if runtime.is_empty() {
                        &self.stock
                    } else {
                        &runtime
                    },
                },
                ProfileLayer {
                    kind: ProfileLayerKind::User,
                    profiles: &user,
                },
                ProfileLayer {
                    kind: ProfileLayerKind::Project,
                    profiles: &project_profiles,
                },
            ],
            &limits,
            original,
        )
        .map_err(err)?;
        let list = |key: &str| -> io::Result<Vec<String>> {
            Ok(config
                .get::<String>(&ConfigKey::new(key)?)?
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect())
        };
        let whitelist = list("ast.grammars_whitelist")?;
        let blacklist = list("ast.grammars_blacklist")?;
        // Grammar filters have no profile-free family aliases in the current catalog.
        let allow_native_only = whitelist.is_empty() && blacklist.is_empty();
        let selection =
            SelectionPolicy::new(whitelist, blacklist, &limits, original).map_err(err)?;
        let mut overrides = BTreeMap::new();
        for pair in list("ast.grammar")? {
            if let Some((ext, grammar)) = pair.split_once('=') {
                let ext = ext.trim();
                let grammar = grammar.trim();
                if ext.is_empty() || grammar.is_empty() {
                    continue;
                }
                let ext = if ext.starts_with('.') {
                    ext.to_ascii_lowercase()
                } else {
                    format!(".{}", ext.to_ascii_lowercase())
                };
                overrides.insert(ext, grammar.to_owned());
            }
        }
        let mut selectors = Vec::new();
        for profile in catalog.profiles() {
            for selector in profile.extensions().value().iter() {
                let route = selector.value();
                let extension = route.starts_with('.').then_some(route.as_str());
                let explicit = overrides
                    .get(&route.to_ascii_lowercase())
                    .map(String::as_str);
                let selected = catalog.resolve_file(
                    FileSelector {
                        basename: if extension.is_some() { "" } else { route },
                        extension,
                        explicit_grammar: explicit,
                    },
                    &selection,
                    original,
                );
                match selected {
                    Ok(Selection::Candidates(_)) => selectors.push(route.clone()),
                    Ok(Selection::Skip(_)) => {}
                    Err(e) => return Err(err(e)),
                }
            }
        }
        for (ext, grammar) in &overrides {
            if matches!(
                catalog
                    .resolve_file(
                        FileSelector {
                            basename: "",
                            extension: Some(ext),
                            explicit_grammar: Some(grammar)
                        },
                        &selection,
                        original
                    )
                    .map_err(err)?,
                Selection::Candidates(_)
            ) {
                selectors.push(ext.clone());
            }
        }
        let mut native_only = BTreeSet::new();
        if allow_native_only {
            for extension in ScipFamily::native_extensions() {
                original.check()?;
                // A replaced stock route or an exclusive custom profile remains authoritative.
                let has_syntax_route = self.stock.iter().any(|profile| {
                    matches!(profile.patch().extensions(), Presence::Set(routes)
                        if routes.value().iter().any(|route| route.value().eq_ignore_ascii_case(extension)))
                }) || catalog.profiles().iter().any(|profile| {
                    profile.extensions().value().iter().any(|route| route.value().eq_ignore_ascii_case(extension))
                });
                if !has_syntax_route && !overrides.contains_key(extension) {
                    native_only.insert(extension.to_owned());
                    selectors.push(extension.to_owned());
                }
            }
        }
        let custom = CustomParsers::resolve(config)?;
        selectors.extend(custom.extensions().map(str::to_owned));
        selectors.sort();
        selectors.dedup();
        let result = LoadedProfiles {
            catalog,
            selection,
            overrides,
            native_only,
            custom,
            selectors,
            guards,
        };
        result.revalidate(original)?;
        Ok(result)
    }
}
fn language(profile: &DecodedProfile) -> io::Result<&str> {
    match profile.patch().language() {
        Presence::Set(text) => Ok(text.value()),
        _ => Err(io::ErrorKind::InvalidData.into()),
    }
}
impl LoadedProfiles {
    /// Borrow the effective inventory after revalidating the retained profile sources.
    pub fn profiles(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<&[graphit_ast::catalog::EffectiveProfile]> {
        self.revalidate(original)?;
        Ok(self.catalog.profiles())
    }

    pub fn allows(&self, path: &Path, original: &SnapshotContext) -> io::Result<bool> {
        if self.custom.get(path).is_some() {
            original.check()?;
            return Ok(true);
        }
        match self.resolve(path, original)? {
            Selection::Candidates(_) => Ok(true),
            Selection::Skip(SelectionSkip::NoSelector) => {
                Ok(self.unselected_native_family(path, original)?.is_some())
            }
            Selection::Skip(SelectionSkip::Filtered) => Ok(false),
        }
    }
    pub fn native_scip_compiler(
        &self,
        path: &Path,
        original: &SnapshotContext,
    ) -> io::Result<Option<ScipCompiler>> {
        if let Some(image) = self.custom.get(path) {
            original.check()?;
            return Ok(Some(ScipCompiler::CustomImage(image.to_owned())));
        }
        if !matches!(
            self.resolve(path, original)?,
            Selection::Skip(SelectionSkip::NoSelector)
        ) {
            return Ok(None);
        }
        Ok(self
            .unselected_native_family(path, original)?
            .map(ScipCompiler::Maintained))
    }
    pub fn scip_compiler(
        &self,
        path: &Path,
        language: Option<&str>,
        original: &SnapshotContext,
    ) -> io::Result<Option<ScipCompiler>> {
        original.check()?;
        Ok(match self.custom.get(path) {
            Some(image) => Some(ScipCompiler::CustomImage(image.to_owned())),
            None => ScipFamily::for_path_profile(
                path.to_str().ok_or(io::ErrorKind::InvalidData)?,
                language,
            )
            .map(ScipCompiler::Maintained),
        })
    }
    pub fn selected_scip_compiler(
        &self,
        path: &Path,
        original: &SnapshotContext,
    ) -> io::Result<Option<ScipCompiler>> {
        if let Some(image) = self.custom.get(path) {
            original.check()?;
            return Ok(Some(ScipCompiler::CustomImage(image.to_owned())));
        }
        if let Selection::Candidates(profiles) = self.resolve(path, original)? {
            for profile in profiles {
                if let Some(compiler) =
                    self.scip_compiler(path, Some(profile.language().value()), original)?
                {
                    return Ok(Some(compiler));
                }
            }
            return Ok(None);
        }
        self.native_scip_compiler(path, original)
    }
    fn unselected_native_family(
        &self,
        path: &Path,
        original: &SnapshotContext,
    ) -> io::Result<Option<ScipFamily>> {
        original.check()?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_ascii_lowercase()));
        let family = if extension
            .as_ref()
            .is_some_and(|value| self.native_only.contains(value))
        {
            ScipFamily::for_path_profile(path.to_str().ok_or(io::ErrorKind::InvalidData)?, None)
        } else {
            None
        };
        original.check()?;
        Ok(family)
    }
    pub fn resolve<'p>(
        &'p self,
        path: &Path,
        original: &SnapshotContext,
    ) -> io::Result<Selection<'p>> {
        original.check()?;
        let basename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(io::ErrorKind::InvalidData)?;
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{s}"));
        let explicit = extension
            .as_ref()
            .and_then(|s| self.overrides.get(&s.to_ascii_lowercase()))
            .map(String::as_str);
        let selection = self
            .catalog
            .resolve_file(
                FileSelector {
                    basename,
                    extension: extension.as_deref(),
                    explicit_grammar: explicit,
                },
                &self.selection,
                original,
            )
            .map_err(|cause| match original.check() {
                Err(error) => io::Error::new(error.kind(), cause),
                Ok(()) => err(cause),
            })?;
        original.check()?;
        Ok(selection)
    }
    pub fn revalidate(&self, original: &Arc<SnapshotContext>) -> io::Result<()> {
        original.check()?;
        for guard in &self.guards {
            guard.check_request(original)?;
            guard.revalidate_content()?;
        }
        original.check()
    }
}

const STOCK: &[(&str, &[u8])] = &[
    (
        "stock/bash.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/bash.yaml"),
    ),
    (
        "stock/c.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/c.yaml"),
    ),
    (
        "stock/clojure.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/clojure.yaml"),
    ),
    (
        "stock/cobol85.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/cobol85.yaml"),
    ),
    (
        "stock/cpp.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/cpp.yaml"),
    ),
    (
        "stock/csharp.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/csharp.yaml"),
    ),
    (
        "stock/css.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/css.yaml"),
    ),
    (
        "stock/dart.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/dart.yaml"),
    ),
    (
        "stock/db2.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/db2.yaml"),
    ),
    (
        "stock/dockerfile.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/dockerfile.yaml"),
    ),
    (
        "stock/elixir.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/elixir.yaml"),
    ),
    (
        "stock/go.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/go.yaml"),
    ),
    (
        "stock/graphql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/graphql.yaml"),
    ),
    (
        "stock/groovy.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/groovy.yaml"),
    ),
    (
        "stock/haskell.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/haskell.yaml"),
    ),
    (
        "stock/hcl.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/hcl.yaml"),
    ),
    (
        "stock/html.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/html.yaml"),
    ),
    (
        "stock/java.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/java.yaml"),
    ),
    (
        "stock/javascript.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml"),
    ),
    (
        "stock/json.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/json.yaml"),
    ),
    (
        "stock/julia.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/julia.yaml"),
    ),
    (
        "stock/kotlin.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/kotlin.yaml"),
    ),
    (
        "stock/lua.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/lua.yaml"),
    ),
    (
        "stock/objc.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/objc.yaml"),
    ),
    (
        "stock/php.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/php.yaml"),
    ),
    (
        "stock/plpgsql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plpgsql.yaml"),
    ),
    (
        "stock/plsql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml"),
    ),
    (
        "stock/postgresql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/postgresql.yaml"),
    ),
    (
        "stock/protobuf.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/protobuf.yaml"),
    ),
    (
        "stock/python.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/python.yaml"),
    ),
    (
        "stock/r.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/r.yaml"),
    ),
    (
        "stock/ruby.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/ruby.yaml"),
    ),
    (
        "stock/rust.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/rust.yaml"),
    ),
    (
        "stock/scala.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/scala.yaml"),
    ),
    (
        "stock/sql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/sql.yaml"),
    ),
    (
        "stock/svelte.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/svelte.yaml"),
    ),
    (
        "stock/swift.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/swift.yaml"),
    ),
    (
        "stock/toml.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/toml.yaml"),
    ),
    (
        "stock/tsql.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/tsql.yaml"),
    ),
    (
        "stock/tsx.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/tsx.yaml"),
    ),
    (
        "stock/typescript.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/typescript.yaml"),
    ),
    (
        "stock/vue.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/vue.yaml"),
    ),
    (
        "stock/xml.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/xml.yaml"),
    ),
    (
        "stock/yaml.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/yaml.yaml"),
    ),
    (
        "stock/zig.yaml",
        include_bytes!("../../../crates/graphit-ast/profiles/queries/zig.yaml"),
    ),
];

#[cfg(test)]
#[path = "ast_profile_loader_tests.rs"]
mod tests;
