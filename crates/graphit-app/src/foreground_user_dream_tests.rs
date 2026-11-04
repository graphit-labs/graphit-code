//! USER consumer journeys through production Foreground policies and native SDKs.
use super::*;
use crate::native_install_http::auth_fixture as native_support;
use graphit_auth::{UserMemoryWriteOperation, profile::*, store::AuthStore};
use graphit_core::{
    artifact_store::{StoreOwner, UserIdentity},
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
};
use graphit_records::{
    memory_store::{DeleteMemory, MemoryReference, MemoryReferenceScope},
    reference_projection::ReferenceTargetKind,
    user_memory_store::{
        NativeUserMemoryStore, UserMemoryHistory, UserMemoryListRead, UserMemoryOwner,
        UserMemoryScopeId,
    },
};
fn config(f: &Fixture, enabled: bool) -> Result<ConfigSnapshot> {
    let mut entries = vec![
        (ConfigKey::new("modules.ast")?, json!(false)),
        (ConfigKey::new("modules.knowledge")?, json!(false)),
        (ConfigKey::new("modules.embedding")?, json!(false)),
        (ConfigKey::new("modules.task")?, json!(false)),
        (ConfigKey::new("modules.hooks")?, json!(true)),
        (ConfigKey::new("modules.memory")?, json!(true)),
        (ConfigKey::new("broker.allow_loopback_http")?, json!(true)),
        (ConfigKey::new("dream.enabled")?, json!(enabled)),
        (
            ConfigKey::new("dream.model")?,
            json!("literal-model;$(no-shell)"),
        ),
        (ConfigKey::new("dream.program")?, json!(f.program)),
        (ConfigKey::new("dream.idle_timeout")?, json!(1)),
        (ConfigKey::new("dream.max_duration")?, json!(f.duration)),
        (ConfigKey::new("dream.retries")?, json!(f.retries)),
        (ConfigKey::new("dream.backoff")?, json!(1)),
    ];
    if let Some(limit) = f.input_limit {
        entries.push((
            ConfigKey::new("hub.limits.content.scalar_bytes")?,
            json!(limit),
        ));
        entries.push((
            ConfigKey::new("hub.limits.content.input_bytes")?,
            json!(limit),
        ));
    }
    Ok(ConfigSnapshot::new(
        f.installation.brand(),
        [ConfigDocument::new(
            f.installation.brand(),
            ConfigSource::Command,
            entries,
        )?],
    )?)
}
fn auth_store(
    f: &Fixture,
    owner: &ForegroundSource,
    request: &RequestContext<'_>,
) -> Result<AuthStore> {
    let StoreOwner::Installation { installation_id } = owner.auth.installation().live.owner()
    else {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    };
    let live = CapturedPublicationScope::installation(
        &f.installation,
        installation_id.clone(),
        owner
            .auth
            .installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
        owner.scope,
        request.snapshot(),
    )?;
    Ok(AuthStore::new(
        AuthDocumentStore::capture(
            live,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536)?,
            request.snapshot(),
        )?,
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        request,
    )?)
}
fn runtime<'a>(
    owner: &'a ForegroundSource,
    request: &'a RequestContext<'a>,
) -> Result<graphit_auth::AuthenticatedCaller<'a>> {
    let key = owner.auth.runtime().check(request.snapshot())?;
    let raw = std::str::from_utf8(key.secret(request.snapshot())?)?;
    Ok(owner.auth.issuer().authenticate_runtime(raw, request)?)
}
fn user_capture(
    f: &Fixture,
    owner: &ForegroundSource,
    caller: &graphit_auth::AuthenticatedCaller<'_>,
    request: &RequestContext<'_>,
) -> Result<CapturedPublicationScope> {
    let subject = owner.auth.issuer().user_from_authenticated(caller)?;
    let StoreOwner::Installation { installation_id } = owner.auth.installation().live.owner()
    else {
        return Err(io::Error::from(io::ErrorKind::InvalidData).into());
    };
    Ok(CapturedPublicationScope::user(
        &f.installation,
        installation_id.clone(),
        UserIdentity::new(subject.issuer()?.into(), subject.subject()?.into())?,
        owner.scope,
        request.snapshot(),
    )?)
}
async fn seed_user(
    f: &Fixture,
    owner: &ForegroundSource,
    count: usize,
    body_bytes: usize,
) -> Result<Vec<Memory>> {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let write = owner.auth.issuer().admit_authenticated_user_memory_write(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
        UserMemoryWriteOperation::Memory,
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, false)?,
    )?;
    let StoreOwner::User {
        installation_id,
        identity,
    } = live.owner()
    else {
        unreachable!()
    };
    let sid = UserMemoryScopeId::from_owner(&UserMemoryOwner {
        installation_id: installation_id.clone(),
        identity: identity.clone(),
    })?;
    let mut records = Vec::new();
    for index in 0..count {
        records.push(
            NativeUserMemoryStore::create(
                request.snapshot(),
                &write,
                CreateMemory {
                    idempotency_key: format!("user-seed-{index}"),
                    title: format!("fixture-{index}"),
                    body: format!("USER fato confirmado α {index} {}", "\"".repeat(body_bytes)),
                    attributes: MemoryAttributes {
                        classification: if index == 1 {
                            MemoryClass::Decision
                        } else {
                            MemoryClass::Fact
                        },
                        mandatory: index == 1,
                        important: index == 1,
                    },
                    tags: vec![format!("tag-{index}")],
                    references: Some(vec![MemoryReference {
                        target_kind: ReferenceTargetKind::Task,
                        scope: MemoryReferenceScope::User(sid.as_str().into()),
                        id: format!("literal-task-{index}"),
                        context: None,
                        relation: "supports".into(),
                        field: Some("body".into()),
                    }]),
                },
                policy.memory_write,
            )
            .await?,
        );
    }
    Ok(records)
}
async fn snapshot(
    f: &Fixture,
    owner: &ForegroundSource,
    ids: &[graphit_records::memory_store::MemoryId],
) -> Result<(UserMemoryListRead, Vec<UserMemoryHistory>)> {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let read = owner.auth.issuer().admit_authenticated_user_memory_read(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, false)?,
    )?;
    let list = NativeUserMemoryStore::list(
        request.snapshot(),
        &read,
        MemoryListFilter::default(),
        policy.memory_read,
    )
    .await?;
    let mut history = Vec::new();
    for id in ids {
        if let Some(h) =
            NativeUserMemoryStore::history(request.snapshot(), &read, *id, policy.memory_read)
                .await?
        {
            history.push(h);
        }
    }
    Ok((list, history))
}
fn bootstrap(f: &Fixture) -> Result<ForegroundSource> {
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel)?;
    Ok(ForegroundSource::start(
        &f.installation,
        &[],
        0,
        &config(f, false)?,
        NativeBinding::Codex,
        &original,
    )?)
}
async fn bootstrap_with_profiles(
    f: &Fixture,
    profiles: &[(&str, &native_support::Broker)],
) -> Result<ForegroundSource> {
    let owner = bootstrap(f)?;
    let cancel = Cancellation::default();
    let request = f.policy.request_context(Instant::now(), &cancel)?;
    let store = auth_store(f, &owner, &request)?;
    for &(name, broker) in profiles.iter().rev() {
        native_profile(
            &store,
            broker,
            name,
            "broker-api",
            native_support::now() + 600,
            &request,
        );
    }
    // Foreground captures service permissions from the configured providers at startup.
    owner.shutdown().await?;
    bootstrap(f)
}
struct Ready {
    _directory: tempfile::TempDir,
    listener: tokio::net::UnixListener,
}
async fn start(f: &Fixture, mode: &str) -> Result<(ForegroundSource, Ready)> {
    let directory = tempfile::Builder::new()
        .prefix("user-ready-")
        .tempdir_in(f.temp.path())?;
    let signal = directory.path().join("started.sock");
    let listener = tokio::net::UnixListener::bind(&signal)?;
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel)?;
    let owner = ForegroundSource::start_and_publish(
        &f.installation,
        &[],
        0,
        &config(f, true)?,
        NativeBinding::Codex,
        vec![
            ("USER_DREAM_READY".into(), signal.into_os_string()),
            (
                "PATH".into(),
                std::env::var_os("PATH").ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?,
            ),
            ("DREAM_FIXTURE_MODE".into(), mode.into()),
            (
                "DREAM_FIXTURE_LOG".into(),
                f.temp.path().join("user-attempts.jsonl").into_os_string(),
            ),
        ],
        &original,
    )
    .await?;
    Ok((
        owner,
        Ready {
            _directory: directory,
            listener,
        },
    ))
}
async fn iteration(owner: &mut ForegroundSource) -> Result {
    let user = owner
        .user_dream
        .as_mut()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
    let completed = user.iteration_completion();
    tokio::time::timeout(Duration::from_secs(20), async {
        tokio::select! {
            biased;
            () = completed.notified() => Ok::<(), Box<dyn std::error::Error>>(()),
            result = user.next() => {
                result?;
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "USER controller ended before iteration completion",
                ).into())
            }
        }
    })
    .await
    .map_err(|error| io::Error::other(format!("USER iteration completion: {error}")))?
}
fn prepare_peer(f: &Fixture) -> Result {
    fs::write(&f.program, USER_PEER)?;
    fs::set_permissions(&f.program, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn same_memories(actual: &[Memory], expected: &[Memory]) -> bool {
    actual.len() == expected.len()
        && expected
            .iter()
            .all(|expected| actual.iter().any(|actual| actual == expected))
}
fn attempts(f: &Fixture) -> Result<Vec<serde_json::Value>> {
    match fs::read_to_string(f.temp.path().join("user-attempts.jsonl")) {
        Ok(text) => Ok(text
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
async fn accepted_attempt(
    ready: &Ready,
    owner: &mut ForegroundSource,
) -> Result<tokio::net::UnixStream> {
    use tokio::io::AsyncReadExt;
    let user = owner
        .user_dream
        .as_mut()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
    let readiness = async {
        let (mut stream, _) = ready.listener.accept().await?;
        let mut started = [0u8; 1];
        stream.read_exact(&mut started).await?;
        if started != [1] {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok::<_, io::Error>(stream)
    };
    tokio::time::timeout(Duration::from_secs(15), async {
        tokio::select! {
            stream = readiness => Ok::<_, Box<dyn std::error::Error>>(stream?),
            result = user.next() => {
                result?;
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "USER controller ended before native readiness",
                ).into())
            }
        }
    })
    .await
    .map_err(|error| io::Error::other(format!("USER native readiness: {error}")))?
}
fn native_profile(
    store: &AuthStore,
    broker: &native_support::Broker,
    key: &str,
    audience: &str,
    expiry: u64,
    request: &RequestContext<'_>,
) {
    let binding = BrokerService {
        endpoint: broker.origin.clone(),
        mcp_resource: None,
        allow_anonymous: false,
    };
    let data = ProfileData {
        name: key.into(),
        provider: key.into(),
        username: "display-not-principal".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: broker.origin.clone(),
            subject: "native-fixture-sub".into(),
            s3_disabled: true,
            tokens: OidcTokens {
                access_token: Secret::new(broker.token(expiry, audience).into()),
                refresh_token: None,
                id_token: Secret::new("historical-fixture-id-token".into()),
                expires_at: i64::try_from(Duration::from_secs(expiry).as_micros()).unwrap(),
            },
        },
    };
    let result = store
        .mutate(store.load(request).unwrap(), request, |doc, request| {
            if doc.provider(key).is_none() {
                doc.add_provider(
                    key.into(),
                    ProviderConfig {
                        kind: ProviderKind::Broker(binding),
                        embedding: AiService::Disabled,
                        rerank: AiService::Disabled,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::BrokerScope,
                        },
                    },
                    doc.revision(),
                    1,
                    request,
                )?;
            }
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider(key).unwrap().revision(),
                profile_revision: doc.profile(key).map(|profile| profile.revision()),
            };
            let updated = doc
                .profile(key)
                .map_or(1, |profile| profile.updated_at() + 1);
            doc.replace_profile_data(data, expected, updated, request)?;
            doc.activate(key, doc.revision(), request)
        })
        .unwrap();
    assert!(result.failure().is_none() && result.cleanup_failure().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_startup_without_project_applies_native_fold_delete_and_preserves_metadata() -> Result
{
    let f = Fixture::new()?;
    prepare_peer(&f)?;
    let broker = native_support::Broker::start().await;
    let seeded = bootstrap_with_profiles(&f, &[("user-a", &broker)]).await?;
    let records = seed_user(&f, &seeded, 4, 0).await?;
    assert!(seeded.projects.as_ref().unwrap().is_empty());
    seeded.shutdown().await?;
    let (mut owner, ready) = start(&f, "slow").await?;
    assert!(owner.projects.as_ref().unwrap().is_empty());
    assert!(owner.dream.is_none());
    assert!(owner.user_dream.is_some());
    drop(accepted_attempt(&ready, &mut owner).await?);
    owner
        .user_dream
        .as_ref()
        .unwrap()
        .activity_sender()
        .send_replace(Some(Instant::now()));
    iteration(&mut owner).await?;
    iteration(&mut owner).await?;
    let ids = records.iter().map(|m| m.id).collect::<Vec<_>>();
    let (list, history) = snapshot(&f, &owner, &ids).await?;
    let UserMemoryListRead::Published { memories, .. } = list else {
        panic!("published USER")
    };
    assert_eq!(memories.len(), 2);
    let survivor = memories.iter().find(|m| m.id == records[0].id).unwrap();
    assert_eq!(survivor.body, "USER confirmed α 0; USER confirmed α 1");
    assert_eq!(survivor.revision, records[0].revision + 1);
    assert_eq!(survivor.attributes.classification, MemoryClass::Decision);
    assert!(survivor.attributes.mandatory && survivor.attributes.important);
    assert_eq!(survivor.references, records[0].references);
    assert!(survivor.tags.contains(&"tag-0".into()));
    for removed in [records[1].id, records[3].id] {
        assert!(
            history
                .iter()
                .find(|h| h.current.id == removed)
                .unwrap()
                .deleted
        );
    }
    assert_eq!(attempts(&f)?.len(), 2);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        attempts(&f)?.len(),
        2,
        "success waits for new human activity"
    );
    owner
        .user_dream
        .as_ref()
        .unwrap()
        .activity_sender()
        .send_replace(Some(Instant::now()));
    iteration(&mut owner).await?;
    assert_eq!(attempts(&f)?.len(), 3);
    assert!(owner.projects.as_ref().unwrap().is_empty());
    owner.shutdown().await?;
    broker.shutdown().await;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_no_native_and_no_corpus_startup_never_resolve_or_launch_a_program() -> Result {
    let mut f = Fixture::new()?;
    f.program = f.temp.path().join("deliberately-absent-program");
    let broker = native_support::Broker::start().await;
    let (mut absent, _ready_absent) = start(&f, "noop").await?;
    iteration(&mut absent).await?;
    assert!(attempts(&f)?.is_empty());
    absent.shutdown().await?;
    let seed = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
    let (list, _) = snapshot(&f, &seed, &[]).await?;
    assert!(matches!(list, UserMemoryListRead::NoCorpus));
    seed.shutdown().await?;
    let (mut owner, _ready) = start(&f, "noop").await?;
    iteration(&mut owner).await?;
    assert!(attempts(&f)?.is_empty());
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(attempts(&f)?.is_empty());
    owner.shutdown().await?;
    broker.shutdown().await;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_native_failure_and_escaped_bound_never_launch_or_apply() -> Result {
    for invalid in [false, true] {
        let f = Fixture::new()?;
        let variable_limit = f.policy.content().max_scalar_bytes;
        prepare_peer(&f)?;
        let broker = native_support::Broker::start().await;
        let seed = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
        let cancel = Cancellation::default();
        let request = f.policy.request_context(Instant::now(), &cancel)?;
        let seeded_records = if invalid {
            None
        } else {
            Some(seed_user(&f, &seed, 1, variable_limit / 4 * 3).await?)
        };
        seed.shutdown().await?;
        let (mut owner, _ready) = start(&f, "fold").await?;
        let records = if let Some(records) = seeded_records {
            records
        } else {
            // Complete the real startup publication and NoCorpus iteration first.
            // The next activity trigger exercises the USER Native failure itself.
            iteration(&mut owner).await?;
            let records = seed_user(&f, &owner, 1, 0).await?;
            native_profile(
                &auth_store(&f, &owner, &request)?,
                &broker,
                "user",
                "wrong-audience",
                native_support::now() + 600,
                &request,
            );
            owner
                .user_dream
                .as_ref()
                .unwrap()
                .activity_sender()
                .send_replace(Some(Instant::now()));
            records
        };
        if !invalid {
            assert!(records[0].body.len() < variable_limit);
            // The exact JSON body string is contained in the consumer corpus.
            assert!(serde_json::to_string(&records[0].body)?.len() > variable_limit);
        }
        let error = tokio::time::timeout(
            Duration::from_secs(20),
            owner.user_dream.as_mut().unwrap().next(),
        )
        .await?
        .unwrap_err();
        if invalid {
            assert!(
                matches!(
                    error
                        .get_ref()
                        .and_then(|cause| cause.downcast_ref::<graphit_auth::ServiceUseError>()),
                    Some(graphit_auth::ServiceUseError::Broker(
                        graphit_auth::broker::BrokerError::InvalidToken
                    ))
                ),
                "{error:?}"
            );
        } else {
            assert!(error.to_string().contains("corpus encoding or bound"));
        }
        assert!(attempts(&f)?.is_empty());
        native_profile(
            &auth_store(&f, &owner, &request)?,
            &broker,
            "user",
            "broker-api",
            native_support::now() + 600,
            &request,
        );
        let ids = records.iter().map(|m| m.id).collect::<Vec<_>>();
        let (list, _) = snapshot(&f, &owner, &ids).await?;
        assert!(
            matches!(list,UserMemoryListRead::Published { memories,.. } if same_memories(&memories,&records))
        );
        owner.shutdown().await?;
        broker.shutdown().await;
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_malformed_partial_and_unsuccessful_native_proposals_are_data_not_effects() -> Result {
    for mode in ["malformed", "partial", "exit_failure"] {
        let f = Fixture::new()?;
        prepare_peer(&f)?;
        let broker = native_support::Broker::start().await;
        let seed = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
        let records = seed_user(&f, &seed, 4, 0).await?;
        seed.shutdown().await?;
        let (mut owner, _ready) = start(&f, mode).await?;
        let error = tokio::time::timeout(
            Duration::from_secs(20),
            owner.user_dream.as_mut().unwrap().next(),
        )
        .await?
        .unwrap_err();
        assert!(error.to_string().contains(if mode == "malformed" {
            "recipe rejected"
        } else {
            "native proposal failed"
        }));
        let (list, _) = snapshot(&f, &owner, &[]).await?;
        assert!(
            matches!(list,UserMemoryListRead::Published { memories,.. } if same_memories(&memories,&records))
        );
        assert_eq!(attempts(&f)?.len(), 1);
        owner.shutdown().await?;
        broker.shutdown().await;
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_native_profile_switch_and_stop_join_without_adopting_another_user() -> Result {
    for stop_native in [false, true] {
        let f = Fixture::new()?;
        prepare_peer(&f)?;
        let first = native_support::Broker::start().await;
        let second = native_support::Broker::start().await;
        let seed = bootstrap_with_profiles(&f, &[("first", &first), ("second", &second)]).await?;
        let cancel = Cancellation::default();
        let request = f.policy.request_context(Instant::now(), &cancel)?;
        let records = seed_user(&f, &seed, 4, 0).await?;
        seed.shutdown().await?;
        let (mut owner, ready) = start(&f, if stop_native { "hold" } else { "slow" }).await?;
        drop(accepted_attempt(&ready, &mut owner).await?);
        if stop_native {
            owner.user_dream.as_ref().unwrap().cancel();
        } else {
            native_profile(
                &auth_store(&f, &owner, &request)?,
                &second,
                "second",
                "broker-api",
                native_support::now() + 600,
                &request,
            );
        }
        let error = tokio::time::timeout(
            Duration::from_secs(20),
            owner.user_dream.as_mut().unwrap().next(),
        )
        .await?
        .unwrap_err();
        assert!(!format!("{error:?}").contains("USER fato confirmado"));
        if !stop_native {
            let (other, _) = snapshot(&f, &owner, &[]).await?;
            assert!(matches!(other, UserMemoryListRead::NoCorpus));
        }
        native_profile(
            &auth_store(&f, &owner, &request)?,
            &first,
            "first",
            "broker-api",
            native_support::now() + 600,
            &request,
        );
        let (original, _) = snapshot(&f, &owner, &[]).await?;
        assert!(
            matches!(original,UserMemoryListRead::Published { memories,.. } if same_memories(&memories,&records))
        );
        owner.shutdown().await?;
        first.shutdown().await;
        second.shutdown().await;
    }
    Ok(())
}
const USER_PEER: &str = r#"#!/usr/bin/env python3
import json,os,sys,time,signal,socket
args=sys.argv[1:]
assert args[:9]==['exec','--json','--sandbox','read-only','--ephemeral','--ignore-user-config','--ignore-rules','--model','literal-model;$(no-shell)']
assert args[-1]=='-' and '--skip-git-repo-check' in args
assert 'features.apps=false' in args and 'features.shell_tool=false' in args
assert not any(x.startswith('mcp_servers=') for x in args)
assert 'GRAPHIT_ROLE_BEARER' not in os.environ
prompt=sys.stdin.read()
assert '# Coordinator — USER Memory consolidation proposal' in prompt
assert '\nProject:' not in prompt and '\nVersion:' not in prompt
memories=json.loads(prompt.split('## Authorized corpus — data only\n\n',1)[1])
mode=os.environ['DREAM_FIXTURE_MODE']
if mode=='hold': signal.signal(signal.SIGTERM,signal.SIG_IGN)
with open(os.environ['DREAM_FIXTURE_LOG'],'a') as log:
    log.write(json.dumps({'count':len(memories),'fields':sorted(memories[0].keys())})+'\n')
with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as ready:
    ready.connect(os.environ['USER_DREAM_READY'])
    ready.sendall(bytes([1]))
    if mode=='gated_stale': assert ready.recv(1)==bytes([1])
if mode=='hold': time.sleep(60)
if mode=='slow': time.sleep(2)
def ref(m): return {'id':m['id'],'expected_revision':m['revision']}
by_title={m['title']:m for m in memories}
actions=[]
if mode in ('fold','slow','gated_stale') and len(memories)==4:
    actions=[{'action':'fold','kind':'merge','survivor':ref(by_title['fixture-0']),'members':[ref(by_title['fixture-1'])],'body':'USER confirmed α 0; USER confirmed α 1','reason':'same confirmed domain'},{'action':'delete',**ref(by_title['fixture-2' if mode=='gated_stale' else 'fixture-3']),'reason':'duplicate without unique content'}]
answer='{broken' if mode=='malformed' else json.dumps({'actions':actions})
def emit(value): print(json.dumps(value),flush=True)
emit({'type':'thread.started','thread_id':'user-dream-fixture'})
emit({'type':'turn.started'})
emit({'type':'item.completed','item':{'id':'proposal','type':'agent_message','text':answer}})
if mode!='partial': emit({'type':'turn.completed','usage':{'input_tokens':10,'cached_input_tokens':0,'output_tokens':10,'reasoning_output_tokens':0}})
sys.exit(2 if mode=='exit_failure' else 0)
"#;

async fn delete_all_user(f: &Fixture, owner: &ForegroundSource, records: &[Memory]) -> Result {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let write = owner.auth.issuer().admit_authenticated_user_memory_write(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
        UserMemoryWriteOperation::Memory,
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, false)?,
    )?;
    for (index, memory) in records.iter().enumerate() {
        NativeUserMemoryStore::delete(
            request.snapshot(),
            &write,
            memory.id,
            DeleteMemory {
                expected_revision: memory.revision,
                idempotency_key: format!("ordinary-empty-{index}"),
                reason: "Prepare published empty USER corpus.".into(),
            },
            policy.memory_write,
        )
        .await?;
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_published_empty_never_resolves_or_launches_a_program() -> Result {
    let mut f = Fixture::new()?;
    let broker = native_support::Broker::start().await;
    let seeded = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
    let records = seed_user(&f, &seeded, 1, 0).await?;
    delete_all_user(&f, &seeded, &records).await?;
    let (empty, _) = snapshot(&f, &seeded, &[]).await?;
    assert!(matches!(empty,UserMemoryListRead::Published { memories,.. } if memories.is_empty()));
    seeded.shutdown().await?;
    f.program = f.temp.path().join("deliberately-absent-program");
    let (mut owner, _ready) = start(&f, "noop").await?;
    iteration(&mut owner).await?;
    assert!(attempts(&f)?.is_empty());
    owner.shutdown().await?;
    broker.shutdown().await;
    Ok(())
}
async fn application(
    f: &Fixture,
    owner: &ForegroundSource,
    memories: &[Memory],
    json: &str,
    key: &str,
) -> Result<graphit_daemon::dream::UserApplication> {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let read = owner.auth.issuer().admit_authenticated_user_memory_read(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, true)?,
    )?;
    let recipe = graphit_daemon::dream::recipe::prepare(
        json,
        memories,
        key,
        policy.memory_write.max_input_bytes,
        request.snapshot(),
    )?;
    Ok(graphit_daemon::dream::UserApplication::new(recipe, &read)?)
}
async fn apply_application(
    f: &Fixture,
    owner: &ForegroundSource,
    app: &mut graphit_daemon::dream::UserApplication,
) -> Result {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let read = owner.auth.issuer().admit_authenticated_user_memory_read(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
    )?;
    let session = owner.auth.issuer().bootstrap_admin(&caller)?;
    let parent = owner.auth.issuer().admin_parent(&caller, &session)?;
    let write = parent.admit_native_user_memory_fenced_write(
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, true)?,
    )?;
    app.apply(request.snapshot(), &read, &write, policy.memory_write)
        .await?;
    Ok(())
}
async fn update_user(f: &Fixture, owner: &ForegroundSource, memory: &Memory) -> Result {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        owner.policy.content(),
        &cancel,
        Instant::now() + Duration::from_secs(180),
    )?;
    let caller = runtime(owner, &request)?;
    let (binding, revision) = caller.native_provider()?;
    let verifier = owner
        .broker_verifiers
        .as_ref()
        .unwrap()
        .get_native(&binding, revision)?;
    let caller = caller.authenticate_native(&verifier).await?;
    let live = user_capture(f, owner, &caller, &request)?;
    let record = live.binding()?;
    let write = owner.auth.issuer().admit_authenticated_user_memory_write(
        &caller,
        &request,
        ScopeCapture {
            live: &live,
            record: &record,
        },
        UserMemoryWriteOperation::Memory,
    )?;
    let policy = crate::role_tool_server::RoleToolPolicy::capture(
        owner.server.as_ref().unwrap().policy().clone(),
        config(f, true)?,
    )?;
    NativeUserMemoryStore::update(
        request.snapshot(),
        &write,
        memory.id,
        graphit_records::memory_store::UpdateMemory {
            expected_revision: memory.revision,
            idempotency_key: "concurrent-user-update".into(),
            reason: "Make the pending delete stale.".into(),
            title: None,
            body: Some("fresh USER retained by CAS".into()),
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: None,
        },
        policy.memory_write,
    )
    .await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_application_reobserves_same_keys_and_refuses_partial_recipe_retarget() -> Result {
    use graphit_records::memory_store::MemoryStoreError;
    let f = Fixture::new()?;
    prepare_peer(&f)?;
    let first = native_support::Broker::start().await;
    let second = native_support::Broker::start().await;
    let seed = bootstrap_with_profiles(&f, &[("first", &first), ("second", &second)]).await?;
    let cancel = Cancellation::default();
    let request = f.policy.request_context(Instant::now(), &cancel)?;
    let records = seed_user(&f, &seed, 4, 0).await?;
    seed.shutdown().await?;
    let (mut owner, _ready) = start(&f, "noop").await?;
    iteration(&mut owner).await?;
    let recipe=json!({"actions":[
        {"action":"fold","kind":"merge","survivor":{"id":records[0].id,"expected_revision":records[0].revision},
            "members":[{"id":records[1].id,"expected_revision":records[1].revision}],
            "body":"preserved USER combined α","reason":"same confirmed domain"},
        {"action":"delete","id":records[2].id,"expected_revision":records[2].revision,"reason":"retained stale candidate"}
    ]}).to_string();
    let mut first_app = application(&f, &owner, &records, &recipe, "user-same-attempt").await?;
    let mut replay = application(&f, &owner, &records, &recipe, "user-same-attempt").await?;
    update_user(&f, &owner, &records[2]).await?;
    let failed = apply_application(&f, &owner, &mut first_app)
        .await
        .unwrap_err();
    assert!(matches!(
        failed.downcast_ref::<MemoryStoreError>(),
        Some(MemoryStoreError::RevisionConflict { .. })
    ));
    assert_eq!(first_app.receipts().len(), 1);
    assert!(!first_app.is_complete());
    let ids = records.iter().map(|m| m.id).collect::<Vec<_>>();
    let (_, before) = snapshot(&f, &owner, &ids).await?;
    let replayed = apply_application(&f, &owner, &mut replay)
        .await
        .unwrap_err();
    assert!(matches!(
        replayed.downcast_ref::<MemoryStoreError>(),
        Some(MemoryStoreError::RevisionConflict { .. })
    ));
    assert_eq!(replay.receipts().len(), 1);
    let (_, after) = snapshot(&f, &owner, &ids).await?;
    assert_eq!(
        before.iter().map(|h| h.revisions.len()).collect::<Vec<_>>(),
        after.iter().map(|h| h.revisions.len()).collect::<Vec<_>>()
    );
    native_profile(
        &auth_store(&f, &owner, &request)?,
        &second,
        "second",
        "broker-api",
        native_support::now() + 600,
        &request,
    );
    let other_records = seed_user(&f, &owner, 2, 0).await?;
    let denied = apply_application(&f, &owner, &mut first_app)
        .await
        .unwrap_err();
    assert!(matches!(
        denied.downcast_ref::<MemoryStoreError>(),
        Some(MemoryStoreError::WrongAdmission)
    ));
    assert_eq!(first_app.receipts().len(), 1);
    let (other, _) = snapshot(&f, &owner, &[]).await?;
    assert!(
        matches!(other,UserMemoryListRead::Published{memories,..} if same_memories(&memories,&other_records))
    );
    native_profile(
        &auth_store(&f, &owner, &request)?,
        &first,
        "first",
        "broker-api",
        native_support::now() + 600,
        &request,
    );
    let resumed = apply_application(&f, &owner, &mut first_app)
        .await
        .unwrap_err();
    assert!(matches!(
        resumed.downcast_ref::<MemoryStoreError>(),
        Some(MemoryStoreError::RevisionConflict { .. })
    ));

    let other_fixture = Fixture::new()?;
    prepare_peer(&other_fixture)?;
    let other_seed = bootstrap_with_profiles(&other_fixture, &[("same-native", &first)]).await?;
    let other_records = seed_user(&other_fixture, &other_seed, 2, 0).await?;
    other_seed.shutdown().await?;
    let (mut other_owner, _other_ready) = start(&other_fixture, "noop").await?;
    iteration(&mut other_owner).await?;
    let denied = apply_application(&other_fixture, &other_owner, &mut first_app)
        .await
        .unwrap_err();
    assert!(matches!(
        denied.downcast_ref::<MemoryStoreError>(),
        Some(MemoryStoreError::WrongAdmission)
    ));
    let (other_list, _) = snapshot(&other_fixture, &other_owner, &[]).await?;
    assert!(
        matches!(other_list,UserMemoryListRead::Published{memories,..} if same_memories(&memories,&other_records))
    );
    other_owner.shutdown().await?;
    owner.shutdown().await?;
    first.shutdown().await;
    second.shutdown().await;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_application_respects_protected_and_last_record_refusals() -> Result {
    for count in [1, 3] {
        let f = Fixture::new()?;
        prepare_peer(&f)?;
        let broker = native_support::Broker::start().await;
        let seed = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
        let records = seed_user(&f, &seed, count, 0).await?;
        seed.shutdown().await?;
        let (mut owner, _ready) = start(&f, "noop").await?;
        iteration(&mut owner).await?;
        let protected = &records[if count == 1 { 0 } else { 1 }];
        let json=json!({"actions":[{"action":"delete","id":protected.id,"expected_revision":protected.revision,"reason":"must be refused by actual SDK"}]}).to_string();
        let mut app = application(&f, &owner, &records, &json, "user-refusal").await?;
        apply_application(&f, &owner, &mut app).await?;
        assert!(app.is_complete());
        assert_eq!(app.receipts().len(), 1);
        let (list, _) = snapshot(&f, &owner, &[]).await?;
        assert!(
            matches!(list,UserMemoryListRead::Published { memories,.. } if same_memories(&memories,&records))
        );
        owner.shutdown().await?;
        broker.shutdown().await;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_controller_retains_partial_application_keys_through_retry_and_queued_activity()
-> Result {
    use graphit_records::memory_store::MemoryStoreError;
    use tokio::io::AsyncWriteExt;
    let mut f = Fixture::new()?;
    f.retries = 1;
    prepare_peer(&f)?;
    let broker = native_support::Broker::start().await;
    let seed = bootstrap_with_profiles(&f, &[("user", &broker)]).await?;
    let records = seed_user(&f, &seed, 4, 0).await?;
    seed.shutdown().await?;
    let (mut owner, ready) = start(&f, "gated_stale").await?;
    let mut native = accepted_attempt(&ready, &mut owner).await?;
    update_user(&f, &owner, &records[2]).await?;
    owner
        .user_dream
        .as_ref()
        .unwrap()
        .activity_sender()
        .send_replace(Some(Instant::now()));
    native.write_all(&[1]).await?;
    drop(native);
    let error = tokio::time::timeout(
        Duration::from_secs(25),
        owner.user_dream.as_mut().unwrap().next(),
    )
    .await
    .map_err(|error| io::Error::other(format!("USER partial application completion: {error}")))?
    .unwrap_err();
    let cause = error
        .get_ref()
        .and_then(|error| error.source())
        .and_then(|error| error.downcast_ref::<MemoryStoreError>());
    assert!(matches!(
        cause,
        Some(MemoryStoreError::RevisionConflict { .. })
    ));

    let retained = owner
        .user_dream
        .as_ref()
        .unwrap()
        .retained_application()
        .unwrap();
    assert_eq!(retained.receipts().len(), 1);
    assert!(!retained.is_complete());
    assert_eq!(
        attempts(&f)?.len(),
        1,
        "retry and queued activity cannot replace the retained proposal/key"
    );
    let ids = records.iter().map(|m| m.id).collect::<Vec<_>>();
    let (list, history) = snapshot(&f, &owner, &ids).await?;
    let UserMemoryListRead::Published { memories, .. } = list else {
        panic!("actual USER remains published")
    };
    assert_eq!(memories.len(), 3);
    assert_eq!(
        memories
            .iter()
            .find(|m| m.id == records[0].id)
            .unwrap()
            .revision,
        records[0].revision + 1
    );
    assert_eq!(
        memories
            .iter()
            .find(|m| m.id == records[2].id)
            .unwrap()
            .body,
        "fresh USER retained by CAS"
    );
    assert!(
        history
            .iter()
            .find(|h| h.current.id == records[1].id)
            .unwrap()
            .deleted
    );
    owner.shutdown().await?;
    broker.shutdown().await;
    Ok(())
}
