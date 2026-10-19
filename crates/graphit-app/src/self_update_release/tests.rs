use super::*;
use graphit_core::file_lock::Cancellation;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Good,
    Redirect,
    Truncated,
    BadHash,
    BadJson,
    Partial,
    MissingCoordinator,
    MissingGgufNotice,
    MissingRuntimeLibrary,
    InvalidRuntimePath,
    MissingPlatformManifest,
    WrongPlatformManifest,
    MissingPlatformLibrary,
    EscapingPlatformLibrary,
    WrongPlatformExecutable,
    InvalidMacMinimum,
    MissingNativeLicense,
    MissingLadybugLicense,
    InvalidNativeSources,
    MissingCargoSource,
    EmptyCargoSource,
    LocalGitSource,
    Duplicate,
    Escape,
    Cycle,
    Special,
    InvalidGzip,
    HttpError,
    DeclaredSize,
    EscapingPath,
}
fn limits() -> Limits {
    Limits {
        http: HttpLimits {
            request_body_bytes: 1024,
            response_body_bytes: 1024 * 1024,
            url_bytes: 4096,
            header_bytes: 8192,
            header_count: 32,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(1),
            max_certificates: 8,
            max_certificate_bytes: 8192,
        },
        tree: CopyLimits::new(1024 * 1024, 256, 65536, 32).unwrap(),
        metadata_bytes: 8192,
        decoded_archive_bytes: 2 * 1024 * 1024,
        redirects: 2,
    }
}
fn original(seconds: u64, cancel: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(seconds),
        cancel,
    ))
}
fn files(target: Target) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for binary in target.binaries() {
        files.push((binary.into(), b"controlled binary".to_vec()));
    }
    for name in [
        "graphit-runtime/LICENSE",
        "graphit-runtime/NATIVE-RUNTIME.md",
        "graphit-runtime/agents/coordinator/AGENT.md",
        "graphit-runtime/agents/coordinator/prompt.md",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp/LICENSE",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp/licenses/LICENSE-jsonhpp",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/LICENSE-MIT",
        "graphit-runtime/licenses/cargo/llama-cpp-sys-2/LICENSE-APACHE",
        "graphit-runtime/licenses/nvidia/LICENSE",
        "graphit-runtime/licenses/cargo/lance/LICENSE",
        "graphit-runtime/licenses/cargo/lbug/LICENSE",
        "graphit-runtime/licenses/cargo/lbug/lbug-src/third_party/dependency/LICENSE",
        "graphit-runtime/licenses/crates/graphit-ast/src/antlr_engine/cpp/vendor/antlr4/LICENSE.txt",
        "graphit-runtime/licenses/crates/graphit-ast/src/antlr_engine/cpp/generated/GRAMMAR_NOTICES.md",
        "graphit-runtime/licenses/crates/graphit-ast/grammars/tree_sitter/language/LICENSE",
    ] {
        files.push((name.into(), b"controlled notice".to_vec()));
    }
    files.push(("graphit-runtime/build.json".into(), serde_json::to_vec(&json!({
        "platform":target.name(),"cargo_lock_sha256":"ab".repeat(32),
        "native_sources":{"llama-cpp-sys-2":{"version":"0.1.161","source":"git+https://github.com/utilityai/llama-cpp-rs?rev=424d1ffe6c1c4212d5da8e6075f473e33e35fb7f#424d1ffe6c1c4212d5da8e6075f473e33e35fb7f"},"lance":{"version":"14.0.2","source":"registry+https://github.com/rust-lang/crates.io-index"},"lbug":{"version":"0.22.1","source":"registry+https://github.com/rust-lang/crates.io-index"}}
    })).unwrap()));
    if target == Target::LinuxAmd64 {
        let library = b"controlled CPU runtime library".to_vec();
        files.push(("graphit-runtime/lib/libstdc++.so.6".into(), library.clone()));
        let executables: Vec<_> = target
            .binaries()
            .into_iter()
            .map(|name| json!({"name":name}))
            .collect();
        files.push(("graphit-runtime/linux-libraries.json".into(), serde_json::to_vec(&json!({
            "platform":target.name(),"libraries":[{"soname":"libstdc++.so.6","sha256":bytes_hash(&library),"runpath":"$ORIGIN"}],
            "executables":executables
        })).unwrap()));
    } else {
        let macos = target == Target::DarwinArm64;
        let name = if macos {
            "libcrypto.3.dylib"
        } else {
            "vcruntime140.dll"
        };
        let file = if macos {
            format!("graphit-runtime/lib/{name}")
        } else {
            name.into()
        };
        files.push((file.clone(), b"controlled platform runtime".to_vec()));
        let executables: Vec<_> = target
            .binaries()
            .into_iter()
            .map(|file| {
                if macos {
                    json!({"file":file,"minimum_macos":"11.0"})
                } else {
                    json!({"file":file})
                }
            })
            .collect();
        let library = if macos {
            json!({"name":name,"file":file,"minimum_macos":"11.0"})
        } else {
            json!({"name":name,"file":file})
        };
        let mut manifest =
            json!({"platform":target.name(),"libraries":[library],"executables":executables});
        if macos {
            manifest["minimum_macos"] = "11.0".into();
        }
        let manifest_path = if macos {
            "graphit-runtime/macos-libraries.json"
        } else {
            "graphit-runtime/windows-libraries.json"
        };
        files.push((manifest_path.into(), serde_json::to_vec(&manifest).unwrap()));
    }

    files
}
fn bytes_hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut text, "{byte:02x}").expect("formatting a String");
    }
    text
}
fn archive(target: Target, mode: Mode, executable: Option<&Path>) -> Vec<u8> {
    if mode == Mode::InvalidGzip {
        return b"not a gzip or TAR archive".to_vec();
    }
    let compression = if executable.is_some() {
        flate2::Compression::fast()
    } else {
        flate2::Compression::default()
    };
    let gzip = flate2::write::GzEncoder::new(Vec::new(), compression);
    let mut archive = tar::Builder::new(gzip);
    // Release packages contain regular executable entries, including sparse ELF inputs.
    archive.sparse(false);
    let files = files(target);
    for (name, mut bytes) in files {
        if name == target.binaries()[0]
            && let Some(executable) = executable
        {
            archive
                .append_path_with_name(executable, format!("{}/{name}", target.root_name()))
                .unwrap();
            continue;
        }
        let platform_manifest = matches!(
            name.as_str(),
            "graphit-runtime/macos-libraries.json" | "graphit-runtime/windows-libraries.json"
        );
        if mode == Mode::MissingPlatformManifest && platform_manifest
            || mode == Mode::MissingPlatformLibrary
                && matches!(
                    name.as_str(),
                    "vcruntime140.dll" | "graphit-runtime/lib/libcrypto.3.dylib"
                )
        {
            continue;
        }
        if platform_manifest {
            let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            match mode {
                Mode::WrongPlatformManifest => manifest["platform"] = "unsupported-target".into(),
                Mode::EscapingPlatformLibrary => {
                    manifest["libraries"][0]["file"] = "../outside.dll".into()
                }
                Mode::WrongPlatformExecutable => {
                    manifest["executables"][0]["file"] = "other.exe".into()
                }
                Mode::InvalidMacMinimum => manifest["minimum_macos"] = "invalid".into(),
                _ => {}
            }
            bytes = serde_json::to_vec(&manifest).unwrap();
        }
        if mode == Mode::MissingCoordinator && name.ends_with("/coordinator/prompt.md") {
            continue;
        }
        if mode == Mode::MissingGgufNotice && name.ends_with("/llama.cpp/LICENSE")
            || mode == Mode::MissingRuntimeLibrary && name == "graphit-runtime/lib/libstdc++.so.6"
        {
            continue;
        }
        if mode == Mode::InvalidRuntimePath && name == "graphit-runtime/linux-libraries.json" {
            let mut metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            metadata["libraries"][0]["soname"] = "../../outside".into();
            bytes = serde_json::to_vec(&metadata).unwrap();
        }
        if mode == Mode::MissingNativeLicense
            && name == "graphit-runtime/licenses/cargo/lance/LICENSE"
            || mode == Mode::MissingLadybugLicense
                && name == "graphit-runtime/licenses/cargo/lbug/LICENSE"
        {
            continue;
        }
        if mode == Mode::InvalidNativeSources && name == "graphit-runtime/build.json" {
            let mut build: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            build["native_sources"]["lance"]["version"] = "14.0.1".into();
            bytes = serde_json::to_vec(&build).unwrap();
        }
        if matches!(
            mode,
            Mode::MissingCargoSource | Mode::EmptyCargoSource | Mode::LocalGitSource
        ) && name == "graphit-runtime/build.json"
        {
            let mut build: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let source = &mut build["native_sources"]["lance"];
            match mode {
                Mode::MissingCargoSource => {
                    source.as_object_mut().unwrap().remove("source");
                }
                Mode::EmptyCargoSource => source["source"] = "".into(),
                Mode::LocalGitSource => {
                    source["source"] = "git+file:///private/checkout#0123456789abcdef".into()
                }
                _ => unreachable!(),
            }
            bytes = serde_json::to_vec(&build).unwrap();
        }
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        let path = format!("{}/{name}", target.root_name());
        archive
            .append_data(&mut header, &path, bytes.as_slice())
            .unwrap();
        if mode == Mode::Duplicate && name == target.binaries()[0] {
            archive
                .append_data(&mut header, &path, bytes.as_slice())
                .unwrap();
        }
    }
    // Safe internal links remain a supported archive feature; escaping links do not.
    if target != Target::WindowsAmd64 && mode != Mode::MissingRuntimeLibrary {
        let (prefix, name, link) = match target {
            Target::LinuxAmd64 => ("graphit-runtime/lib", "libstdc++.so", "libstdc++.so.6"),
            _ => (
                "graphit-runtime/licenses",
                "llama-license",
                "cargo/llama-cpp-sys-2/llama.cpp/LICENSE",
            ),
        };
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        let link = if mode == Mode::Escape {
            "../../../../outside"
        } else if mode == Mode::Cycle {
            name
        } else {
            link
        };
        archive
            .append_link(
                &mut header,
                format!("{}/{prefix}/{name}", target.root_name()),
                link,
            )
            .unwrap();
    }
    if mode == Mode::Special {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Fifo);
        header.set_size(0);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                format!("{}/fifo", target.root_name()),
                io::empty(),
            )
            .unwrap();
    }
    if mode == Mode::EscapingPath {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(1);
        header.set_mode(0o600);
        // Adversarial fixture: append a real TAR header without Builder path validation.
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..10].copy_from_slice(b"../outside");
        header.set_cksum();
        archive.append(&header, &b"x"[..]).unwrap();
    }
    let gzip = archive.into_inner().unwrap();
    gzip.finish().unwrap()
}
struct Fixture {
    source: Source,
    started: Cancellation,
    stop: Cancellation,
    requests: Arc<std::sync::Mutex<Vec<String>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Fixture {
    async fn new(target: Target, mode: Mode) -> Self {
        Self::with_executable(target, mode, None).await
    }
    async fn with_executable(target: Target, mode: Mode, executable: Option<&Path>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let source = Source {
            metadata: Url::parse(&format!("{base}/metadata")).unwrap(),
            asset_origin: Url::parse(&base).unwrap(),
            allow_loopback_http: true,
        };
        let stop = Cancellation::default();
        let serving_stop = stop.clone();
        let started = Cancellation::default();
        let body_started = started.clone();
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let archive = archive(target, mode, executable);
        let name = target.archive_name();
        let hash = if mode == Mode::BadHash {
            "00".repeat(32)
        } else {
            use std::fmt::Write as _;
            let mut hex = String::with_capacity(64);
            for byte in Sha256::digest(&archive).iter() {
                write!(&mut hex, "{byte:02x}").expect("formatting a String");
            }
            hex
        };
        let checksum = format!("{hash}  {name}\n").into_bytes();
        let prefix = "/graphit-labs/graphit-code/releases/download/v1.2.3/".to_owned();
        let metadata = if mode == Mode::BadJson {
            b"invalid JSON".to_vec()
        } else {
            serde_json::to_vec(&json!({
            "tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[
                {"name":name,"browser_download_url":format!("{base}{prefix}{name}"),"size":archive.len()},
                {"name":format!("{name}.sha256"),"browser_download_url":format!("{base}{prefix}{name}.sha256"),"size":checksum.len()}
            ]
        })).unwrap()
        };
        let task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! { _=serving_stop.cancelled()=>break, result=listener.accept()=>result };
                let (mut socket, _) = accepted.unwrap();
                let mut request = Vec::new();
                let mut bytes = [0; 1024];
                loop {
                    let count = tokio::select! { _=serving_stop.cancelled()=>return, result=socket.read(&mut bytes)=>result.unwrap_or(0) };
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&bytes[..count]);
                    if request.windows(4).any(|b| b == b"\r\n\r\n") {
                        break;
                    }
                    assert!(request.len() <= 8192);
                }
                if request.is_empty() {
                    continue;
                }
                let request = std::str::from_utf8(&request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .split_ascii_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                observed.lock().unwrap().push(request.clone());
                if mode == Mode::Redirect && request.starts_with(&prefix) {
                    let response = format!(
                        "HTTP/1.1 302 Found\r\nLocation: /assets/{}\r\nContent-Length: 15\r\nConnection: close\r\n\r\nredirect-canary",
                        request.strip_prefix(&prefix).unwrap()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    continue;
                }
                if mode == Mode::HttpError && request == "/metadata" {
                    let _ = socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture").await;
                    continue;
                }
                let is_archive = request.ends_with(".tar.gz");
                let body = if request == "/metadata" {
                    &metadata
                } else if request.ends_with(".sha256") {
                    &checksum
                } else {
                    &archive
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    if mode == Mode::DeclaredSize && is_archive {
                        body.len() + 1
                    } else {
                        body.len()
                    }
                );
                if socket.write_all(header.as_bytes()).await.is_err() {
                    continue;
                }
                if is_archive && matches!(mode, Mode::Partial | Mode::Truncated) {
                    let _ = socket.write_all(&body[..body.len() / 2]).await;
                    body_started.cancel();
                    if mode == Mode::Partial {
                        serving_stop.cancelled().await;
                        return;
                    }
                } else {
                    let _ = socket.write_all(body).await;
                }
            }
        });
        Self {
            source,
            started,
            stop,
            requests,
            task: Some(task),
        }
    }
    fn source(&self) -> Source {
        Source {
            metadata: self.source.metadata.clone(),
            asset_origin: self.source.asset_origin.clone(),
            allow_loopback_http: true,
        }
    }
    async fn finish(mut self) {
        self.stop.cancel();
        self.task.take().unwrap().await.unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
fn empty(parent: &Path) -> bool {
    fs::read_dir(parent).unwrap().next().is_none()
}

#[tokio::test]
async fn actual_tcp_http_sha_tar_and_private_stage_support_all_package_targets() {
    for target in [
        Target::LinuxAmd64,
        Target::DarwinArm64,
        Target::WindowsAmd64,
    ] {
        let stage = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        fs::write(installed.path().join("sentinel"), b"installed unchanged").unwrap();
        let fixture = Fixture::new(target, Mode::Good).await;
        let result = prepare_from(
            original(5, Cancellation::default()),
            "v1.0.0",
            target,
            stage.path(),
            limits(),
            fixture.source(),
        )
        .await
        .unwrap();
        let Preparation::Ready(ready) = result else {
            panic!("prepared package required");
        };
        assert_eq!(ready.tag(), "v1.2.3");
        assert_eq!(ready.target(), target);
        if target == Target::WindowsAmd64 {
            assert_eq!(ready.root_assets(), &["vcruntime140.dll"]);
            assert!(ready.directory().join("vcruntime140.dll").is_file());
        }
        assert!(
            ready
                .directory()
                .join("graphit-runtime/agents/coordinator/prompt.md")
                .is_file()
        );
        assert_eq!(
            *ready.digest(),
            <[u8; 32]>::from(Sha256::digest(archive(target, Mode::Good, None)))
        );
        graphit_fs::verify_private_directory(&ready._private_directory).unwrap();
        let container = ready._container.path().to_owned();
        assert!(!format!("{ready:?}").contains(&container.display().to_string()));
        let requests = fixture.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].ends_with(".tar.gz.sha256"));
        drop(ready);
        assert!(!container.exists());
        assert!(empty(stage.path()));
        assert_eq!(
            fs::read(installed.path().join("sentinel")).unwrap(),
            b"installed unchanged"
        );
        fixture.finish().await;
    }
}
#[tokio::test]
async fn redirects_do_not_append_body_and_exhausted_redirect_budget_fails() {
    let stage = tempfile::tempdir().unwrap();
    let target = Target::LinuxAmd64;
    let fixture = Fixture::new(target, Mode::Redirect).await;
    let result = prepare_from(
        original(5, Cancellation::default()),
        "1.0.0",
        target,
        stage.path(),
        limits(),
        fixture.source(),
    )
    .await
    .unwrap();
    assert!(matches!(result, Preparation::Ready(_)));
    drop(result);
    assert!(empty(stage.path()));
    assert_eq!(fixture.requests.lock().unwrap().len(), 5);
    fixture.finish().await;
    let fixture = Fixture::new(target, Mode::Redirect).await;
    let mut capped = limits();
    capped.redirects = 0;
    assert!(matches!(
        prepare_from(
            original(5, Cancellation::default()),
            "1.0.0",
            target,
            stage.path(),
            capped,
            fixture.source()
        )
        .await,
        Err(PrepareError::Redirect)
    ));
    assert!(empty(stage.path()));
    fixture.finish().await;
}
#[tokio::test]
async fn invalid_http_json_hash_truncation_and_archive_never_return_a_prepared_owner() {
    for mode in [
        Mode::BadJson,
        Mode::BadHash,
        Mode::Truncated,
        Mode::InvalidGzip,
        Mode::MissingCoordinator,
        Mode::MissingGgufNotice,
        Mode::MissingRuntimeLibrary,
        Mode::InvalidRuntimePath,
        Mode::MissingNativeLicense,
        Mode::MissingLadybugLicense,
        Mode::InvalidNativeSources,
        Mode::MissingCargoSource,
        Mode::EmptyCargoSource,
        Mode::LocalGitSource,
        Mode::Duplicate,
        Mode::Escape,
        Mode::Cycle,
        Mode::Special,
        Mode::HttpError,
        Mode::DeclaredSize,
        Mode::EscapingPath,
    ] {
        let stage = tempfile::tempdir().unwrap();
        let target = Target::LinuxAmd64;
        let fixture = Fixture::new(target, mode).await;
        let result = prepare_from(
            original(5, Cancellation::default()),
            "1.0.0",
            target,
            stage.path(),
            limits(),
            fixture.source(),
        )
        .await;
        match mode {
            Mode::HttpError => assert!(matches!(result, Err(PrepareError::HttpStatus(503)))),
            Mode::DeclaredSize => assert!(matches!(result, Err(PrepareError::Size))),
            Mode::EscapingPath => assert!(matches!(result, Err(PrepareError::Archive))),
            Mode::MissingGgufNotice
            | Mode::MissingRuntimeLibrary
            | Mode::InvalidRuntimePath
            | Mode::MissingNativeLicense
            | Mode::MissingLadybugLicense
            | Mode::InvalidNativeSources
            | Mode::MissingCargoSource
            | Mode::EmptyCargoSource
            | Mode::LocalGitSource => {
                assert!(matches!(result, Err(PrepareError::Layout)))
            }
            _ => assert!(result.is_err()),
        }
        assert!(empty(stage.path()));
        fixture.finish().await;
    }
}
#[tokio::test]
async fn effective_file_node_path_depth_metadata_and_decoded_tar_limits_are_enforced() {
    for case in ["bytes", "nodes", "paths", "depth", "metadata", "decoded"] {
        let stage = tempfile::tempdir().unwrap();
        let target = Target::LinuxAmd64;
        let fixture = Fixture::new(target, Mode::Good).await;
        let mut capped = limits();
        capped.tree = match case {
            "bytes" => CopyLimits::new(1, 256, 65536, 32).unwrap(),
            "nodes" => CopyLimits::new(1024 * 1024, 1, 65536, 32).unwrap(),
            "paths" => CopyLimits::new(1024 * 1024, 256, 1, 32).unwrap(),
            "depth" => CopyLimits::new(1024 * 1024, 256, 65536, 1).unwrap(),
            _ => capped.tree,
        };
        if case == "metadata" {
            capped.metadata_bytes = 32;
        }
        if case == "decoded" {
            capped.decoded_archive_bytes = 100;
        }
        assert!(
            prepare_from(
                original(5, Cancellation::default()),
                "1.0.0",
                target,
                stage.path(),
                capped,
                fixture.source()
            )
            .await
            .is_err(),
            "{case}"
        );
        assert!(empty(stage.path()));
        fixture.finish().await;
    }
}
#[tokio::test]
async fn cancel_deadline_and_future_drop_retire_only_the_partial_private_stage() {
    for mode in ["cancel", "deadline", "drop"] {
        let stage = tempfile::tempdir().unwrap();
        let target = Target::LinuxAmd64;
        let fixture = Fixture::new(target, Mode::Partial).await;
        let cancellation = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now()
                + if mode == "deadline" {
                    // The deadline includes metadata/checksum requests and staging.
                    // Allow those to reach the explicit partial-body barrier even
                    // in unoptimized builds; the held body must still time out.
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(5)
                },
            cancellation.clone(),
        ));
        let mut operation = Box::pin(prepare_from(
            original,
            "1.0.0",
            target,
            stage.path(),
            limits(),
            fixture.source(),
        ));
        tokio::select! { _=fixture.started.cancelled()=>{}, result=&mut operation=>panic!("body must be partial: {result:?}") }
        assert!(
            !empty(stage.path()),
            "own stage must exist at the actual streaming boundary"
        );
        match mode {
            "cancel" => {
                cancellation.cancel();
                assert!(matches!(
                    operation.await,
                    Err(PrepareError::Network(NetworkError::Cancelled))
                        | Err(PrepareError::Io(io::ErrorKind::Interrupted))
                ));
            }
            "deadline" => {
                assert!(matches!(
                    operation.await,
                    Err(PrepareError::Network(NetworkError::Deadline))
                        | Err(PrepareError::Io(io::ErrorKind::TimedOut))
                ));
            }
            "drop" => drop(operation),
            _ => unreachable!(),
        }
        assert!(empty(stage.path()));
        fixture.finish().await;
    }
}
#[tokio::test]
async fn actual_development_version_does_not_contact_http_or_create_staging() {
    let stage = tempfile::tempdir().unwrap();
    let fixture = Fixture::new(Target::LinuxAmd64, Mode::Good).await;
    assert!(matches!(
        prepare_from(
            original(5, Cancellation::default()),
            "dev",
            Target::LinuxAmd64,
            stage.path(),
            limits(),
            fixture.source()
        )
        .await
        .unwrap(),
        Preparation::Development
    ));
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert!(empty(stage.path()));
    fixture.finish().await;
}

