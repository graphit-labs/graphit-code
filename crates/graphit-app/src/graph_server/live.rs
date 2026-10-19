//! Wire adapters for the authorized, retained local live graph.
use super::*;
use graphit_ast::{
    icebug_query::{QueryOptions, QueryResult},
    live_graph::LiveGraph,
};
use graphit_core::file_io::SnapshotContext;
use serde_json::{Value, json};

fn invalid() -> ReadFailure {
    ReadFailure::Query(io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid native graph result",
    ))
}
fn quoted(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}
fn body<'a>(value: &'a Value, kind: &str) -> Result<&'a Value, ReadFailure> {
    if value.get("type").and_then(Value::as_str) != Some(kind) {
        return Err(invalid());
    }
    value.get("value").ok_or_else(invalid)
}
fn string(value: &Value) -> Result<&str, ReadFailure> {
    body(value, "STRING")?.as_str().ok_or_else(invalid)
}
fn properties(value: &Value) -> Result<&serde_json::Map<String, Value>, ReadFailure> {
    body(
        body(value, "NODE")?.get("properties").ok_or_else(invalid)?,
        "PROPERTIES",
    )?
    .as_object()
    .ok_or_else(invalid)
}
fn native_id(value: &Value, generation: &[u8; 32], edge: bool) -> Result<(u32, u64), ReadFailure> {
    let value = body(value, "INTERNAL_ID")?;
    if value.get("generation") != Some(&json!(generation)) {
        return Err(invalid());
    }
    let table = value
        .get(if edge { "member" } else { "table" })
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(invalid)?;
    let row = value
        .get("row")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .parse::<u64>()
        .map_err(|_| invalid())?;
    Ok((table, row))
}
fn identifier(value: &Value, generation: &[u8; 32]) -> Result<NodeId, ReadFailure> {
    let (table, row) = native_id(value, generation, false)?;
    Ok(NodeId {
        table,
        row: row.to_string(),
    })
}
fn node_key(value: &Value, generation: &[u8; 32]) -> Result<(u32, u64), ReadFailure> {
    native_id(
        body(value, "NODE")?.get("id").ok_or_else(invalid)?,
        generation,
        false,
    )
}
fn property_from<'a>(name: &'a str, value: &'a Value) -> Result<Property<'a>, ReadFailure> {
    let v = value.get("value").ok_or_else(invalid)?;
    let value = if v.is_null() {
        PropertyValue::Null
    } else {
        match value.get("type").and_then(Value::as_str) {
            Some("STRING") => PropertyValue::String(v.as_str().ok_or_else(invalid)?),
            Some("INT64") => {
                let text = v.as_str().ok_or_else(invalid)?;
                text.parse::<i64>().map_err(|_| invalid())?;
                PropertyValue::Int64(text.into())
            }
            Some("BOOL") => PropertyValue::Bool(v.as_bool().ok_or_else(invalid)?),
            _ => return Err(invalid()),
        }
    };
    Ok(Property { name, value })
}
fn property_values(value: &Value) -> Result<Vec<Property<'_>>, ReadFailure> {
    body(value, "PROPERTIES")?
        .as_object()
        .ok_or_else(invalid)?
        .iter()
        .map(|(name, value)| property_from(name, value))
        .collect()
}
fn optional_text<'a>(
    props: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<&'a str>, ReadFailure> {
    match props.get(name) {
        None => Ok(None),
        Some(v) if v.get("value").is_some_and(Value::is_null) => Ok(None),
        Some(v) => string(v).map(Some),
    }
}
fn node<'a>(value: &'a Value, generation: &[u8; 32]) -> Result<Node<'a>, ReadFailure> {
    let native = body(value, "NODE")?;
    let props = properties(value)?;
    let line = match props.get("line_number") {
        None => None,
        Some(v) if v.get("value").is_some_and(Value::is_null) => None,
        Some(v) => {
            let text = body(v, "INT64")?.as_str().ok_or_else(invalid)?;
            text.parse::<i64>().map_err(|_| invalid())?;
            Some(text.to_owned())
        }
    };
    Ok(Node {
        id: identifier(native.get("id").ok_or_else(invalid)?, generation)?,
        label: string(native.get("label").ok_or_else(invalid)?)?,
        name: optional_text(props, "name")?,
        path: optional_text(props, "path")?,
        uid: optional_text(props, "uid")?,
        cluster: optional_text(props, "cluster")?,
        lang: optional_text(props, "lang")?,
        line,
        primary_key: Some(property_from(
            "entity_key",
            props.get("entity_key").ok_or_else(invalid)?,
        )?),
    })
}
fn edge<'a>(value: &'a Value, generation: &[u8; 32]) -> Result<Edge<'a>, ReadFailure> {
    let native = body(value, "REL")?;
    let (member, row) = native_id(native.get("id").ok_or_else(invalid)?, generation, true)?;
    let props = body(native.get("properties").ok_or_else(invalid)?, "PROPERTIES")?
        .as_object()
        .ok_or_else(invalid)?;
    Ok(Edge {
        id: EdgeId {
            member,
            row: row.to_string(),
        },
        from: identifier(native.get("from").ok_or_else(invalid)?, generation)?,
        to: identifier(native.get("to").ok_or_else(invalid)?, generation)?,
        relationship: string(native.get("label").ok_or_else(invalid)?)?,
        uid: string(props.get("uid").ok_or_else(invalid)?)?,
    })
}
fn property_node<'a>(
    value: &'a Value,
    generation: &[u8; 32],
) -> Result<PropertyNode<'a>, ReadFailure> {
    let native = body(value, "NODE")?;
    Ok(PropertyNode {
        id: identifier(native.get("id").ok_or_else(invalid)?, generation)?,
        label: string(native.get("label").ok_or_else(invalid)?)?,
        properties: property_values(native.get("properties").ok_or_else(invalid)?)?,
    })
}
fn property_edge<'a>(
    value: &'a Value,
    generation: &[u8; 32],
) -> Result<PropertyEdge<'a>, ReadFailure> {
    let native = edge(value, generation)?;
    Ok(PropertyEdge {
        id: native.id,
        from: native.from,
        to: native.to,
        relationship: native.relationship,
        uid: native.uid,
        properties: property_values(body(value, "REL")?.get("properties").ok_or_else(invalid)?)?,
    })
}
fn query(
    graph: &Arc<graphit_data::CapturedLocalGraph>,
    namespace: [u8; 32],
    original: &Arc<SnapshotContext>,
    text: &str,
    parameters: BTreeMap<String, Value>,
    options: QueryOptions,
) -> Result<QueryResult, ReadFailure> {
    graphit_ast::icebug_query::execute_owned(graph, original, namespace, text, &parameters, options)
        .map_err(ReadFailure::Query)
}
/// Retain only this graph operation's native shard capture and PROJECT admission.
pub(crate) fn render_live(
    index: &LiveGraph,
    original: &Arc<SnapshotContext>,
    operation: Operation,
    input: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    options: QueryOptions,
    maximum: usize,
) -> Result<Vec<u8>, ReadFailure> {
    let namespace = index.namespace();
    index
        .with_graph(original, |graph| {
            render_captured(
                graph,
                namespace,
                original,
                operation,
                input,
                neighborhood,
                options,
                maximum,
            )
        })
        .map_err(ReadFailure::Query)?
}

