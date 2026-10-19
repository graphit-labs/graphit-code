//! Per-call ReadAst admissions. The capture and its proofs live until the read joins.
use super::{CallContext, Error, invalid};
use crate::{
    ast_source::SourceParameters,
    graph_server::{QueryData, search::Input},
    knowledge_live::{ArtifactSelection, ReadContextsSnapshot},
    live_agent_ast::{self, CapturedAstContext},
    reference_explorer,
};
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ScopeCapture,
    broker::project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
};
use graphit_core::artifact_store::CapturedPublicationScope;
use serde_json::Value;
use std::io;

pub(super) const SOURCE: &str = "graphit_ast_source";
pub(super) const SCHEMA: &str = "graphit_ast_schema";
pub(super) const QUERY: &str = "graphit_ast_query";
pub(super) const SEARCH: &str = "graphit_ast_search";

pub(crate) fn is_tool(name: &str) -> bool {
    matches!(name, SOURCE | SCHEMA | QUERY | SEARCH)
}
enum Command {
    Source(SourceParameters),
    Schema,
    Query(QueryData),
    Search(Input),
}
fn parse(name: &str, mut input: Value) -> Result<(ArtifactSelection, Command), Error> {
    let object = input
        .as_object_mut()
        .ok_or_else(|| invalid("invalid_arguments"))?;
    let selected = object
        .remove("context")
        .ok_or_else(|| invalid("context_required"))?;
    let selected: ArtifactSelection =
        serde_json::from_value(selected).map_err(|_| invalid("invalid_arguments"))?;
    let command = match name {
        SOURCE => {
            let source: SourceParameters =
                serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?;
            source
                .validate()
                .map_err(|_| invalid("invalid_arguments"))?;
            Command::Source(source)
        }
        SCHEMA if object.is_empty() => Command::Schema,
        QUERY => {
            Command::Query(serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?)
        }
        SEARCH => Command::Search(
            serde_json::from_value(input).map_err(|_| invalid("invalid_arguments"))?,
        ),
        _ => return Err(invalid("invalid_arguments")),
    };
    Ok((selected, command))
}
async fn captured(
    context: &CallContext<'_>,
    capture: &CapturedAstContext<'_>,
    command: Command,
) -> Result<Value, Error> {
    context.check_io()?;
    capture.recheck()?;
    let value = match command {
        Command::Source(input) => serde_json::to_value(capture.source(&input).await?)?,
        Command::Schema => capture.schema().await?,
        Command::Query(input) => capture.query(input).await?,
        Command::Search(input) => capture.search(context.identity.0, input).await?,
    };
    capture.recheck()?;
    context.check_io()?;
    Ok(value)
}

