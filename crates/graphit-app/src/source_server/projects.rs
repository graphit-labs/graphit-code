//! Authorized local discovery. Inventory, checkout observations and live membership are distinct.
use super::*;
use graphit_auth::{
    AuthenticatedCaller, Obligation, Permission, ProjectReadModule, ProjectRecordAuthority,
};
use graphit_core::{
    native_path::NativePath, project_identity::ProjectIdentityStore,
    project_inventory::GlobalProjectInventory, version_context::Checkout,
};
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;

pub(super) const LIST: &str = "graphit_projects_list";
pub(super) const GET: &str = "graphit_projects_get";
pub(super) const LABEL_SET: &str = "graphit_projects_labels_set";
pub(super) const LABEL_UNSET: &str = "graphit_projects_labels_unset";
pub(super) fn definitions() -> [Tool; 4] {
    let properties = json!({
        "project_id":{"type":"string"}, "name":{"type":"string"},
        "version":{"type":"string"}, "online":{"type":"boolean"},
        "include_paths":{"type":"boolean","default":true},
        "page_size":{"type":"integer","minimum":1,"default":25},
        "cursor":{"type":"string"}
    });
    [LIST, GET, LABEL_SET, LABEL_UNSET].map(|name| {
        if matches!(name, LABEL_SET | LABEL_UNSET) {
            let required = if name == LABEL_SET {
                json!(["project_id", "project_dir", "key", "value"])
            } else {
                json!(["project_id", "project_dir", "key"])
            };
            return Tool::new(name,
                if name == LABEL_SET { "Add a label value to an admitted local checkout. Requires project administration." } else { "Remove a label value or, with value omitted, the entire key. Requires project administration." },
                json!({"type":"object","properties":{
                    "project_id":{"type":"string"},"project_dir":{"type":"string"},
                    "project_version":{"type":"string"},"key":{"type":"string"},"value":{"type":"string"}
                },"required":required,"additionalProperties":false}).as_object().unwrap().clone());
        }
        let mut properties = properties.clone();
        if name == LIST {
            properties["label_key"] = json!({"type":"string","description":"Exact local label key"});
            properties["label_value"] = json!({"type":"string","description":"Exact value of label_key"});
        }
        Tool::new(name,
        "Discover authorized local PROJECT inventory, retained versions and checkouts. Historical registration is not online presence or access. GET requires an exact ID or unique name. Cursor invalidates when the observed authorized catalogue changes; selectors grant no access.",
        json!({"type":"object","properties":properties,"additionalProperties":false}).as_object().unwrap().clone())
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    project_id: Option<ProjectId>,
    name: Option<String>,
    version: Option<String>,
    online: Option<bool>,
    label_key: Option<String>,
    label_value: Option<String>,
    #[serde(default = "paths_default")]
    include_paths: bool,
    #[serde(default = "page_default")]
    page_size: NonZeroUsize,
    cursor: Option<String>,
}
fn paths_default() -> bool {
    true
}
fn page_default() -> NonZeroUsize {
    NonZeroUsize::new(25).unwrap()
}
fn failed(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"code":code}))
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let arguments = Value::Object(call.arguments.unwrap_or_default());
    let input = match call.name.as_ref() {
        LABEL_SET | LABEL_UNSET => match serde_json::from_value::<LabelArguments>(arguments) {
            Ok(input) if input.operation.is_none() => Operation::Labels(
                input,
                if call.name == LABEL_SET {
                    LabelOperation::Set
                } else {
                    LabelOperation::Unset
                },
            ),
            _ => return failed("invalid_arguments"),
        },
        _ => match serde_json::from_value(arguments) {
            Ok(input) => Operation::Read(input, call.name == GET),
            Err(_) => return failed("invalid_arguments"),
        },
    };
    let mutation = matches!(&input, Operation::Labels(..));
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failed("credential_required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failed("native_credential_required"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return failed("credential_required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let state = Arc::clone(&server.0);
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let work_cancel = cancel.clone();
    let deadline = Instant::now() + state.policy.timeout;
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = state.jobs.lock().await;
        jobs.reap();
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = execute(&owner, input, &credential, local, &work_cancel, deadline);
            let _ = send.send(result);
        });
    }
    let result = tokio::select! {
        result = &mut receive => Some(result),
        () = context.ct.cancelled() => None,
        () = state.stop.cancelled() => None,
        () = tokio::time::sleep_until(deadline.into()) => None,
    };
    let result = match result {
        Some(result) => result,
        None if mutation => {
            cancel.cancel();
            receive.await
        }
        None => return failed("request_cancelled_or_expired"),
    };
    match result {
        Ok(Ok(_))
            if !mutation
                && (context.ct.is_cancelled()
                    || state.stop.is_cancelled()
                    || Instant::now() >= deadline) =>
        {
            failed("request_cancelled_or_expired")
        }
        Ok(Ok(value)) if value["status"] == "partial" => crate::mcp_output::typed_error(value),
        Ok(Ok(value)) => crate::mcp_output::typed(value),
        Ok(Err(error)) => failed(error),
        Err(_) => failed("operation_failed"),
    }
}
enum Operation {
    Read(Arguments, bool),
    Labels(LabelArguments, LabelOperation),
}
fn execute(
    state: &State,
    input: Operation,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<Value, &'static str> {
    let request = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|_| "request_cancelled_or_expired")?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, &request)
    } else {
        issuer.authenticate_runtime(credential, &request)
    }
    .map_err(|_| "authentication_or_permission_denied")?;
    match input {
        Operation::Read(input, get) => read_inventory(state, input, get, &caller, &request),
        Operation::Labels(input, operation) => {
            edit_labels(state, input, operation, &caller, &request)
        }
    }
}
fn valid_scalar(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && !value.contains('\0') && value.len() <= limit
}
pub(super) fn browser_list(
    state: &State,
    bytes: &[u8],
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<Value, &'static str> {
    let input = serde_json::from_slice(bytes).map_err(|_| "invalid_arguments")?;
    read_inventory(state, input, false, caller, request)
}
fn read_inventory(
    state: &State,
    input: Arguments,
    get: bool,
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<Value, &'static str> {
    if input.page_size.get() > state.policy.content.max_collection_entries
        || (get
            && (input.project_id.is_none() && input.name.is_none()
                || input.cursor.is_some()
                || input.label_key.is_some()))
        || (input.label_value.is_some() && input.label_key.is_none())
        || [
            &input.name,
            &input.version,
            &input.label_key,
            &input.label_value,
        ]
        .into_iter()
        .flatten()
        .any(|v| !valid_scalar(v, state.policy.content.max_scalar_bytes))
    {
        return Err("invalid_arguments");
    }
    let issuer = state.auth.issuer();
    // Configure permits installation inventory administration; ordinary readers
    // see only per-PROJECT admitted entries. Neither path mints a project grant.
    let administrator = matches!(
        caller.authorize(Permission::Configure),
        Ok(Obligation::Unconditional)
    );
    let inventory = GlobalProjectInventory::at(
        &state.policy.installation,
        state.policy.native_install.hub.content().max_input_bytes,
    )
    .and_then(|store| store.load(request.snapshot()))
    .map_err(|_| "inventory_unavailable")?;
    let live = state
        .live_projects
        .snapshot()
        .map_err(|_| "selection_unavailable")?;
    let authority = ProjectRecordAuthority::new(issuer);
    let mut result = Vec::new();
    for entry in inventory.projects() {
        request
            .check()
            .map_err(|_| "request_cancelled_or_expired")?;
        let id = entry.identity().id();
        let metadata = entry.identity().metadata();
        let name = &metadata.name;
        if input.project_id.is_some_and(|v| v != id)
            || input.name.as_ref().is_some_and(|v| {
                if get {
                    v != name
                } else {
                    !name.to_lowercase().contains(&v.to_lowercase())
                }
            })
            || input.label_key.as_deref().is_some_and(|key| {
                metadata.cluster.get(key.trim()).is_none_or(|values| {
                    input
                        .label_value
                        .as_deref()
                        .is_some_and(|value| !values.iter().any(|v| v == value.trim()))
                })
            })
        {
            continue;
        }
        let mut checkouts = Vec::new();
        let mut versions = std::collections::BTreeSet::new();
        let mut online = false;
        for checkout in entry.checkouts() {
            let paths = ProjectPaths::at(checkout.path(), &state.policy.installation);
            let admitted = |version: &str| -> bool {
                let Ok(paths) = &paths else {
                    return false;
                };
                if paths.root() != checkout.path() {
                    return false;
                }
                let Ok(version) =
                    VersionContext::select(paths, Checkout::NonGit, version, None, "")
                else {
                    return false;
                };
                let Ok(scope) = PublicationScope::project(paths, id) else {
                    return false;
                };
                let scope = ScopeInput {
                    installation: state.auth.installation(),
                    project: Some(ProjectSelection {
                        scope: &scope,
                        version: &version,
                    }),
                    providers: &[],
                    binding: state.policy.binding,
                    global_version: "",
                };
                [
                    ProjectReadModule::Task,
                    ProjectReadModule::Memory,
                    ProjectReadModule::Knowledge,
                    ProjectReadModule::Ast,
                    ProjectReadModule::Hub,
                ]
                .into_iter()
                .any(|module| {
                    authority
                        .admit_authenticated_read(caller, request, scope, module)
                        .is_ok()
                })
            };
            for version in entry.versions() {
                if input.version.as_deref().is_none_or(|v| v == version)
                    && (administrator || admitted(version))
                {
                    versions.insert(version.to_owned());
                }
            }
            if !administrator && !admitted(checkout.selected_version()) {
                continue;
            }
            let active = live
                .projects
                .iter()
                .any(|(p, i, _)| *i == id && p.root() == checkout.path());
            online |= active;
            let selected = live
                .current
                .and_then(|v| live.projects.get(v))
                .is_some_and(|(p, i, _)| *i == id && p.root() == checkout.path());
            let exists = match std::fs::metadata(checkout.path()) {
                Ok(v) => Some(v.is_dir()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Some(false),
                Err(_) => None,
            };
            let active_versions = live
                .projects
                .iter()
                .filter(|(paths, project, _)| *project == id && paths.root() == checkout.path())
                .map(|(_, _, version)| version.version())
                .collect::<Vec<_>>();
            let current_version = live
                .current
                .and_then(|index| live.projects.get(index))
                .filter(|(paths, project, _)| *project == id && paths.root() == checkout.path())
                .map(|(_, _, version)| version.version());
            let mut value = json!({"exists":exists,"online":active,"current":selected,"active_versions":active_versions,"current_version":current_version,"registered_version":checkout.selected_version(),"default_version":checkout.default_version(),"selection_source":checkout.source(),"checkout":checkout.checkout()});
            if input.include_paths {
                value["dir"] = serde_json::to_value(
                    NativePath::new(checkout.path().to_owned())
                        .map_err(|_| "invalid_native_path")?,
                )
                .map_err(|_| "response_invalid")?;
            }
            checkouts.push(value);
        }
        if administrator && entry.checkouts().len() == 0 {
            versions.extend(
                entry
                    .versions()
                    .filter(|v| input.version.as_deref().is_none_or(|wanted| wanted == *v))
                    .map(str::to_owned),
            );
        }
        if versions.is_empty() && checkouts.is_empty() && !administrator {
            continue;
        }
        if input.version.is_some() && versions.is_empty()
            || input.online.is_some_and(|v| v != online)
        {
            continue;
        }
        result.push(json!({"project_id":id,"name":name,"description":metadata.description,"cluster":metadata.cluster,"registered":true,"online":online,"versions":versions,"checkouts":checkouts}));
    }
    caller
        .check()
        .map_err(|_| "authentication_or_permission_denied")?;
    request
        .check()
        .map_err(|_| "request_cancelled_or_expired")?;
    if get {
        let value = match result.len() {
            0 => return Err("project_not_found_or_denied"),
            1 => json!({"project":result.remove(0)}),
            _ => return Err("project_name_ambiguous"),
        };
        if serde_json::to_vec(&value)
            .map_err(|_| "response_invalid")?
            .len()
            > state.policy.content.max_output_bytes
        {
            return Err("response_limit");
        }
        return Ok(value);
    }
    // Cursor binds the authorized projection and filters. Changes fail explicitly
    // instead of silently skipping/duplicating rows across a mutable inventory.
    let binding = json!({"rows":result,"name":input.name,"id":input.project_id,"version":input.version,"online":input.online,"label_key":input.label_key,"label_value":input.label_value,"paths":input.include_paths,"size":input.page_size});
    let digest = Sha256::digest(serde_json::to_vec(&binding).map_err(|_| "response_invalid")?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let offset = match input.cursor {
        None => 0,
        Some(cursor) => {
            let (generation, offset) = cursor.split_once(':').ok_or("invalid_cursor")?;
            if generation != digest {
                return Err("cursor_expired");
            }
            offset.parse::<usize>().map_err(|_| "invalid_cursor")?
        }
    };
    if offset > result.len() {
        return Err("invalid_cursor");
    }
    let end = offset
        .saturating_add(input.page_size.get())
        .min(result.len());
    let next = (end < result.len()).then(|| format!("{digest}:{end}"));
    let value = json!({"generation":digest,"projects":&result[offset..end],"next_cursor":next});
    if serde_json::to_vec(&value)
        .map_err(|_| "response_invalid")?
        .len()
        > state.policy.content.max_output_bytes
    {
        return Err("response_limit");
    }
    Ok(value)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LabelOperation {
    Set,
    Unset,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelArguments {
    operation: Option<LabelOperation>,
    project_id: ProjectId,
    project_dir: NativePath,
    project_version: Option<String>,
    key: String,
    value: Option<String>,
}

pub(super) fn browser_labels(
    state: &State,
    bytes: &[u8],
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<Value, &'static str> {
    let input: LabelArguments = serde_json::from_slice(bytes).map_err(|_| "invalid_arguments")?;
    let operation = input.operation.ok_or("invalid_arguments")?;
    edit_labels(state, input, operation, caller, request)
}

fn edit_labels(
    state: &State,
    input: LabelArguments,
    operation: LabelOperation,
    caller: &AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<Value, &'static str> {
    let limit = state.policy.content.max_scalar_bytes;
    if !valid_scalar(&input.key, limit)
        || matches!(operation, LabelOperation::Set) && input.value.is_none()
        || input
            .value
            .as_deref()
            .is_some_and(|value| !valid_scalar(value, limit))
        || input
            .project_version
            .as_deref()
            .is_some_and(|version| !valid_scalar(version, limit))
    {
        return Err("invalid_arguments");
    }
    let issuer = state.auth.issuer();
    let session = issuer
        .bootstrap_admin(caller)
        .map_err(|_| "authentication_or_permission_denied")?;
    let parent = issuer
        .admin_parent(caller, &session)
        .map_err(|_| "authentication_or_permission_denied")?;
    parent
        .authorize(Permission::Configure)
        .map_err(|_| "authentication_or_permission_denied")?;

    let live = state
        .live_projects
        .snapshot()
        .map_err(|_| "selection_unavailable")?;
    let mut candidates = live.projects.iter().filter(|(paths, id, version)| {
        *id == input.project_id
            && paths.root() == input.project_dir.as_path()
            && input
                .project_version
                .as_deref()
                .is_none_or(|wanted| wanted == version.version())
    });
    let (paths, id, version) = candidates.next().ok_or("checkout_not_active_or_denied")?;
    if candidates.next().is_some() {
        return Err(if input.project_version.is_none() {
            "project_version_required"
        } else {
            "checkout_selection_ambiguous"
        });
    }
    let scope =
        PublicationScope::project(paths, *id).map_err(|_| "checkout_not_active_or_denied")?;
    let selection = ProjectSelection {
        scope: &scope,
        version,
    };
    parent
        .authorize_project_publication(selection)
        .map_err(|_| "checkout_not_active_or_denied")?;

    let inventory = GlobalProjectInventory::at(
        &state.policy.installation,
        state.policy.native_install.hub.content().max_input_bytes,
    )
    .map_err(|_| "inventory_unavailable")?;
    let mut inventory_writer = inventory
        .writer(request.snapshot())
        .map_err(|_| "inventory_unavailable")?;
    let recorded = inventory
        .load(request.snapshot())
        .map_err(|_| "inventory_unavailable")?;
    if !recorded.get(*id).is_some_and(|project| {
        project
            .checkouts()
            .any(|checkout| checkout.path() == paths.root())
    }) {
        return Err("checkout_not_registered");
    }
    let remaining = request
        .snapshot()
        .required_deadline()
        .map_err(|_| "request_cancelled_or_expired")?
        .checked_duration_since(Instant::now())
        .ok_or("request_cancelled_or_expired")?;
    let current_paths = ProjectPaths::at(paths.root(), &state.policy.installation)
        .map_err(|_| "checkout_not_active_or_denied")?;
    if current_paths.root() != paths.root() {
        return Err("checkout_not_active_or_denied");
    }
    let store = ProjectIdentityStore::at(&current_paths);
    let mut writer = store
        .writer(remaining, request.cancellation())
        .map_err(|_| "project_identity_unavailable")?;
    let identity = store
        .load()
        .map_err(|_| "project_identity_unavailable")?
        .filter(|identity| identity.id() == *id && identity.allows_persistent_stores())
        .ok_or("checkout_not_active_or_denied")?;
    let mut metadata = identity.metadata().clone();
    let key = input.key.trim();
    let value = input.value.as_deref().map(str::trim);
    match (operation, value) {
        (LabelOperation::Set, Some(value)) => {
            let values = metadata.cluster.entry(key.to_owned()).or_default();
            if !values.iter().any(|existing| existing == value) {
                values.push(value.to_owned());
            }
        }
        (LabelOperation::Unset, Some(value)) => {
            if let Some(values) = metadata.cluster.get_mut(key) {
                values.retain(|existing| existing != value);
                if values.is_empty() {
                    metadata.cluster.remove(key);
                }
            }
        }
        (LabelOperation::Unset, None) => {
            metadata.cluster.remove(key);
        }
        (LabelOperation::Set, None) => return Err("invalid_arguments"),
    }
    let updated = state
        .live_projects
        .with_active(paths, *id, version, || {
            request
                .check()
                .map_err(|_| "request_cancelled_or_expired")?;
            parent
                .authorize_project_publication(selection)
                .map_err(|_| "checkout_not_active_or_denied")?;
            writer
                .update(*id, &metadata)
                .map_err(|error| match error.kind() {
                    io::ErrorKind::InvalidInput => "invalid_label_metadata",
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
                        "request_cancelled_or_expired"
                    }
                    _ => "project_identity_write_failed",
                })
        })
        .map_err(|_| "selection_unavailable")?
        .ok_or("checkout_not_active_or_denied")??;
    let document_changed = updated != identity;
    match inventory_writer.refresh_project_metadata(paths, &updated) {
        Ok(inventory_changed) => Ok(json!({
            "status":"ok","project_id":id,"document_changed":document_changed,
            "inventory_changed":inventory_changed
        })),
        Err(_) => Ok(json!({
            "status":"partial","code":"inventory_refresh_failed","project_id":id,
            "document_changed":document_changed
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_auth::Permission;
    use graphit_core::{
        brand::Brand, module_settings::Module, project_inventory::ProjectInventoryObservation,
    };
    use graphit_hub::NativeBinding;

    fn register(f: &super::super::test_fixture::Fixture, request: &RequestContext<'_>) {
        let inventory = GlobalProjectInventory::at(&f.paths, 65536).unwrap();
        for (paths, id, version) in f.projects.iter() {
            let checkout = graphit_core::git_checkout::GitCheckout::inspect(
                paths.root(),
                Duration::from_secs(5),
                request.cancellation(),
            )
            .unwrap();
            let observation = ProjectInventoryObservation::observe(
                paths,
                *id,
                &checkout,
                version,
                request.snapshot(),
            )
            .unwrap();
            inventory
                .writer(request.snapshot())
                .unwrap()
                .register_checkout(&observation)
                .unwrap();
        }
    }
    fn input(value: Value) -> Arguments {
        serde_json::from_value(value).unwrap()
    }
    fn execute(
        state: &State,
        input: Arguments,
        get: bool,
        credential: &str,
        local: bool,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Value, &'static str> {
        super::execute(
            state,
            Operation::Read(input, get),
            credential,
            local,
            cancel,
            deadline,
        )
    }
    #[tokio::test]
    async fn inventory_discovery_rechecks_native_authority_paths_paging_and_live_presence() {
        let f = super::super::test_fixture::fixture_with_projects(
            &[Permission::Read(Module::Task)],
            Brand::default(),
            NativeBinding::Codex,
            &["first", "second"],
        );
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let request = RequestContext::new(f.server.0.policy.content, &cancel, deadline).unwrap();
        register(&f, &request);
        let first = execute(
            &f.server.0,
            input(json!({"page_size":1})),
            false,
            &f.raw,
            false,
            &cancel,
            deadline,
        )
        .unwrap();
        assert_eq!(first["projects"].as_array().unwrap().len(), 1);
        assert!(first["projects"][0]["checkouts"][0]["dir"].is_string());
        let second = execute(
            &f.server.0,
            input(json!({"page_size":1,"cursor":first["next_cursor"]})),
            false,
            &f.raw,
            false,
            &cancel,
            deadline,
        )
        .unwrap();
        assert_ne!(
            first["projects"][0]["project_id"],
            second["projects"][0]["project_id"]
        );
        assert!(second["next_cursor"].is_null());
        let id = first["projects"][0]["project_id"].clone();
        let shown = execute(
            &f.server.0,
            input(json!({"project_id":id,"include_paths":false})),
            true,
            &f.raw,
            false,
            &cancel,
            deadline,
        )
        .unwrap();
        assert_eq!(shown["project"]["online"], true);
        assert!(shown["project"]["checkouts"][0].get("dir").is_none());
        assert_eq!(
            execute(
                &f.server.0,
                input(json!({})),
                false,
                "wrong credential",
                false,
                &cancel,
                deadline
            )
            .unwrap_err(),
            "authentication_or_permission_denied"
        );
        f.server
            .0
            .live_projects
            .replace(Arc::new(Vec::new()), 0)
            .unwrap();
        assert_eq!(
            execute(
                &f.server.0,
                input(json!({"page_size":1,"cursor":first["next_cursor"]})),
                false,
                &f.raw,
                false,
                &cancel,
                deadline
            )
            .unwrap_err(),
            "cursor_expired"
        );
        let offline = execute(
            &f.server.0,
            input(json!({"project_id":id})),
            true,
            &f.raw,
            false,
            &cancel,
            deadline,
        )
        .unwrap();
        assert_eq!(offline["project"]["registered"], true);
        assert_eq!(offline["project"]["online"], false);
        f.server.shutdown().await.unwrap();
        let denied =
            super::super::test_fixture::fixture(&[], Brand::default(), NativeBinding::Codex);
        register(&denied, &request);
        let hidden = execute(
            &denied.server.0,
            input(json!({})),
            false,
            &denied.raw,
            false,
            &cancel,
            deadline,
        )
        .unwrap();
        assert!(hidden["projects"].as_array().unwrap().is_empty());
        assert!(
            execute(
                &denied.server.0,
                input(json!({"project_id":denied.projects[0].1})),
                true,
                &denied.raw,
                false,
                &cancel,
                deadline
            )
            .is_err()
        );
        denied.server.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn browser_mutation_checks_authority_and_exact_checkout() {
        let f = super::super::test_fixture::fixture(
            &[Permission::Read(Module::Task)],
            Brand::default(),
            NativeBinding::Codex,
        );
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            f.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        register(&f, &request);
        let caller = f
            .auth
            .issuer()
            .authenticate_runtime(&f.raw, &request)
            .unwrap();
        let (paths, id, _) = &f.projects[0];
        let body = serde_json::to_vec(&json!({
            "operation":"set","project_id":id,"project_dir":paths.root(),"key":"team","value":"frontend"
        })).unwrap();
        assert_eq!(
            browser_labels(&f.server.0, &body, &caller, &request).unwrap_err(),
            "authentication_or_permission_denied"
        );
        let listed =
            browser_list(&f.server.0, br#"{"label_key":"team"}"#, &caller, &request).unwrap();
        assert_eq!(listed["projects"], json!([]));
        assert!(
            ProjectIdentityStore::at(paths)
                .load()
                .unwrap()
                .unwrap()
                .metadata()
                .cluster
                .is_empty()
        );
        f.server.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn root_http_announces_and_dispatches_discovery_with_real_credentials() {
        let f = super::super::test_fixture::fixture_with_projects_and_authority(
            &[Permission::Read(Module::Task)],
            &[
                Permission::Configure,
                Permission::External(graphit_auth::ExternalEffect::PublishedFilesystem),
            ],
            Brand::default(),
            NativeBinding::Codex,
            &["first", "second"],
            &[],
        );
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            f.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        register(&f, &request);
        let socket = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancel,
        )
        .unwrap();
        let address = socket.address();
        let server = f.server.clone();
        let serving = tokio::spawn(async move { server.serve(socket).await });
        let client = crate::source_server::test_http_client();
        let connection =
            crate::ast_source::connect_source(&f.paths, address, &client, &request, ())
                .await
                .unwrap();
        let tools = connection.peer().list_all_tools().await.unwrap();
        for name in [LIST, GET, LABEL_SET, LABEL_UNSET] {
            assert!(tools.iter().any(|tool| tool.name == name));
        }
        let result = connection
            .peer()
            .call_tool(
                CallToolRequestParams::new(GET).with_arguments(
                    json!({"project_id":f.projects[0].1,"ai_optimized":false})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true), "{result:?}");
        assert_eq!(
            result.structured_content.as_ref().unwrap()["project"]["project_id"],
            json!(f.projects[0].1)
        );
        let call = |name: &'static str, mut input: Value| {
            input["ai_optimized"] = json!(false);
            connection.peer().call_tool(
                CallToolRequestParams::new(name).with_arguments(input.as_object().unwrap().clone()),
            )
        };
        let (paths, project_id, version) = &f.projects[0];
        let key = "service";
        let target = json!({"project_id":project_id,"project_dir":paths.root(),"key":key,"project_version":version.version()});
        let mut wrong = target.clone();
        wrong["project_dir"] = json!(f.projects[1].0.root());
        wrong["value"] = json!("api");
        let denied = call(LABEL_SET, wrong).await.unwrap();
        assert_eq!(denied.is_error, Some(true), "{denied:?}");

        let first_page = call(LIST, json!({"page_size":1}))
            .await
            .unwrap()
            .structured_content
            .unwrap();
        for (value, changed) in [(" api ", true), ("worker", true), ("api", false)] {
            let mut set = target.clone();
            set["value"] = json!(value);
            let receipt = call(LABEL_SET, set).await.unwrap();
            assert_ne!(receipt.is_error, Some(true), "{receipt:?}");
            assert_eq!(
                receipt.structured_content.unwrap()["document_changed"],
                changed
            );
        }
        let stale = call(
            LIST,
            json!({"page_size":1,"cursor":first_page["next_cursor"]}),
        )
        .await
        .unwrap();
        assert_eq!(stale.is_error, Some(true), "{stale:?}");
        let matched = call(
            LIST,
            json!({"label_key":key,"label_value":"api","include_paths":false}),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
        assert_eq!(matched["projects"].as_array().unwrap().len(), 1);
        let project = &matched["projects"][0];
        assert_eq!(project["project_id"], json!(project_id));
        assert_eq!(project["cluster"][key], json!(["api", "worker"]));
        assert_eq!(project["description"], "");
        assert_eq!(
            project["checkouts"][0]["registered_version"],
            version.version()
        );
        assert!(project["checkouts"][0].get("dir").is_none());
        let empty = call(LIST, json!({"label_key":key,"label_value":"API"}))
            .await
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(empty["projects"], json!([]));

        let mut unset = target.clone();
        unset["value"] = json!("api");
        let receipt = call(LABEL_UNSET, unset).await.unwrap();
        assert_ne!(receipt.is_error, Some(true), "{receipt:?}");
        let stored = ProjectIdentityStore::at(paths).load().unwrap().unwrap();
        assert_eq!(stored.metadata().cluster[key], ["worker"]);
        let receipt = call(LABEL_UNSET, target).await.unwrap();
        assert_ne!(receipt.is_error, Some(true), "{receipt:?}");
        let empty = call(LIST, json!({"label_key":key}))
            .await
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(empty["projects"], json!([]));
        connection.cancel().await.unwrap();
        f.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}
