//! Metadata production through the actual Native CLI without inference/model work.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_native_publication_registers_real_project_metadata() -> Result {
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn actual_native_metadata() {}\n",
    )?;
    let broker = BrokerFixture::start(&f).await?;
    let setup: Result = (|| {
        install_active_profile(&f, &broker.ready, false)?;
        for module in [
            "agents",
            "hooks",
            "memory",
            "knowledge",
            "embedding",
            "dream",
        ] {
            ConfigStore::global(&f.paths)?.set(
                ConfigKey::new(&format!("modules.{module}"))?,
                json!(false),
                Duration::from_secs(5),
                &Cancellation::default(),
            )?;
        }
        Ok(())
    })();
    if let Err(primary) = setup {
        let cleanup = broker.shutdown().await;
        return match cleanup {
            Ok(()) => Err(primary),
            Err(cleanup) => {
                Err(io::Error::other(format!("{primary}; Broker cleanup: {cleanup}")).into())
            }
        };
    }
    let mut command = f.command();
    command.env("GRAPHIT_MODULES__AST", "true").args([
        "daemon",
        "foreground",
        "--project-version",
        "main",
    ]);
    let daemon = match f.daemon_command(command).await {
        Ok(d) => d,
        Err(primary) => {
            let cleanup = broker.shutdown().await;
            return match cleanup {
                Ok(()) => Err(primary),
                Err(cleanup) => {
                    Err(io::Error::other(format!("{primary}; Broker cleanup: {cleanup}")).into())
                }
            };
        }
    };
    let result: Result = async {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let schema = f
                .command()
                .env("GRAPHIT_MODULES__AST", "true")
                .args(["ast", "schema", "--project-version", "main"])
                .output()?;
            if schema.status.success() {
                let data: serde_json::Value = serde_json::from_slice(&schema.stdout)?;
                if data["nodes"].as_array().is_some_and(|n| !n.is_empty()) {
                    break;
                }
            }
            if Instant::now() >= until {
                return Err("actual Main AST unavailable for Native metadata publication".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let identity = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing producer")?;
        let producer = identity.id();
        let absent: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/v1/hub/catalog/projects", broker.ready.endpoint))
            .bearer_auth(&broker.ready.access_bearer)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(
            absent["projects"],
            json!([]),
            "no manual project metadata seed"
        );
        let receipt = publish_command(&f, "ast", "fixture/native-metadata", "release/metadata")?;
        assert_eq!(receipt["status"], "published");
        assert_eq!(receipt["publication"]["catalog"], "published");
        assert_eq!(receipt["project_metadata"], "published");
        let duplicate = publish_command(&f, "ast", "fixture/native-metadata", "release/metadata")?;
        assert_eq!(duplicate["status"], receipt["status"]);
        assert_eq!(duplicate["publication"], receipt["publication"]);
        assert_eq!(duplicate["project_metadata"], "unchanged");
        let store = broker.store(producer).await?;
        let key = object_store::path::Path::from(format!(
            "{}/{}",
            broker.ready.prefix,
            graphit_hub::project_metadata::project_metadata_key(producer)
        ));
        let bytes = store.get(&key).await?.bytes().await?;
        let metadata =
            graphit_hub::project_metadata::HubProjectMetadata::decode(&bytes, producer, 65536)?;
        assert_eq!(metadata.name, identity.metadata().name);
        let page: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/v1/hub/catalog/projects", broker.ready.endpoint))
            .bearer_auth(&broker.ready.access_bearer)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(
            page["projects"],
            json!([{"id":producer,"name":metadata.name}])
        );
        // Reject adopting a document for another PROJECT even at this authorized key.
        let foreign = ProjectIdentityStore::at(&f.projects[1])
            .load()?
            .ok_or("missing sibling")?;
        let hostile = graphit_hub::project_metadata::HubProjectMetadata {
            id: foreign.id(),
            name: foreign.metadata().name.clone(),
        }
        .encode(65536)?;
        store.put(&key, hostile.into()).await?;
        let denied = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "hub",
                "publish-native",
                "--provider",
                PROVIDER,
                "--kind",
                "ast",
                "--id",
                "fixture/native-metadata",
                "--version",
                "release/rejected",
                "--name",
                "Rejected",
                "--project-version",
                "main",
            ])
            .output()?;
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        Ok(())
    }
    .await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = broker.shutdown().await;
    match (result,daemon_cleanup,broker_cleanup){(Ok(()),Ok(()),Ok(()))=>Ok(()),(primary,daemon,broker)=>Err(io::Error::other(format!("Native metadata journey: {primary:?}; daemon cleanup: {daemon:?}; Broker cleanup: {broker:?}")).into())}
}
