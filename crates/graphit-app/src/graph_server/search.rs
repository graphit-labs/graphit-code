//! Search over one real imported graph package, with independent AST/M admissions.
use super::*;
use arrow_array::{Array, FixedSizeBinaryArray, StringArray};
use futures_util::TryStreamExt;
use graphit_ast::{
    entity_search::{Entity, EntitySearch, Hit},
    native_nn_query::NativeNnMetric,
};
use graphit_auth::{
    AuthenticatedCaller, ImportedAstReadAdmission, ProjectReadAdmission, ServicePurpose,
    VerifiedRoleGrant, broker::oauth::web::AuthenticatedWebCaller,
};
use graphit_daemon::model_service::EmbedInput;
use serde_json::{Value, json};

/// Existing execution resources; these references carry no read admission.
pub(crate) struct Context<'a> {
    pub(crate) policy: &'a SourcePolicy,
    pub(crate) auth: &'a graphit_daemon::auth_startup::DaemonAuth,
    pub(crate) config: &'a graphit_core::config::ConfigSnapshot,
    pub(crate) local_models: &'a crate::source_server::LocalModelControlSlot,
    pub(crate) verifiers: Option<&'a graphit_daemon::broker_verifiers::BrokerVerifierCache>,
    pub(crate) models: &'a crate::source_server::SemanticModelSlot,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    Fts,
    #[default]
    Semantic,
    Vector,
    Hybrid,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Metric {
    L2,
    #[default]
    Cosine,
    Dot,
}
impl Metric {
    fn native(self) -> NativeNnMetric {
        match self {
            Self::L2 => NativeNnMetric::L2,
            Self::Cosine => NativeNnMetric::Cosine,
            Self::Dot => NativeNnMetric::Dot,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub query: String,
    #[serde(default)]
    pub mode: Mode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<usize>,
    #[serde(default)]
    pub metric: Metric,
    #[serde(
        default,
        deserialize_with = "crate::reference_explorer::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rerank: Option<bool>,
}
impl Input {
    pub fn validate(&self, policy: &SourcePolicy) -> Result<usize, ReadFailure> {
        let k = self.k.unwrap_or(10);
        if self.query.trim().is_empty()
            || self.query.contains('\0')
            || self.query.len() > policy.content.max_scalar_bytes
            || k == 0
            || k > policy.content.max_collection_entries
        {
            return Err(ReadFailure::BadInput);
        }
        Ok(k)
    }
}
fn column<'a, T: Array + 'static>(
    batch: &'a arrow_array::RecordBatch,
    name: &str,
) -> io::Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|value| value.as_any().downcast_ref())
        .ok_or_else(|| io::ErrorKind::InvalidData.into())
}

pub(super) async fn imported<'a>(
    source: &crate::source_server::SourceServer,
    graph: &graphit_ast::imported_graph::ImportedGraph<'_, '_>,
    read: &'a ImportedAstReadAdmission<'a>,
    input: Input,
) -> Result<Value, ReadFailure> {
    let original = read.request();
    let Some(pin) =
        EntitySearch::open(Arc::clone(graph.reader()?), Arc::clone(original.snapshot()))
            .await
            .map_err(ReadFailure::Query)?
    else {
        return Ok(json!({"status":"index_absent","results":[]}));
    };
    copied(
        source.ast_search_context().map_err(ReadFailure::Query)?,
        &pin,
        Authority::Imported(read),
        original,
        input,
    )
    .await
}

