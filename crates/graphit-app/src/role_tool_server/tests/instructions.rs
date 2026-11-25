use super::*;

fn payloads(markdown: &str) -> Vec<Value> {
    markdown
        .split("```json\n")
        .skip(1)
        .map(|body| serde_json::from_str(body.split_once("```").unwrap().0).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_memory_skill_examples_execute_actual_reads_and_authorized_capture() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let entry = call(
        &client,
        "graphit_module_skill",
        json!({"input":{"module":"memory"}}),
        false,
    )
    .await;
    let markdown = entry["source"].as_str().unwrap();
    assert!(markdown.contains("Memory for an authenticated Role"));
    let examples = payloads(markdown);
    assert_eq!(examples.len(), 4);
    let source_example = examples[2].clone();
    let search_example = examples[1].clone();
    let mut companion = Value::Null;
    for (tool, payload) in [
        "graphit_memory_mandatory",
        "graphit_memory_search",
        "graphit_memory_source",
        "graphit_module_skill",
    ]
    .into_iter()
    .zip(examples)
    {
        serde_json::from_value::<Arguments>(payload.clone()).unwrap();
        companion = call(&client, tool, payload, false).await;
    }
    let examples = payloads(companion["source"].as_str().unwrap());
    assert_eq!(examples.len(), 3);
    let mut created = Value::Null;
    for (tool, payload) in [
        "graphit_memory_search",
        "graphit_memory_insert",
        "graphit_memory_source",
    ]
    .into_iter()
    .zip(examples)
    {
        serde_json::from_value::<Arguments>(payload.clone()).unwrap();
        let result = call(&client, tool, payload, false).await;
        if tool == "graphit_memory_insert" {
            created = result;
        }
    }
    let id = &created["result"]["id"];
    assert!(id.is_string());
    // The guide requires replacing its illustrative ID with an observed one.
    let mut observed_source = source_example;
    observed_source["input"]["id"] = id.clone();
    let found = call(&client, "graphit_memory_source", observed_source, false).await;
    assert_eq!(found["kind"], "found");
    assert!(found["selection"].to_string().contains("webhook replay"));
    let found = call(&client, "graphit_memory_search", search_example, false).await;
    assert_eq!(found["kind"], "published");
    assert_eq!(found["hits"][0]["current"]["id"], *id);
    for input in [
        json!({"text":"webhook replay","top_k":5,"scope":"user"}),
        json!({"query":"webhook replay","top_k":5,"ai_optimized":true}),
        json!({"text":"webhook replay","top_k":5,"exclude_mandatory":true}),
    ] {
        assert_eq!(
            call(
                &client,
                "graphit_memory_search",
                json!({"input":input}),
                true
            )
            .await["code"],
            "invalid_arguments"
        );
    }
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_knowledge_skill_examples_execute_search_source_and_native_references() {
    let (_temp, server, raw) = fixture();
    references::knowledge_with_references(&server, &raw).await;
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    for (reference, tools) in [
        (
            None,
            vec![
                "graphit_knowledge_search",
                "graphit_wiki_source",
                "graphit_knowledge_references",
                "graphit_wiki_log",
                "graphit_module_skill",
            ],
        ),
        (
            Some("references/documentation-design.md"),
            vec![
                "graphit_knowledge_search",
                "graphit_wiki_source",
                "graphit_module_skill",
            ],
        ),
        (
            Some("references/worked-domain.md"),
            vec!["graphit_knowledge_search", "graphit_wiki_source"],
        ),
    ] {
        let mut input = json!({"module":"knowledge"});
        if let Some(reference) = reference {
            input["reference"] = json!(reference);
        }
        let rendered = call(
            &client,
            "graphit_module_skill",
            json!({"input":input}),
            false,
        )
        .await;
        let markdown = rendered["source"].as_str().unwrap();
        assert!(!markdown.contains("\"project_dir\""));
        assert!(!markdown.contains("\"ai_optimized\""));
        let examples = payloads(markdown);
        assert_eq!(examples.len(), tools.len());
        for (tool, payload) in tools.into_iter().zip(examples) {
            serde_json::from_value::<Arguments>(payload.clone()).unwrap();
            let result = call(&client, tool, payload, false).await;
            match tool {
                "graphit_knowledge_search" => {
                    assert_eq!(result["kind"], "hits");
                    assert!(
                        result["hits"][0]["preview"]["text"]
                            .as_str()
                            .unwrap()
                            .contains("Deployment policy")
                    );
                }
                "graphit_wiki_source" => assert_eq!(result["kind"], "found"),
                "graphit_knowledge_references" => {
                    assert_eq!(result["published"]["read"]["outgoing"]["total"], 2)
                }
                "graphit_wiki_log" => {
                    assert_eq!(result["kind"], "published");
                    assert_eq!(result["entries"].as_array().unwrap().len(), 1);
                    assert!(
                        result["entries"][0]["changes"]["added"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|slug| slug == "alpha")
                    );
                    assert!(result["generation"].get("corpus_uri").is_none());
                }
                "graphit_module_skill" => assert!(result["source"].is_string()),
                _ => unreachable!(),
            }
        }
    }
    for input in [
        json!({"query":"deployment policy","context":"foreign@v1"}),
        json!({"query":"deployment policy","wikis":[],"hub_refs":[]}),
    ] {
        assert_eq!(
            call(
                &client,
                "graphit_knowledge_search",
                json!({"input":input}),
                true
            )
            .await["code"],
            "invalid_arguments"
        );
    }
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