#[test]
fn abandoned_extraction_worker_retains_real_stage_until_its_owned_work_retires() {
    // Saturate the real blocking pool, rather than replacing the extractor or
    // injecting a callback. The production preparation future queues its work.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for cancel_worker in [false, true] {
            let stage = tempfile::tempdir().unwrap();
            let target = Target::LinuxAmd64;
            let fixture = Fixture::new(target, Mode::Good).await;
            let (started, ready) = tokio::sync::oneshot::channel();
            let (release, held) = std::sync::mpsc::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                let _ = held.recv();
            });
            ready.await.unwrap();
            let cancel = Cancellation::default();
            let mut preparation = Box::pin(prepare_from(
                original(5, cancel.clone()),
                "1.0.0",
                target,
                stage.path(),
                limits(),
                fixture.source(),
            ));
            assert!(
                tokio::time::timeout(Duration::from_secs(1), preparation.as_mut())
                    .await
                    .is_err()
            );
            let entries = fs::read_dir(stage.path())
                .unwrap()
                .collect::<io::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(entries.len(), 1);
            let container = entries[0].path();
            assert_eq!(
                fs::metadata(container.join("owned/release.tar.gz"))
                    .unwrap()
                    .len(),
                archive(target, Mode::Good, None).len() as u64
            );
            assert!(
                !container.join("owned/tree").exists(),
                "the real extraction is still queued"
            );
            drop(preparation);
            // If ownership were still in the async frame, Drop would remove it
            // here. Persistence while the pool is held proves the actual worker owns it.
            assert!(
                container.exists(),
                "abandonment must retain the queued worker's own stage"
            );
            if cancel_worker {
                cancel.cancel();
            }
            release.send(()).unwrap();
            blocker.await.unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while container.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(
                empty(stage.path()),
                "the retired worker must remove only its owned stage"
            );
            fixture.finish().await;
        }
    });
}

