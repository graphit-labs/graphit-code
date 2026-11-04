//! Actual CLI/Source catalog reads with real Broker/S3/Auth and published metadata.
use super::super::super::remote_artifacts::search as artifact_search;
use super::*;
use graphit_hub::project_metadata::{HubProjectMetadata, project_metadata_key};

fn projects(f: &Fixture, arguments: &[&str]) -> Result<serde_json::Value> {
    let output = f.command().args(arguments).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "catalog CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if value["status"] != "ok"
        || !value["projects"].is_array()
        || value.get("subject").is_some()
        || value.get("authorization_revision").is_some()
    {
        return Err("catalog public DTO diverged or leaked transport authority".into());
    }
    Ok(value)
}
fn refused(f: &Fixture, arguments: &[&str], code: &str, exit: i32) -> Result {
    let output = f.command().args(arguments).output()?;
    if output.status.code() != Some(exit) || !String::from_utf8_lossy(&output.stderr).contains(code)
    {
        return Err(io::Error::other(format!(
            "catalog rejection/exit diverged: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}
fn rows(value: &serde_json::Value) -> Result<&Vec<serde_json::Value>> {
    value["projects"]
        .as_array()
        .ok_or_else(|| "missing projects".into())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_hub_projects_reads_published_metadata_and_pages_current_standalone_acl() -> Result
{
    let f = Fixture::new().await?;
    let (broker, storage) = BrokerFixture::start_standalone(&f, true).await?;
    if let Err(cause) = (|| {
        install_active_profile(&f, &broker.ready, false)?;
        local_profiles(&f, &broker.ready, &storage)
    })() {
        return joined(Err(cause), broker.shutdown().await);
    }
    let daemon = match f.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(cause) => return joined(Err(cause), broker.shutdown().await),
    };
    let result:Result=async{
        let identity=ProjectIdentityStore::at(&f.projects[0]).load()?.ok_or("missing first identity")?;
        let second=ProjectIdentityStore::at(&f.projects[1]).load()?.ok_or("missing second identity")?;
        let producer=identity.id();let producer_text=producer.to_string();
        let config=ConfigSnapshot::new(f.paths.brand(),[HubResourcePolicy::distribution_defaults(f.paths.brand())?])?;
        let policy=HubResourcePolicy::resolve(&config)?;
        let source=SourceRef::resources(&f.projects[0],producer,ArtifactType::Skill,ArtifactId::new("fixture/catalog-source",policy.projection())?,ConcreteVersion::new("catalog/one",policy.projection())?,policy.projection())?;
        fs::create_dir_all(source.directory().as_path())?;
        fs::write(source.directory().as_path().join("SKILL.md"),"---\nname: catalog-source\ndescription: Real catalog publication\n---\nAvailable catalog fixture\n")?;
        // No PROJECT metadata is seeded for this Broker journey: the Rust publisher must write it.
        let published=f.command().args(["--project-version","main","hub","publish","--provider",PROVIDER,"--kind","skill","--id","fixture/catalog-source","--version","catalog/one","--name","Artifact name is not the project name","--set-latest"]).output()?;
        if !published.status.success(){return Err(io::Error::other(format!("real metadata-producing FILE publication failed: {}",String::from_utf8_lossy(&published.stderr))).into());}
        let store=broker.store(producer).await?;
        let persisted=store.get(&object_key(&broker.ready.prefix,&project_metadata_key(producer))).await?.bytes().await?;
        let project=HubProjectMetadata::decode(&persisted,producer,policy.content().max_input_bytes)?;
        assert_eq!(project.name,identity.metadata().name);
        let broker_page=projects(&f,&["hub","projects","--provider",PROVIDER,"--id",&producer_text])?;
        assert_eq!(rows(&broker_page)?.len(),1);assert_eq!(broker_page["projects"][0]["id"],producer_text);assert_eq!(broker_page["projects"][0]["name"],project.name);assert!(broker_page["next_cursor"].is_null());
        let prefix_page=projects(&f,&["hub","projects","--provider",PROVIDER,"--name-prefix",&project.name])?;
        assert!(rows(&prefix_page)?.iter().any(|p|p["id"]==producer_text));
        refused(&f,&["hub","projects","--provider",PROVIDER,"--limit","0"],"invalid_arguments",2)?;
        refused(&f,&["hub","projects","--provider",PROVIDER,"--profile",PROFILE],"invalid_arguments",2)?;

        // A second real FILE publication supplies actual manifests for standalone search pages.
        let second_id=ArtifactId::new("fixture/catalog-second",policy.projection())?;
        let second_source=SourceRef::resources(&f.projects[0],producer,ArtifactType::Skill,second_id.clone(),ConcreteVersion::new("catalog/one",policy.projection())?,policy.projection())?;
        fs::create_dir_all(second_source.directory().as_path())?;
        fs::write(second_source.directory().as_path().join("SKILL.md"),"---\nname: catalog-second\ndescription: Second real catalog publication\n---\nSecond catalog item\n")?;
        let second_published=f.command().args(["--project-version","main","hub","publish","--provider",PROVIDER,"--kind","skill","--id",second_id.as_str(),"--version","catalog/one","--name","Second catalog artifact"]).output()?;
        if !second_published.status.success(){return Err(io::Error::other(format!("second actual FILE catalog producer failed: {}",String::from_utf8_lossy(&second_published.stderr))).into());}
        let standalone=storage_client(&storage)?;
        let base=["hub","projects","--transport","standalone","--provider",LOCAL,"--profile",PROFILE];
        let issuer=URL_SAFE_NO_PAD.encode(&broker.ready.issuer);let subject=URL_SAFE_NO_PAD.encode(&broker.ready.subject);
        let user_key=format!("v1/users/{issuer}/{subject}/projects.json");
        for key in ["v1/global/projects.json","v1/anonymous/projects.json","v1/authenticated/projects.json",&user_key]{put_json(&standalone,&storage.prefix,key,json!({"v":1,"projects":[]})).await?;}
        // Standalone paging/negative cases use codec-authored fixtures; they do not claim a standalone publication journey.
        let second_metadata=HubProjectMetadata{id:second.id(),name:second.metadata().name.clone()};
        for metadata in [&project,&second_metadata]{standalone.put(&object_key(&storage.prefix,&project_metadata_key(metadata.id)),metadata.encode(policy.content().max_input_bytes)?.into()).await?;}
        assert!(rows(&projects(&f,&base)?)?.is_empty());
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[{"all":true}]})).await?;
        let mut args=base.to_vec();args.extend(["--limit","1"]);
        let first=projects(&f,&args)?;assert_eq!(rows(&first)?.len(),1);
        let cursor=first["next_cursor"].as_str().ok_or("missing real next cursor")?.to_owned();
        let cursor_json:serde_json::Value=serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor)?)?;
        assert_eq!(cursor_json["after"],first["projects"][0]["id"]);assert!(cursor_json["broker"].is_null());
        let mut next=args.clone();next.extend(["--cursor",&cursor]);
        let last=projects(&f,&next)?;assert_eq!(rows(&last)?.len(),1);assert_ne!(last["projects"][0]["id"],first["projects"][0]["id"]);assert!(last["next_cursor"].is_null());
        let mut wrong_query=base.to_vec();wrong_query.extend(["--limit","2","--cursor",&cursor]);refused(&f,&wrong_query,"catalog_restart_required",1)?;
        let mut wrong_identity=next.clone();wrong_identity.extend(["--identity-provider",PROVIDER]);refused(&f,&wrong_identity,"catalog_restart_required",1)?;
        // Copy only the two actual published manifest documents into this independent
        // standalone fixture. This tests the reader, not a standalone publisher.
        let mut manifest_documents=Vec::new();
        for id in [ArtifactId::new("fixture/catalog-source",policy.projection())?,second_id] {
            let key=graphit_hub::remote_storage::registry_key(producer,ArtifactType::Skill,&id)?;
            let bytes=store.get(&object_key(&broker.ready.prefix,&key)).await?.bytes().await?;
            standalone.put(&object_key(&storage.prefix,&key),bytes.clone().into()).await?;
            manifest_documents.push((key,bytes));
        }
        let producer_text_for_search=producer.to_string();
        let search_args=["hub","search","--transport","standalone","--provider",LOCAL,"--profile",PROFILE,"--producer",&producer_text_for_search,"--kind","skill","--limit","1"];
        let artifact_first=artifact_search(&f,&search_args)?;
        let artifact_cursor=artifact_first["next_cursor"].as_str().ok_or("missing real standalone artifact cursor")?.to_owned();
        let mut artifact_next=search_args.to_vec();artifact_next.extend(["--cursor",&artifact_cursor]);
        let artifact_last=artifact_search(&f,&artifact_next)?;
        assert_eq!(artifact_last["artifacts"].as_array().unwrap().len(),1);assert!(artifact_last["next_cursor"].is_null());
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[{"id":producer}]})).await?;
        refused(&f,&artifact_next,"catalog_restart_required",1)?;
        // Current deny wins before a protected manifest LIST/GET, including a malformed object.
        standalone.put(&object_key(&storage.prefix,&manifest_documents[0].0),b"{invalid protected manifest".to_vec().into()).await?;
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[]})).await?;
        refused(&f,&artifact_next,"authentication_or_permission_denied",1)?;
        standalone.put(&object_key(&storage.prefix,&manifest_documents[0].0),manifest_documents[0].1.clone().into()).await?;
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[{"name_prefix":project.name}]})).await?;
        refused(&f,&next,"catalog_restart_required",1)?;
        let restricted=projects(&f,&base)?;assert!(rows(&restricted)?.iter().all(|p|p["name"].as_str().is_some_and(|name|name.starts_with(&project.name))));assert!(rows(&restricted)?.iter().any(|p|p["id"]==producer_text));
        // A cursor cannot cause a metadata GET outside a finite project ACL.
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[{"id":producer},{"id":second.id()}]})).await?;
        let finite=projects(&f,&args)?;
        let finite_cursor=finite["next_cursor"].as_str().ok_or("missing finite ACL cursor")?;
        let mut forged:serde_json::Value=serde_json::from_slice(&URL_SAFE_NO_PAD.decode(finite_cursor)?)?;
        let foreign="01D39ZY06FGSCTVN4T2V9PKHG1".parse()?;
        assert_ne!(foreign,producer);assert_ne!(foreign,second.id());
        // Decoding this object would produce invalid_remote_catalog, not the expected restart.
        standalone.put(&object_key(&storage.prefix,&project_metadata_key(foreign)),b"{invalid metadata".to_vec().into()).await?;
        forged["after"]=json!(foreign);
        let forged_cursor=URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged)?);
        let mut forged_args=args.clone();forged_args.extend(["--cursor",&forged_cursor]);
        refused(&f,&forged_args,"catalog_restart_required",1)?;
        standalone.put(&object_key(&storage.prefix,&project_metadata_key(foreign)),HubProjectMetadata{id:foreign,name:"Foreign catalog cursor boundary".into()}.encode(policy.content().max_input_bytes)?.into()).await?;
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[]})).await?;
        put_json(&standalone,&storage.prefix,&user_key,json!({"v":1,"projects":[{"id":producer}]})).await?;
        assert!(rows(&projects(&f,&base)?)?.is_empty());
        let mut authenticated=base.to_vec();authenticated.extend(["--identity-provider",PROVIDER]);let named=projects(&f,&authenticated)?;assert_eq!(rows(&named)?.len(),1);assert_eq!(named["projects"][0]["id"],producer_text);
        standalone.put(&object_key(&storage.prefix,"v1/global/projects.json"),Vec::<u8>::new().into()).await?;
        let empty_level=projects(&f,&authenticated)?;assert_eq!(rows(&empty_level)?.len(),1);assert_eq!(empty_level["projects"][0]["id"],producer_text);
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":2,"projects":[]})).await?;
        refused(&f,&base,"invalid_remote_catalog",1)?;
        put_json(&standalone,&storage.prefix,"v1/global/projects.json",json!({"v":1,"projects":[{"all":true}]})).await?;
        standalone.put(&object_key(&storage.prefix,&project_metadata_key(second.id())),project.encode(policy.content().max_input_bytes)?.into()).await?;
        refused(&f,&base,"invalid_remote_catalog",1)?;
        Ok(())
    }.await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = broker.shutdown().await;
    joined(result, joined(daemon_cleanup, broker_cleanup))
}