#[derive(Clone, Copy)]
pub(crate) enum Authority<'a> {
    Native(&'a AuthenticatedCaller<'a>, &'a ProjectReadAdmission<'a>),
    Web(&'a AuthenticatedWebCaller<'a>, &'a ProjectReadAdmission<'a>),
    Imported(&'a ImportedAstReadAdmission<'a>),
    Role(&'a VerifiedRoleGrant<'a>, &'a ProjectReadAdmission<'a>),
}
impl Authority<'_> {
    fn recheck(&self) -> Result<(), ReadFailure> {
        match self {
            Self::Native(_, read) | Self::Web(_, read) | Self::Role(_, read) => {
                read.recheck().map_err(|error| {
                    ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
                })
            }
            Self::Imported(read) => read
                .recheck()
                .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error))),
        }
    }
}
pub(crate) fn rerank_configuration(
    context: &Context<'_>,
    authority: Authority<'_>,
    original: &RequestContext<'_>,
    requested: Option<bool>,
) -> io::Result<(bool, graphit_core::config::ConfigSnapshot)> {
    let config = match authority {
        Authority::Imported(_) => context.config.clone(),
        Authority::Native(_, read) | Authority::Web(_, read) | Authority::Role(_, read) => {
            let selected = read.project().map_err(io::Error::other)?;
            let project = graphit_core::project_paths::ProjectPaths::at(
                selected.scope.live.owner_root(),
                &context.policy.installation,
            )?;
            crate::ast_profile_loader::project_config(
                &context.policy.installation,
                &project,
                context.config,
                original.snapshot(),
            )?
        }
    };
    let enabled = match requested {
        Some(enabled) => enabled,
        None => crate::search_rerank::configured(&config)?,
    };
    Ok((enabled, config))
}

pub(crate) async fn copied<'a>(
    context: Context<'_>,
    pin: &EntitySearch,
    authority: Authority<'a>,
    original: &'a RequestContext<'a>,
    input: Input,
) -> Result<Value, ReadFailure> {
    authority.recheck()?;
    input.validate(context.policy)?;
    let (rerank, config) = rerank_configuration(&context, authority, original, input.rerank)
        .map_err(ReadFailure::Query)?;
    let embedding = graphit_core::module_settings::ModuleSettings::resolve(&config)
        .map_err(ReadFailure::Query)?
        .enabled(graphit_core::module_settings::Module::Embedding);
    let query = input.query.clone();
    let mut response = copied_inner(&context, pin, authority, original, embedding, input).await?;
    if rerank {
        let rows = response
            .get_mut("results")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ReadFailure::Query(io::ErrorKind::InvalidData.into()))?;
        if !rows.is_empty() {
            let documents =
                rerank_documents(pin, rows, original, context.policy.content.max_input_bytes)
                    .await
                    .map_err(ReadFailure::Query)?;
            let mut retained = Vec::with_capacity(rows.len());
            let documents = rows
                .drain(..)
                .zip(documents)
                .filter_map(|(row, document)| {
                    document.inspect(|_document| {
                        retained.push(row);
                    })
                })
                .collect::<Vec<_>>();
            *rows = retained;
            if rows.is_empty() {
                authority.recheck()?;
                original.check().map_err(|_| ReadFailure::Context)?;
                return Ok(response);
            }
            authority.recheck()?;
            let issuer = context.auth.issuer();
            let mut service = match authority {
                Authority::Imported(read) => issuer.capture_imported_ast_rerank_service(read),
                Authority::Native(caller, _) => {
                    issuer.capture_service(caller, ServicePurpose::Rerank)
                }
                Authority::Role(grant, _) => {
                    issuer.capture_role_service(grant, ServicePurpose::Rerank)
                }
                Authority::Web(caller, read) => {
                    issuer.capture_web_rerank_service(caller, read, original)
                }
            }
            .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
            let control = context
                .local_models
                .read()
                .map_err(|_| {
                    ReadFailure::Query(io::Error::other("local model control state poisoned"))
                })?
                .clone();
            let documents = documents.iter().map(String::as_str).collect::<Vec<_>>();
            let ranked = crate::search_rerank::run(
                original.snapshot(),
                &mut service,
                control
                    .as_ref()
                    .map(|control| (control, context.auth.runtime().as_ref())),
                &config,
                &context.policy.native_install.hub,
                context.verifiers,
                &query,
                &documents,
            )
            .await
            .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
            pin.revalidate().map_err(ReadFailure::Query)?;
            authority.recheck()?;
            original.check().map_err(|_| ReadFailure::Context)?;
            apply_rerank(rows, ranked).map_err(ReadFailure::Query)?;
        }
    }
    pin.revalidate().map_err(ReadFailure::Query)?;
    authority.recheck()?;
    original.check().map_err(|_| ReadFailure::Context)?;
    Ok(response)
}