pub(crate) async fn prepared_release(
    parent: &Path,
    snapshot: Arc<SnapshotContext>,
    executable: Option<&Path>,
) -> PreparedRelease {
    prepared_release_mode(parent, snapshot, executable, Mode::Good).await
}

async fn prepared_release_mode(
    parent: &Path,
    snapshot: Arc<SnapshotContext>,
    executable: Option<&Path>,
    mode: Mode,
) -> PreparedRelease {
    let target = crate::self_update::native_target().unwrap();
    let fixture = Fixture::with_executable(target, mode, executable).await;
    let mut preparation_limits = limits();
    if let Some(executable) = executable {
        let bytes = fs::metadata(executable)
            .unwrap()
            .len()
            .checked_add(1024 * 1024)
            .unwrap();
        preparation_limits.http.response_body_bytes = usize::try_from(bytes).unwrap();
        preparation_limits.tree = CopyLimits::new(bytes, 256, 65536, 32).unwrap();
        preparation_limits.decoded_archive_bytes = bytes;
    }
    let result = prepare_from(
        snapshot,
        "v1.0.0",
        target,
        parent,
        preparation_limits,
        fixture.source(),
    )
    .await
    .unwrap();
    fixture.finish().await;
    let Preparation::Ready(ready) = result else {
        panic!("real release preparation did not return Ready");
    };
    ready
}