#[allow(clippy::too_many_arguments)]
fn render_captured(
    graph: &Arc<graphit_data::CapturedLocalGraph>,
    namespace: [u8; 32],
    original: &Arc<SnapshotContext>,
    operation: Operation,
    input: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
    options: QueryOptions,
    maximum: usize,
) -> Result<Vec<u8>, ReadFailure> {
    original.check().map_err(ReadFailure::Query)?;
    let generation = graph
        .owned_generation(namespace)
        .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
    let schema = graph
        .schema()
        .map_err(|error| ReadFailure::Query(io::Error::other(error)))?;
    let response = match operation {
        Operation::Imports => return Err(ReadFailure::BadInput),
        Operation::Query => {
            let input = input.ok_or(ReadFailure::BadInput)?;
            encode(
                &graphit_ast::icebug_query::execute_owned(
                    graph,
                    original,
                    namespace,
                    &input.query,
                    &input.parameters,
                    options,
                )
                .map_err(ReadFailure::Query)?,
                maximum,
            )?
        }
        Operation::Schema => {
            let mut counts = BTreeMap::<(String, String, String), u64>::new();
            for member in schema.relationships() {
                let rows = counts
                    .entry((
                        member.kind().to_owned(),
                        member.from_label().to_owned(),
                        member.to_label().to_owned(),
                    ))
                    .or_default();
                *rows = rows.checked_add(member.rows()).ok_or_else(invalid)?;
            }
            let relationships = counts.into_iter().map(|((kind, from, to), rows)| {
                    json!({"kind":kind,"from_label":from,"to_label":to,"rows":rows.to_string(),"properties":[["uid","STRING"]]})
                }).collect::<Vec<_>>();
            encode(
                &json!({
                    "generation":generation,
                    "nodes":schema.node_tables().map(NodeSchema).collect::<Vec<_>>(),
                    "relationships":relationships,
                }),
                maximum,
            )
        }?,
        Operation::Sample => {
            if schema.node_tables().len() == 0 {
                return encode(
                    &json!({"generation":generation,"nodes":[],"edges":[]}),
                    maximum,
                );
            }
            let filter = if schema.node_tables().any(|table| {
                table
                    .columns()
                    .any(|(name, kind)| name == "is_file_observation" && kind == "BOOL")
            }) {
                "WHERE coalesce(n.is_file_observation,false)=false"
            } else {
                ""
            };
            let seeds = query(
                graph,
                namespace,
                original,
                &format!("MATCH(n) {filter} RETURN n LIMIT $limit"),
                BTreeMap::from([("limit".into(), 300.min(options.max_rows).into())]),
                options,
            )?;
            let relations = if schema.relationships().len() == 0 {
                QueryResult {
                    columns: Vec::new(),
                    rows: Vec::new(),
                }
            } else {
                query(
                    graph,
                    namespace,
                    original,
                    "MATCH(s)-[r]->(t) RETURN s,r,t LIMIT $limit",
                    BTreeMap::from([("limit".into(), 1000.min(options.max_rows).into())]),
                    options,
                )?
            };
            let mut selected = BTreeMap::new();
            for row in &seeds.rows {
                let value = row.first().ok_or_else(invalid)?;
                selected.insert(node_key(value, &generation)?, value);
            }
            for row in &relations.rows {
                if row.len() != 3 {
                    return Err(invalid());
                }
                for value in [&row[0], &row[2]] {
                    selected.insert(node_key(value, &generation)?, value);
                }
            }
            let nodes = selected
                .into_values()
                .map(|v| node(v, &generation))
                .collect::<Result<Vec<_>, _>>()?;
            let edges = relations
                .rows
                .iter()
                .map(|r| edge(&r[1], &generation))
                .collect::<Result<Vec<_>, _>>()?;
            encode(
                &Sample {
                    generation,
                    nodes,
                    edges,
                },
                maximum,
            )?
        }
        Operation::Neighborhood => {
            let input = neighborhood.ok_or(ReadFailure::BadInput)?;
            if input.limit == 0
                || input.limit > 1000
                || input
                    .limit
                    .checked_add(1)
                    .is_none_or(|v| v > options.max_rows)
            {
                return Err(ReadFailure::BadInput);
            }
            let key = match &input.anchor_key {
                Anchor::String(value) => value,
                Anchor::Int64(_) => return Err(ReadFailure::BadInput),
            };
            if !schema
                .node_tables()
                .any(|table| table.label() == input.anchor_label)
                || !schema
                    .node_tables()
                    .any(|table| table.label() == input.other_label)
            {
                return Err(ReadFailure::BadInput);
            }
            let pair = match input.direction {
                Direction::Outgoing => (input.anchor_label.clone(), input.other_label.clone()),
                Direction::Incoming => (input.other_label.clone(), input.anchor_label.clone()),
            };
            if !schema.relationships().any(|member| {
                member.kind() == input.relationship
                    && member.from_label() == pair.0
                    && member.to_label() == pair.1
            }) {
                return Err(ReadFailure::BadInput);
            }
            let request = GraphNeighborhoodRequest {
                anchor_label: &input.anchor_label,
                anchor_key: GraphAnchorKey::String(key),
                relationship: &input.relationship,
                other_label: &input.other_label,
                direction: match input.direction {
                    Direction::Outgoing => GraphDirection::Outgoing,
                    Direction::Incoming => GraphDirection::Incoming,
                },
                limit: input.limit,
                cursor: None,
            };
            let fingerprint = graphit_data::neighborhood_fingerprint(&request)
                .map_err(|_| ReadFailure::BadInput)?;
            if let Some(cursor) = &input.cursor
                && (cursor.generation != generation
                    || cursor.fingerprint != fingerprint
                    || cursor.last_uid.is_empty())
            {
                return Err(ReadFailure::BadInput);
            }
            let anchor = query(
                graph,
                namespace,
                original,
                &format!(
                    "MATCH(a:{} {{entity_key:$key}}) RETURN a",
                    quoted(&input.anchor_label)
                ),
                BTreeMap::from([("key".into(), key.clone().into())]),
                options,
            )?;
            if anchor.rows.len() != 1 {
                return Err(ReadFailure::BadInput);
            }
            let anchor_cell = anchor.rows[0].first().ok_or_else(invalid)?;
            let anchor_id = identifier(
                body(anchor_cell, "NODE")?.get("id").ok_or_else(invalid)?,
                &generation,
            )?;
            let pattern = match input.direction {
                Direction::Outgoing => format!(
                    "-[r:{}]->(b:{})",
                    quoted(&input.relationship),
                    quoted(&input.other_label)
                ),
                Direction::Incoming => format!(
                    "<-[r:{}]-(b:{})",
                    quoted(&input.relationship),
                    quoted(&input.other_label)
                ),
            };
            let after = if input.cursor.is_some() {
                "WHERE r.uid>$after"
            } else {
                ""
            };
            let mut parameters = BTreeMap::from([
                ("key".into(), key.clone().into()),
                ("limit".into(), (input.limit + 1).into()),
            ]);
            if let Some(cursor) = &input.cursor {
                parameters.insert("after".into(), cursor.last_uid.clone().into());
            }
            let mut result = query(
                graph,
                namespace,
                original,
                &format!(
                    "MATCH(a:{} {{entity_key:$key}}){pattern} {after} RETURN a,r,b ORDER BY r.uid LIMIT $limit",
                    quoted(&input.anchor_label)
                ),
                parameters,
                options,
            )?;
            let more = result.rows.len() > input.limit;
            result.rows.truncate(input.limit);
            let mut selected = BTreeMap::new();
            selected.insert(node_key(anchor_cell, &generation)?, anchor_cell);
            for row in &result.rows {
                if row.len() != 3 {
                    return Err(invalid());
                }
                selected.insert(node_key(&row[2], &generation)?, &row[2]);
            }
            let nodes = selected
                .into_values()
                .map(|v| property_node(v, &generation))
                .collect::<Result<Vec<_>, _>>()?;
            let edges = result
                .rows
                .iter()
                .map(|r| property_edge(&r[1], &generation))
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if more {
                Some(Cursor {
                    generation,
                    fingerprint,
                    last_uid: edges.last().ok_or_else(invalid)?.uid.into(),
                })
            } else {
                None
            };
            encode(
                &NeighborhoodPage {
                    generation,
                    anchor: anchor_id,
                    nodes,
                    edges,
                    next_cursor,
                },
                maximum,
            )?
        }
    };
    original.check().map_err(ReadFailure::Query)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue};
    fn original() -> Arc<SnapshotContext> {
        Arc::new(SnapshotContext::service(Cancellation::default()))
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
            &original(),
            op,
            query,
            neighborhood,
            options(),
            1024 * 1024,
        )
        .unwrap_or_else(|_| panic!("live graph render failed"));
        serde_json::from_slice(&bytes).unwrap()
    }
    fn request(cursor: Option<Cursor>) -> NeighborhoodData {
        NeighborhoodData {
            anchor_label: "Function".into(),
            anchor_key: Anchor::String("a".into()),
            relationship: "CALLS".into(),
            other_label: "Function".into(),
            direction: Direction::Outgoing,
            limit: 1,
            cursor,
        }
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
            incoming.direction = Direction::Incoming;
            incoming.anchor_key = Anchor::String("b".into());
            let incoming = result(&graph, Operation::Neighborhood, None, Some(incoming));
            assert_eq!(incoming["edges"][0]["uid"], "01-ab");
            let mut incorrect = cursor.clone();
            incorrect.fingerprint[0] ^= 1;
            assert!(
                render_live(
                    &graph,
                    &original,
                    Operation::Neighborhood,
                    None,
                    Some(request(Some(incorrect))),
                    options(),
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
                    &original,
                    Operation::Query,
                    Some(QueryData {
                        query: "MATCH(n) DELETE n".into(),
                        parameters: BTreeMap::new()
                    }),
                    None,
                    options(),
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
            graph.remove_file("c.rs", &original).unwrap();
            graph.remove_file("b.rs", &original).unwrap();
            assert!(
                render_live(
                    &graph,
                    &original,
                    Operation::Neighborhood,
                    None,
                    Some(request(Some(cursor))),
                    options(),
                    1024 * 1024
                )
                .is_err()
            );
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
                    &original,
                    Operation::Sample,
                    None,
                    None,
                    options(),
                    1
                )
                .is_err()
            );
        })
        .await
        .unwrap();
    }
}