pub(super) async fn read(
    context: &CallContext<'_>,
    stored: &ReadContextsSnapshot,
    name: &str,
    input: Value,
) -> Result<Value, Error> {
    let (wanted, command) = parse(name, input)?;
    if !context.has_ast(stored)? || wanted.artifact_type != "ast" {
        return Err(invalid("selected_context_unavailable"));
    }
    let mut prepared = context
        .ast
        .iter()
        .filter(|entry| entry.selection == &wanted);
    let prepared = prepared
        .next()
        .ok_or_else(|| invalid("context_not_selected"))?;
    if context
        .ast
        .iter()
        .filter(|entry| entry.selection == &wanted)
        .count()
        != 1
    {
        return Err(invalid("selected_context_ambiguous"));
    }
    let _selected = stored
        .contexts
        .selected
        .iter()
        .find(|entry| entry.selection == wanted)
        .ok_or(Error::ContextChanged)?;
    let source = context
        .source
        .ok_or_else(|| invalid("selected_context_unavailable"))?;
    if let Command::Search(input) = &command {
        input
            .validate(source.policy())
            .map_err(|_| invalid("invalid_arguments"))?;
    }
    context.check_io()?;
    let r = context.resources;
    let (caller, verifier, provider) = context.identity;
    let issuer = r.auth.issuer();
    let hub = issuer.admit_web_installed_hub_read(
        caller,
        verifier,
        context.original,
        provider,
        r.auth.installation(),
    )?;
    let selectors =
        live_agent_ast::select_contexts(r, &hub, source, std::iter::once(prepared.selection))?;
    let selected = selectors
        .first()
        .ok_or_else(|| invalid("selected_context_unavailable"))?;
    if selected.project_index != prepared.project_index || selected.imported != prepared.imported {
        return Err(Error::ContextChanged);
    }
    let server = source.graph_server();
    let value = if let Some(index) = selected.project_index {
        let project = r.projects.get(index).ok_or(Error::ContextChanged)?;
        r.require_active(project)?;
        let (paths, id, version) = project;
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            r.policy.scope,
            context.original.snapshot(),
        )?;
        let binding = scope.binding()?;
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend: context.backend,
                original: context.original,
                scope: BrokerProjectScope {
                    installation: r.auth.installation(),
                    project: ProjectCapture {
                        scope: ScopeCapture {
                            live: &scope,
                            record: &binding,
                        },
                        version,
                    },
                    providers: &[],
                },
                provider,
                module: ProjectReadModule::Ast,
            })
            .await
            .map_err(reference_explorer::Failure::from)?;
        context.check_io()?;
        let capture = live_agent_ast::capture_project(
            selected,
            project,
            &server,
            source,
            &read,
            r.auth.installation(),
            context.original,
        )
        .await?;
        let value = captured(context, &capture, command).await?;
        read.recheck()?;
        r.require_active(project)?;
        value
    } else {
        let read = issuer.admit_web_imported_ast_read(
            caller,
            verifier,
            context.original,
            provider,
            r.auth.installation(),
        )?;
        let (id, version) = selected
            .imported
            .as_ref()
            .ok_or_else(|| invalid("selected_context_changed"))?;
        let registry = &source.policy().registry;
        let remote = crate::remote_native_mount::Selection::capture(
            crate::remote_native_mount::ReadAdmission::Ast(&read),
            registry,
            id,
            version,
        )
        .map_err(io::Error::other)?;
        let value = if let Some(remote) = remote {
            let mut service = issuer
                .capture_imported_ast_hub_service(&read, remote.record().provider_name())
                .map_err(io::Error::other)?;
            let verifier = if service.mode().map_err(io::Error::other)?
                == graphit_auth::ServiceMode::BrokerNative
            {
                Some(
                    r.native_verifiers
                        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
                        .get(&mut service)
                        .map_err(io::Error::other)?,
                )
            } else {
                None
            };
            let lease = match verifier.as_deref() {
                Some(v) => service.native_credential(v).await,
                None => service.credential(),
            }
            .map_err(io::Error::other)?;
            let opened = crate::source_server::remote_native::open(
                remote,
                &lease,
                r.native_http,
                r.native_allow_loopback_http,
                crate::native_publication::metadata_limits(
                    source.policy().content,
                    registry.limits().projection(),
                ),
            )
            .await
            .map_err(io::Error::other)?;
            context.check_io()?;
            let capture = live_agent_ast::capture_remote_imported(
                selected, &server, source, &opened, &lease, &read,
            )
            .await?;
            captured(context, &capture, command).await?
        } else {
            context.check_io()?;
            let capture =
                live_agent_ast::capture_local_imported(selected, &server, source, &read).await?;
            captured(context, &capture, command).await?
        };
        read.recheck()?;
        value
    };
    live_agent_ast::select_contexts(r, &hub, source, std::iter::once(prepared.selection))?;
    hub.recheck()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context() -> Value {
        json!({"source":"project","project_id":"fixture-project","type":"ast","id":"fixture-project","version":"main"})
    }

    #[test]
    fn ast_wire_requires_explicit_context_and_reuses_strict_source_selectors() {
        assert!(parse(SOURCE, json!({"path":"a.rs"})).is_err());
        assert!(parse(SCHEMA, json!({"context":null})).is_err());
        assert!(parse(SOURCE, json!({"context":context(),"path":"../a.rs"})).is_err());
        assert!(
            parse(
                SOURCE,
                json!({"context":context(),"path":"a.rs","project_dir":"/other"})
            )
            .is_err()
        );
        assert!(
            parse(
                SCHEMA,
                json!({"context":context(),"query":"MATCH (n) RETURN n"})
            )
            .is_err()
        );
        let (_, Command::Source(source)) = parse(
            SOURCE,
            json!({
                "context":context(),"path":"a.unindexed","entity":"name","entity_type":"Function",
                "head":4,"tail":2,"start_line":1,"end_line":7,"pattern":"needle",
                "regex":true,"before":1,"after":2,"line_numbers":true
            }),
        )
        .unwrap() else {
            panic!("Source command");
        };
        assert_eq!(source.path, "a.unindexed");
        assert_eq!(source.entity.as_deref(), Some("name"));
        assert_eq!(source.entity_type.as_deref(), Some("Function"));
        assert_eq!(
            (source.head, source.tail, source.start_line, source.end_line),
            (Some(4), Some(2), Some(1), Some(7))
        );
        assert_eq!(
            (
                source.pattern.as_deref(),
                source.regex,
                source.before,
                source.after,
                source.line_numbers
            ),
            (Some("needle"), true, Some(1), Some(2), true)
        );
    }

    #[test]
    fn ast_query_and_search_reject_undeclared_scope_and_forward_existing_data() {
        assert!(
            parse(
                QUERY,
                json!({"context":context(),"query":"MATCH (n) RETURN n","project_version":"other"})
            )
            .is_err()
        );
        assert!(
            parse(
                SEARCH,
                json!({"context":context(),"query":"needle","scope":"global"})
            )
            .is_err()
        );
        let (_, Command::Query(query)) = parse(
            QUERY,
            json!({
                "context":context(),"query":"MATCH (n) WHERE n.name = $name RETURN n.name",
                "parameters":{"name":"needle"}
            }),
        )
        .unwrap() else {
            panic!("Query command");
        };
        assert_eq!(query.parameters["name"], "needle");
        let (_, Command::Search(search)) = parse(
            SEARCH,
            json!({
                "context":context(),"query":"needle","mode":"fts","k":3,"metric":"cosine"
            }),
        )
        .unwrap() else {
            panic!("Search command");
        };
        assert_eq!(search.k, Some(3));
        assert!(matches!(
            search.mode,
            crate::graph_server::search::Mode::Fts
        ));
    }
}