async fn rerank_documents(
    pin: &EntitySearch,
    rows: &[Value],
    original: &RequestContext<'_>,
    max_bytes: usize,
) -> io::Result<Vec<Option<String>>> {
    original.check()?;
    let identities = rows
        .iter()
        .map(|row| {
            Ok((
                row.get("uid")
                    .and_then(Value::as_str)
                    .ok_or(io::ErrorKind::InvalidData)?,
                row.get("data_key")
                    .and_then(Value::as_str)
                    .ok_or(io::ErrorKind::InvalidData)?,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let documents = pin.rerank_documents(&identities, max_bytes).await?;
    original.check()?;
    Ok(documents)
}

async fn copied_inner<'a>(
    context: &Context<'_>,
    pin: &EntitySearch,
    authority: Authority<'a>,
    original: &'a RequestContext<'a>,
    embedding: bool,
    input: Input,
) -> Result<Value, ReadFailure> {
    authority.recheck()?;
    let k = input.validate(context.policy)?;
    let lexical = if matches!(input.mode, Mode::Fts | Mode::Hybrid) {
        pin.query(&input.query, k)
            .await
            .map_err(ReadFailure::Query)?
    } else {
        Vec::new()
    };
    if matches!(input.mode, Mode::Fts) {
        return Ok(lexical_response(lexical));
    }
    if !embedding {
        return Ok(component_unavailable(input.mode, lexical, "model_disabled"));
    }
    if !pin.nn_present() {
        return Ok(component_unavailable(input.mode, lexical, "index_absent"));
    }
    let resources = context
        .models
        .read()
        .map_err(|_| ReadFailure::Query(io::Error::other("semantic model state poisoned")))?
        .clone();
    let Some(resources) = resources else {
        return Ok(component_unavailable(
            input.mode,
            lexical,
            "model_unavailable",
        ));
    };
    original.check().map_err(|_| ReadFailure::Context)?;
    authority.recheck()?;
    let issuer = context.auth.issuer();
    let mut service = match authority {
        Authority::Imported(read) => issuer.capture_imported_ast_embedding_service(read),
        Authority::Native(caller, _) => issuer.capture_service(caller, ServicePurpose::Embedding),
        Authority::Role(grant, _) => issuer.capture_role_service(grant, ServicePurpose::Embedding),
        Authority::Web(caller, read) => {
            issuer.capture_web_embedding_service(caller, read, original)
        }
    }
    .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
    let metadata = crate::embedding_service::metadata(&mut service, &resources.execution())
        .await
        .map_err(|error| ReadFailure::Query(error.into_io()))?;
    authority.recheck()?;
    pin.revalidate().map_err(ReadFailure::Query)?;
    let model = crate::ast_semantic::native_nn_model(&metadata)
        .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
    let Some(nn) = pin.nn(&model).await.map_err(ReadFailure::Query)? else {
        return Ok(component_unavailable(input.mode, lexical, "index_absent"));
    };
    let mut probe = nn.scan();
    probe.filter_expr(graphit_ast::corpus::vector_filter(&model));
    probe
        .project(&["uid"])
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?;
    probe
        .limit(Some(1), None)
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?;
    let mut rows = probe
        .try_into_stream()
        .await
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?;
    let mut empty = true;
    while let Some(batch) = rows
        .try_next()
        .await
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?
    {
        pin.revalidate().map_err(ReadFailure::Query)?;
        empty &= batch.num_rows() == 0;
    }
    if empty {
        return Ok(component_unavailable(input.mode, lexical, "index_empty"));
    }
    let embedded = resources
        .embed(
            &mut service,
            &metadata,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::CodeQuery,
                texts: vec![input.query.clone()],
                deadline: original
                    .snapshot()
                    .required_deadline()
                    .map_err(ReadFailure::Query)?,
            },
        )
        .await
        .map_err(|error| ReadFailure::Query(error.into_io()))?;
    let vector = embedded.vectors.first().ok_or(ReadFailure::BadInput)?;
    let matches = pin
        .query_nn(&model, vector, k, input.metric.native())
        .await
        .map_err(ReadFailure::Query)?
        .ok_or(ReadFailure::BadInput)?;
    let batches = matches
        .rows(original.snapshot())
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?;
    let distances = matches
        .distances(original.snapshot())
        .map_err(|e| ReadFailure::Query(io::Error::other(e)))?;
    let mut identities = Vec::new();
    let mut observed = Vec::new();
    for (batch, scores) in batches.iter().zip(distances) {
        let uid = column::<StringArray>(batch, "uid").map_err(ReadFailure::Query)?;
        let key = column::<StringArray>(batch, "data_key").map_err(ReadFailure::Query)?;
        let sha = column::<FixedSizeBinaryArray>(batch, "full_source_sha256")
            .map_err(ReadFailure::Query)?;
        for (row, distance) in scores.iter().enumerate() {
            identities.push((uid.value(row), key.value(row)));
            observed.push((sha.value(row), *distance));
        }
    }
    let entities = pin
        .entities(&identities)
        .await
        .map_err(ReadFailure::Query)?;
    let mut entities: BTreeMap<_, _> = entities
        .into_iter()
        .map(|entity| ((entity.uid.clone(), entity.data_key.clone()), entity))
        .collect();
    let mut ranked = Vec::new();
    for ((uid, key), (sha, distance)) in identities.into_iter().zip(observed) {
        let Some(entity) = entities.remove(&(uid.to_owned(), key.to_owned())) else {
            continue;
        };
        let encoded: String = sha.iter().map(|byte| format!("{byte:02x}")).collect();
        if entity.source_sha256 != encoded {
            continue;
        }
        ranked.push((entity, distance));
    }
    pin.revalidate().map_err(ReadFailure::Query)?;
    service
        .mode()
        .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
    authority.recheck()?;
    original.check().map_err(|_| ReadFailure::Context)?;
    let results = if matches!(input.mode, Mode::Hybrid) {
        fuse(lexical, ranked, k)?
    } else {
        ranked
            .into_iter()
            .map(|(entity, distance)| {
                let mut value = serde_json::to_value(entity).map_err(|_| ReadFailure::BadInput)?;
                value["distance"] = json!(distance);
                Ok(value)
            })
            .collect::<Result<Vec<_>, ReadFailure>>()?
    };
    let mode = match input.mode {
        Mode::Semantic => "semantic",
        Mode::Vector => "vector",
        Mode::Hybrid => "hybrid",
        Mode::Fts => "fts",
    };
    Ok(json!({"status":"available","mode":mode,"results":results,
        "model":{"id":model.model_id,"revision":model.revision,"vector_id":model.vector_id,"dimensions":model.dimensions}}))
}
/// Reorder one retrieved page while preserving every original score and pin.
pub(crate) fn apply_rerank(
    rows: &mut Vec<Value>,
    ranked: crate::search_rerank::RerankScores,
) -> io::Result<()> {
    if ranked.scores.len() != rows.len() || ranked.order.len() != rows.len() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut source = rows.drain(..).map(Some).collect::<Vec<_>>();
    for (rank, index) in ranked.order.into_iter().enumerate() {
        let mut value = source
            .get_mut(index)
            .and_then(Option::take)
            .ok_or(io::ErrorKind::InvalidData)?;
        let object = value.as_object_mut().ok_or(io::ErrorKind::InvalidData)?;
        object.insert("rerank_score".into(), json!(ranked.scores[index]));
        object.insert("rerank_rank".into(), json!(rank + 1));
        rows.push(value);
    }
    Ok(())
}

fn lexical_response(hits: Vec<Hit>) -> Value {
    json!({"status":"available","mode":"fts","results":hits})
}
fn component_unavailable(mode: Mode, hits: Vec<Hit>, status: &str) -> Value {
    if matches!(mode, Mode::Hybrid) {
        let mut value = lexical_response(hits);
        value["mode"] = json!("hybrid");
        value["vector_status"] = json!(status);
        value
    } else {
        json!({"status":status,"results":[]})
    }
}
/// Deterministic union of the two real ranked lists, using standard reciprocal rank fusion.
fn fuse(
    lexical: Vec<Hit>,
    vectors: Vec<(Entity, f32)>,
    k: usize,
) -> Result<Vec<Value>, ReadFailure> {
    let mut union = BTreeMap::new();
    for (rank, hit) in lexical.into_iter().enumerate() {
        union.insert(
            (hit.entity.uid.clone(), hit.entity.data_key.clone()),
            (
                hit.entity,
                1.0 / (60.0 + rank as f64 + 1.0),
                Some(hit.score),
                None,
            ),
        );
    }
    for (rank, (entity, distance)) in vectors.into_iter().enumerate() {
        let entry = union
            .entry((entity.uid.clone(), entity.data_key.clone()))
            .or_insert((entity, 0.0, None, None));
        entry.1 += 1.0 / (60.0 + rank as f64 + 1.0);
        entry.3 = Some(distance);
    }
    let mut rows = union.into_iter().collect::<Vec<_>>();
    rows.sort_by(|(ka, a), (kb, b)| b.1.total_cmp(&a.1).then_with(|| ka.cmp(kb)));
    rows.into_iter()
        .take(k)
        .map(|(_, (entity, score, lexical, distance))| {
            let mut value = serde_json::to_value(entity).map_err(|_| ReadFailure::BadInput)?;
            value["score"] = json!(score);
            value["lexical_score"] = json!(lexical);
            value["distance"] = json!(distance);
            Ok(value)
        })
        .collect()
}

impl GraphServer {
    /// Reuse the ordinary search kernel over the Live caller's retained SDK pin.
    pub(crate) async fn search_captured<'a>(
        &'a self,
        source: &crate::source_server::SourceServer,
        pin: &EntitySearch,
        authority: Authority<'a>,
        original: &'a RequestContext<'a>,
        input: Input,
    ) -> io::Result<Value> {
        let result = copied(
            source.ast_search_context()?,
            pin,
            authority,
            original,
            input,
        )
        .await;
        original.check()?;
        match result {
            Ok(value) => Ok(value),
            Err(ReadFailure::Query(cause) | ReadFailure::Output(cause)) => Err(cause),
            Err(ReadFailure::Imported(ImportedGraphError::Operation(cause))) => Err(cause),
            Err(error) => {
                let status = error.into_response().status();
                let kind = match status.as_u16() {
                    400 => io::ErrorKind::InvalidInput,
                    401 | 403 => io::ErrorKind::PermissionDenied,
                    404 => io::ErrorKind::NotFound,
                    408 | 504 => io::ErrorKind::TimedOut,
                    409 => io::ErrorKind::InvalidData,
                    503 => io::ErrorKind::NotConnected,
                    _ => io::ErrorKind::Other,
                };
                Err(io::Error::new(kind, "captured AST search failed"))
            }
        }
    }
}

#[cfg(test)]
mod rerank_tests {
    use super::*;

    #[test]
    fn rerank_preserves_candidate_membership_and_original_scores() {
        let original = vec![
            json!({"uid":"a","data_key":"a1","score":1.5,"distance":0.2}),
            json!({"uid":"b","data_key":"b1","score":2.5,"distance":0.7}),
        ];
        let mut rows = original.clone();
        apply_rerank(
            &mut rows,
            crate::search_rerank::RerankScores {
                scores: vec![-0.5, 2.],
                order: vec![1, 0],
            },
        )
        .unwrap();
        for (row, index) in rows.iter().zip([1, 0]) {
            assert_eq!(row["uid"], original[index]["uid"]);
            assert_eq!(row["data_key"], original[index]["data_key"]);
            assert_eq!(row["score"], original[index]["score"]);
            assert_eq!(row["distance"], original[index]["distance"]);
        }
        assert_eq!(rows[0]["rerank_score"], json!(2.));
        assert_eq!(rows[1]["rerank_rank"], json!(2));
    }

    #[test]
    fn rerank_flag_rejects_null_and_preserves_omission_and_false() {
        let omitted: Input = serde_json::from_value(json!({"query":"x","mode":"fts"})).unwrap();
        assert_eq!(omitted.rerank, None);
        let disabled: Input = serde_json::from_value(json!({"query":"x","rerank":false})).unwrap();
        assert_eq!(disabled.rerank, Some(false));
        for invalid in [json!(null), json!("true"), json!(1)] {
            assert!(
                serde_json::from_value::<Input>(json!({"query":"x","rerank":invalid})).is_err()
            );
        }
    }
}
