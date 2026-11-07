//! Wire adapters for the authorized, retained local live graph.
use super::*;
use graphit_ast::{
    icebug_query::{QueryCatalog, QueryOptions},
    live_graph::LiveGraph,
};
use graphit_core::file_io::SnapshotContext;
use serde_json::json;

/// Retain only this graph operation's native shard capture and PROJECT admission.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_live(
    index: &LiveGraph,
    catalog: &QueryCatalog,
    original: &Arc<SnapshotContext>,
    scope: &NeighborhoodScope,
    operation: Operation,
    input: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    options: QueryOptions,
    sample_limits: Option<GraphSampleLimits>,
    neighborhood_limits: Option<GraphNeighborhoodLimits>,
    maximum: usize,
) -> Result<Vec<u8>, ReadFailure> {
    index
        .with_graph(original, |graph| {
            render_captured(
                graph,
                catalog,
                original,
                scope,
                operation,
                input,
                neighborhood,
                options,
                sample_limits,
                neighborhood_limits,
                maximum,
            )
        })
        .map_err(ReadFailure::Query)?
}

#[allow(clippy::too_many_arguments)]
fn render_captured(
    graph: &Arc<graphit_data::CapturedLocalGraph>,
    catalog: &QueryCatalog,
    original: &Arc<SnapshotContext>,
    scope: &NeighborhoodScope,
    operation: Operation,
    input: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    options: QueryOptions,
    sample_limits: Option<GraphSampleLimits>,
    neighborhood_limits: Option<GraphNeighborhoodLimits>,
    maximum: usize,
) -> Result<Vec<u8>, ReadFailure> {
    original.check().map_err(ReadFailure::Query)?;
    graph.revalidate(original).map_err(native_graph_error)?;
    let response = match operation {
        Operation::Imports => return Err(ReadFailure::BadInput),
        Operation::Query => {
            let input = input.ok_or(ReadFailure::BadInput)?;
            let runtime = tokio::runtime::Handle::try_current()
                .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
            let result = catalog
                .execute(
                    graph,
                    original,
                    &runtime,
                    &input.query,
                    &input.parameters,
                    options,
                )
                .map_err(ReadFailure::Query)?;
            encode(&result, maximum)?
        }
        Operation::Schema => {
            let schema = graph.schema().map_err(native_graph_error)?;
            let mut counts = BTreeMap::<(String, String, String), u64>::new();
            for member in schema.relationships() {
                let rows = counts
                    .entry((
                        member.kind().to_owned(),
                        member.from_label().to_owned(),
                        member.to_label().to_owned(),
                    ))
                    .or_default();
                *rows = rows.checked_add(member.rows()).ok_or_else(|| {
                    native_graph_error(graphit_data::DataReadError::LimitExceeded(
                        "relationship rows",
                    ))
                })?;
            }
            let relationships = counts.into_iter().map(|((kind, from, to), rows)| {
                json!({"kind":kind,"from_label":from,"to_label":to,"rows":rows.to_string(),"properties":[["uid","STRING"]]})
            }).collect::<Vec<_>>();
            encode(
                &json!({"nodes":schema.node_tables().map(NodeSchema).collect::<Vec<_>>(), "relationships":relationships}),
                maximum,
            )?
        }
        Operation::Sample => {
            let sample = graph
                .sample_graph_excluding_marked(
                    sample_limits.ok_or(ReadFailure::BadInput)?,
                    "is_file_observation",
                )
                .map_err(native_graph_error)?;
            encode(&sample_wire(&sample), maximum)?
        }
        Operation::Neighborhood => {
            let input = neighborhood.ok_or(ReadFailure::BadInput)?;
            if input.limit == 0
                || input.limit > 1000
                || input
                    .limit
                    .checked_add(1)
                    .is_none_or(|value| value > options.max_rows)
            {
                return Err(ReadFailure::BadInput);
            }
            let limits = neighborhood_limits.ok_or(ReadFailure::BadInput)?;
            let request = neighborhood_request(&input, scope)?;
            let anchor = graph
                .resolve_node(request.anchor_label, request.anchor_key, limits)
                .map_err(native_graph_error)?;
            let page = graph
                .neighborhood_graph_at(&anchor, request, limits)
                .map_err(native_graph_error)?;
            encode(&neighborhood_wire(&page, scope)?, maximum)?
        }
    };
    original.check().map_err(ReadFailure::Query)?;
    graph.revalidate(original).map_err(native_graph_error)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue};
    use serde_json::Value;
    fn original() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::service(Cancellation::default()))
    }
    fn scope() -> NeighborhoodScope {
        NeighborhoodScope::Project {
            project: "fixture".into(),
            version: "main".into(),
        }
    }
    fn options() -> QueryOptions {
        QueryOptions {
            buffer_pool_bytes: 64 * 1024 * 1024,
            query_threads: 2,
            max_rows: 2000,
            max_bytes: 1024 * 1024,
            max_depth: 32,
        }
    }
    fn neighborhood_limits() -> GraphNeighborhoodLimits {
        GraphNeighborhoodLimits::new(65536, 256, 20000, 20000, 20000, 256).unwrap()
    }
    fn sample_limits() -> GraphSampleLimits {
        GraphSampleLimits::new(300, 1000, 2300, 65536, 20000, 20000, 256).unwrap()
    }
    fn model(key: &str, path: &str) -> GraphNode {
        GraphNode {
            label: "Function".into(),
            key: key.into(),
            properties: BTreeMap::from([
                ("name".into(), GraphValue::String(key.into())),
                ("path".into(), GraphValue::String(path.into())),
                ("uid".into(), GraphValue::String(key.into())),
                ("lang".into(), GraphValue::String("rust".into())),
                ("line_number".into(), GraphValue::Int64(1)),
            ]),
        }
    }
    fn result(
        index: &LiveGraph,
        op: Operation,
        query: Option<QueryData>,
        neighborhood: Option<NeighborhoodData>,
    ) -> Value {
        let bytes = render_live(
            index,
            &QueryCatalog::new(),
            &original(),
            &scope(),
            op,
            query,
            neighborhood,
            options(),
            Some(sample_limits()),
            Some(neighborhood_limits()),
            1024 * 1024,
        )
        .unwrap_or_else(|_| panic!("live graph render failed"));
        serde_json::from_slice(&bytes).unwrap()
    }
    fn request(cursor: Option<Cursor>) -> NeighborhoodData {
        NeighborhoodData {
            anchor_label: "Function".into(),
            anchor_key: GraphKey::String("a".into()),
            relationship: Some("CALLS".into()),
            other_label: Some("Function".into()),
            direction: Some(Direction::Outgoing),
            limit: 1,
            cursor,
        }
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn unfiltered_neighborhood_pages_both_directions_and_preserves_parallel_edges() {
        tokio::task::spawn_blocking(|| {
            let dir = tempfile::tempdir().unwrap();
            let original = original();
            let graph =
                LiveGraph::open_path_owned(dir.path().join("graph"), &original, [7; 32], true)
                    .unwrap();
            let edges = [
                ("CALLS", "a", "b", "01-out"),
                ("CALLS", "a", "b", "02-parallel"),
                ("CALLS", "b", "a", "03-in"),
                ("REFERS_TO", "a", "a", "04-self"),
                ("CALLS", "b", "c", "05-hop"),
            ]
            .map(|(kind, source, target, uid)| GraphEdge {
                kind: kind.into(),
                source_key: source.into(),
                target_key: target.into(),
                uid: uid.into(),
            });
            graph
                .replace_file(
                    graphit_ast::live_graph::LiveFileInput {
                        path: "graph.rs",
                        source_sha256: [9; 32],
                        raw_version: 1,
                        nodes: &[
                            model("a", "graph.rs"),
                            model("b", "graph.rs"),
                            model("c", "graph.rs"),
                        ],
                        edges: &edges,
                        definitions: &[],
                        references: &[],
                        scip_document: None,
                        oracle: &[],
                        oracle_file: None,
                        pending_compiler: None,
                        scip_compiler: None,
                    },
                    &original,
                )
                .unwrap();
            let page_input = |key: &str, cursor: Value| {
                serde_json::from_value::<NeighborhoodData>(json!({
                    "anchor_label":"Function", "anchor_key":{"type":"string","value":key},
                    "limit":2, "cursor":cursor,
                }))
                .unwrap()
            };
            let mut maximum_page = page_input("a", Value::Null);
            maximum_page.limit = 1000;
            let maximum_page = result(&graph, Operation::Neighborhood, None, Some(maximum_page));
            assert_eq!(maximum_page["edges"].as_array().unwrap().len(), 4);
            assert!(maximum_page["next_cursor"].is_null());
            let first = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(page_input("a", Value::Null)),
            );
            assert!(first.get("generation").is_none());
            assert_eq!(
                first["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|edge| edge["uid"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["01-out", "02-parallel"]
            );
            let selected = first["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| {
                    node["properties"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|property| {
                            property["name"] == "uid" && property["value"]["value"] == "a"
                        })
                })
                .unwrap();
            assert_eq!(
                selected["primary_key"],
                json!({"name":"entity_key","value":{"type":"string","value":"a"}})
            );
            assert_eq!(
                first["edges"][0]["position"],
                json!({
                    "uid":"01-out","relationship":"CALLS","from_label":"Function",
                    "from_key":{"type":"string","value":"a"},"to_label":"Function",
                    "to_key":{"type":"string","value":"b"},"occurrence":"0"
                })
            );
            assert_eq!(
                first["next_cursor"]["position"],
                first["edges"].as_array().unwrap().last().unwrap()["position"]
            );
            assert!(!first["next_cursor"].is_null());
            assert!(first["next_cursor"].get("generation").is_none());
            let second = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(page_input("a", first["next_cursor"].clone())),
            );
            assert_eq!(
                second["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|edge| edge["uid"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["03-in", "04-self"]
            );
            assert!(second["next_cursor"].is_null());
            assert_eq!(second["nodes"].as_array().unwrap().len(), 2);
            let hop = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(page_input("b", Value::Null)),
            );
            let hop = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(page_input("b", hop["next_cursor"].clone())),
            );
            assert!(
                hop["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|edge| edge["uid"] == "05-hop")
            );
            assert!(hop["nodes"].as_array().unwrap().iter().any(|node| {
                node["properties"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|property| property["name"] == "uid" && property["value"]["value"] == "c")
            }));
            for operation in [Operation::Schema, Operation::Sample] {
                assert!(
                    result(&graph, operation, None, None)
                        .get("generation")
                        .is_none()
                );
            }
            let typed = result(
                &graph,
                Operation::Query,
                Some(QueryData {
                    query: "MATCH(a:Function {entity_key:$key}) RETURN a".into(),
                    parameters: BTreeMap::from([("key".into(), "a".into())]),
                }),
                None,
            );
            assert_eq!(typed["rows"][0][0]["type"], "NODE");
            assert_eq!(typed["rows"][0][0]["value"]["id"]["type"], "INTERNAL_ID");
            assert!(typed["rows"][0][0]["value"]["id"]["value"]["row"].is_string());
            assert!(
                typed["rows"][0][0]["value"]["id"]["value"]
                    .get("generation")
                    .is_none()
            );
        })
        .await
        .unwrap();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn real_live_schema_sample_neighborhood_wire_and_keyset() {
        tokio::task::spawn_blocking(|| {
            let dir = tempfile::tempdir().unwrap();
            let original = original();
            let graph =
                LiveGraph::open_path_owned(dir.path().join("graph"), &original, [5; 32], true)
                    .unwrap();
            let observation = GraphNode {
                label: "AstFileObservation".into(),
                key: "file-a".into(),
                properties: BTreeMap::from([(
                    "is_file_observation".into(),
                    GraphValue::Bool(true),
                )]),
            };
            let edges = [
                GraphEdge {
                    kind: "CALLS".into(),
                    source_key: "a".into(),
                    target_key: "b".into(),
                    uid: "01-ab".into(),
                },
                GraphEdge {
                    kind: "CALLS".into(),
                    source_key: "a".into(),
                    target_key: "c".into(),
                    uid: "02-ac".into(),
                },
            ];
            graph
                .replace_file(
                    graphit_ast::live_graph::LiveFileInput {
                        path: "a.rs",
                        source_sha256: [1; 32],
                        raw_version: 1,
                        nodes: &[model("a", "a.rs"), observation],
                        edges: &edges,
                        definitions: &[],
                        references: &[],
                        scip_document: None,
                        oracle: &[],
                        oracle_file: None,
                        pending_compiler: None,
                        scip_compiler: None,
                    },
                    &original,
                )
                .unwrap();
            graph
                .replace_file(
                    graphit_ast::live_graph::LiveFileInput {
                        path: "b.rs",
                        source_sha256: [2; 32],
                        raw_version: 1,
                        nodes: &[model("b", "b.rs"), model("c", "b.rs")],
                        edges: &[],
                        definitions: &[],
                        references: &[],
                        scip_document: None,
                        oracle: &[],
                        oracle_file: None,
                        pending_compiler: None,
                        scip_compiler: None,
                    },
                    &original,
                )
                .unwrap();
            let schema = result(&graph, Operation::Schema, None, None);
            let functions = schema["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["label"] == "Function")
                .unwrap();
            assert_eq!(functions["rows"], "3");
            assert_eq!(functions["language_counts"], json!([["rust", "3"]]));
            assert_eq!(schema["relationships"][0]["rows"], "2");
            let sample = result(&graph, Operation::Sample, None, None);
            assert_eq!(sample["nodes"].as_array().unwrap().len(), 3);
            assert_eq!(sample["edges"].as_array().unwrap().len(), 2);
            let anchor = sample["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["uid"] == "a")
                .unwrap();
            assert_eq!(
                anchor["primary_key"],
                json!({"name":"entity_key","value":{"type":"string","value":"a"}})
            );
            assert_eq!(anchor["line"], "1");
            let first = result(&graph, Operation::Neighborhood, None, Some(request(None)));
            assert_eq!(first["anchor"], anchor["id"]);
            assert_eq!(first["edges"].as_array().unwrap().len(), 1);
            assert_eq!(first["edges"][0]["uid"], "01-ab");
            let cursor: Cursor = serde_json::from_value(first["next_cursor"].clone()).unwrap();
            let second = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(request(Some(cursor.clone()))),
            );
            assert_eq!(second["edges"][0]["uid"], "02-ac");
            assert!(second["next_cursor"].is_null());
            let properties = first["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == anchor["id"])
                .unwrap()["properties"]
                .as_array()
                .unwrap();
            assert!(
                properties
                    .iter()
                    .any(|p| p
                        == &json!({"name":"line_number","value":{"type":"int64","value":"1"}}))
            );
            let mut incoming = request(None);
            incoming.direction = Some(Direction::Incoming);
            incoming.anchor_key = GraphKey::String("b".into());
            let incoming = result(&graph, Operation::Neighborhood, None, Some(incoming));
            assert_eq!(incoming["edges"][0]["uid"], "01-ab");
            let mut incorrect = cursor.clone();
            incorrect.fingerprint[0] ^= 1;
            assert!(
                render_live(
                    &graph,
                    &QueryCatalog::new(),
                    &original,
                    &scope(),
                    Operation::Neighborhood,
                    None,
                    Some(request(Some(incorrect))),
                    options(),
                    Some(sample_limits()),
                    Some(neighborhood_limits()),
                    1024 * 1024
                )
                .is_err()
            );
            let value = result(
                &graph,
                Operation::Query,
                Some(QueryData {
                    query: "RETURN 42 AS value".into(),
                    parameters: BTreeMap::new(),
                }),
                None,
            );
            assert_eq!(value["rows"][0][0]["value"], "42");
            assert!(
                render_live(
                    &graph,
                    &QueryCatalog::new(),
                    &original,
                    &scope(),
                    Operation::Query,
                    Some(QueryData {
                        query: "MATCH(n) DELETE n".into(),
                        parameters: BTreeMap::new()
                    }),
                    None,
                    options(),
                    Some(sample_limits()),
                    Some(neighborhood_limits()),
                    1024 * 1024
                )
                .is_err()
            );
            graph
                .replace_file(
                    graphit_ast::live_graph::LiveFileInput {
                        path: "c.rs",
                        source_sha256: [3; 32],
                        raw_version: 1,
                        nodes: &[model("d", "c.rs")],
                        edges: &[GraphEdge {
                            kind: "CALLS".into(),
                            source_key: "d".into(),
                            target_key: "a".into(),
                            uid: "03-da".into(),
                        }],
                        definitions: &[],
                        references: &[],
                        scip_document: None,
                        oracle: &[],
                        oracle_file: None,
                        pending_compiler: None,
                        scip_compiler: None,
                    },
                    &original,
                )
                .unwrap();
            let schema = result(&graph, Operation::Schema, None, None);
            assert_eq!(schema["relationships"].as_array().unwrap().len(), 1);
            assert_eq!(schema["relationships"][0]["rows"], "3");
            assert_eq!(
                schema["relationships"][0]["properties"],
                json!([["uid", "STRING"]])
            );
            let continued = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(request(Some(cursor.clone()))),
            );
            assert_eq!(
                continued["edges"][0]["uid"], "02-ac",
                "an independent FILE update does not invalidate the logical keyset"
            );
            let other_scope = NeighborhoodScope::Project {
                project: "fixture".into(),
                version: "other-branch".into(),
            };
            assert!(
                render_live(
                    &graph,
                    &QueryCatalog::new(),
                    &original,
                    &other_scope,
                    Operation::Neighborhood,
                    None,
                    Some(request(Some(cursor.clone()))),
                    options(),
                    Some(sample_limits()),
                    Some(neighborhood_limits()),
                    1024 * 1024
                )
                .is_err()
            );
            graph.remove_file("c.rs", &original).unwrap();
            graph.remove_file("b.rs", &original).unwrap();
            let removed = result(
                &graph,
                Operation::Neighborhood,
                None,
                Some(request(Some(cursor))),
            );
            assert_eq!(removed["edges"], json!([]));
            assert!(removed["next_cursor"].is_null());
            let schema = result(&graph, Operation::Schema, None, None);
            assert_eq!(
                schema["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|n| n["label"] == "Function")
                    .unwrap()["rows"],
                "1"
            );
            assert_eq!(schema["relationships"][0]["rows"], "0");
            assert!(
                render_live(
                    &graph,
                    &QueryCatalog::new(),
                    &original,
                    &scope(),
                    Operation::Sample,
                    None,
                    None,
                    options(),
                    Some(sample_limits()),
                    Some(neighborhood_limits()),
                    1
                )
                .is_err()
            );
        })
        .await
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_http_sample_projects_summary_columns_and_keeps_neighborhood_anchor() {
        use graphit_core::tcp_listener::{self, Options, PortSelection};
        use std::time::Duration;

        let (fixture, server, _) = crate::graph_server::tests::web_fixture().await;
        let original = original();
        let live =
            crate::graph_server::tests::live_project_graph(&fixture, &server, &original).await;
        let mut wide = model("wide-key", "sample.rs");
        wide.label = "Auxiliary".into();
        wide.properties
            .insert("is_file_observation".into(), GraphValue::Bool(false));
        wide.properties.insert(
            "unselected_payload".into(),
            GraphValue::String("x".repeat(server.0.policy.graph_query.max_bytes + 1)),
        );
        let marked = |key: &str| GraphNode {
            label: "Auxiliary".into(),
            key: key.into(),
            properties: BTreeMap::from([
                ("name".into(), GraphValue::String(key.into())),
                ("is_file_observation".into(), GraphValue::Bool(true)),
            ]),
        };
        live.graph
            .replace_file(
                graphit_ast::live_graph::LiveFileInput {
                    path: "sample.rs",
                    source_sha256: [4; 32],
                    raw_version: 1,
                    nodes: &[
                        wide,
                        marked("marked-endpoint"),
                        marked("marked-unconnected"),
                    ],
                    edges: &[GraphEdge {
                        kind: "OBSERVES".into(),
                        source_key: "f1".into(),
                        target_key: "marked-endpoint".into(),
                        uid: "marked-edge".into(),
                    }],
                    definitions: &[],
                    references: &[],
                    scip_document: None,
                    oracle: &[],
                    oracle_file: None,
                    pending_compiler: None,
                    scip_compiler: None,
                },
                &original,
            )
            .unwrap();
        let listener = tcp_listener::reserve(
            Options {
                address: "127.0.0.1".parse().unwrap(),
                ports: PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let url = format!("http://{}", listener.address());
        let owner = server.clone();
        let serving = tokio::spawn(async move { owner.serve(listener).await });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let request = |operation: &str, body: Value| {
            client
                .post(format!("{url}/api/graphs/project/{operation}"))
                .bearer_auth(&fixture.raw)
                .header("x-graphit-auth-origin", "runtime")
                .json(&body)
        };
        let response = request("sample", json!({"selection":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let sample: Value = response.json().await.unwrap();
        assert!(sample.get("generation").is_none());
        let nodes = sample["nodes"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .all(|node| node["primary_key"]["value"]["type"] == "string")
        );
        let has_key = |key: &str| {
            nodes
                .iter()
                .any(|node| node["primary_key"]["value"]["value"] == key)
        };
        assert!(has_key("wide-key"));
        assert!(
            has_key("marked-endpoint"),
            "marked relationship endpoints remain in the sample"
        );
        assert!(
            !has_key("marked-unconnected"),
            "only marked seeds are excluded"
        );
        assert_eq!(sample["edges"].as_array().unwrap().len(), 4);

        // This is the real PROJECT query handler: RETURN n cannot fit the wide
        // property, whereas the sample above reads only the declared summary.
        let response = request(
            "query",
            json!({"selection":{},"query":{
                "query":"MATCH(n:Auxiliary {entity_key:$key}) RETURN n",
                "parameters":{"key":"wide-key"}
            }}),
        )
        .send()
        .await
        .unwrap();
        let status = response.status();
        let error = response.text().await.unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert!(
            error.contains("query cell exceeds response bytes"),
            "{error}"
        );

        let anchor = nodes.iter().find(|node| node["label"] == "File").unwrap();
        let mut input = json!({"selection":{},"neighborhood":{
            "anchor_label":"File","anchor_key":anchor["primary_key"]["value"],
            "relationship":"CONTAINS","other_label":"Function","direction":"outgoing","limit":1
        }});
        let response = request("neighborhood", input.clone()).send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let first: Value = response.json().await.unwrap();
        assert!(first.get("generation").is_none());
        assert_eq!(first["anchor"], anchor["id"]);
        assert_eq!(first["edges"][0]["uid"], "u0");
        assert!(!first["next_cursor"].is_null());
        input["neighborhood"]["cursor"] = first["next_cursor"].clone();
        let response = request("neighborhood", input).send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let second: Value = response.json().await.unwrap();
        assert_eq!(second["anchor"], anchor["id"]);
        assert_eq!(second["edges"][0]["uid"], "u1");
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}