#[tokio::test]
async fn platform_runtime_manifests_are_required_and_confined_before_installation() {
    for target in [Target::WindowsAmd64, Target::DarwinArm64] {
        for mode in [
            Mode::MissingPlatformManifest,
            Mode::WrongPlatformManifest,
            Mode::MissingPlatformLibrary,
            Mode::EscapingPlatformLibrary,
            Mode::WrongPlatformExecutable,
        ] {
            let stage = tempfile::tempdir().unwrap();
            let fixture = Fixture::new(target, mode).await;
            let result = prepare_from(
                original(5, Cancellation::default()),
                "v1.0.0",
                target,
                stage.path(),
                limits(),
                fixture.source(),
            )
            .await;
            assert!(matches!(result, Err(PrepareError::Layout)));
            assert!(empty(stage.path()));
            fixture.finish().await;
        }
    }
    let stage = tempfile::tempdir().unwrap();
    let fixture = Fixture::new(Target::DarwinArm64, Mode::InvalidMacMinimum).await;
    assert!(matches!(
        prepare_from(
            original(5, Cancellation::default()),
            "v1.0.0",
            Target::DarwinArm64,
            stage.path(),
            limits(),
            fixture.source()
        )
        .await,
        Err(PrepareError::Layout)
    ));
    fixture.finish().await;
}
#[test]
fn macos_minimum_uses_numeric_components_and_rejects_unsupported_hosts() {
    validate_macos_host("14.2.1", "14.2.1").unwrap();
    validate_macos_host("14.2", "15.0").unwrap();
    assert!(
        matches!(validate_macos_host("14.2.1", "14.2"), Err(PrepareError::UnsupportedMacOS { required, current }) if required == "14.2.1" && current == "14.2")
    );
    for invalid in ["14", "14.-1", "14.2.3.4", "14.2 suffix", "0.0"] {
        assert!(matches!(macos_version(invalid), Err(PrepareError::Layout)));
    }
}
