//! Qualification of read-only publication checks; setup is outside operation timings.
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactStore, CopyLimits, CopyPolicy,
        PublicationCheck,
    },
    brand::Brand,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use std::{
    error::Error,
    fs,
    hint::black_box,
    io,
    path::Path,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const COPY: ArtifactMode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message).into())
    }
}
fn store(root: &Path) -> Result<ArtifactStore> {
    let installation =
        InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("global")))?;
    let project = ProjectPaths::at(root, &installation)?;
    Ok(ArtifactStore::new(
        graphit_core::artifact_store::CapturedPublicationScope::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse()?,
            graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
            &project_birth_request()?,
        )
        .unwrap(),
        ArtifactLimits::new(
            1024 * 1024,
            1024,
            CopyLimits::new(64 * 1024 * 1024, 10_000, 1024 * 1024, 64)?,
        )?,
    ))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(unix)]
    if args.first().is_some_and(|s| s == "owned-access") {
        return owned_access();
    }
    #[cfg(unix)]
    if args.len() == 2 && args[0] == "owned-access-child" {
        return owned_child(Path::new(&args[1]));
    }
    ensure(args.len() == 2, "expected scenario and iterations")?;
    let iterations: usize = args[1].parse()?;
    ensure(
        (1..=1_000_000).contains(&iterations),
        "iterations outside limit",
    )?;
    let scenario = args[0].as_str();
    let (kind, mode, changed) = match scenario {
        "file" => (NodeKind::File, COPY, false),
        "tree" => (NodeKind::Directory, COPY, false),
        "link" => (NodeKind::File, ArtifactMode::Link, false),
        "changed-file" => (NodeKind::File, COPY, true),
        "changed-tree" => (NodeKind::Directory, COPY, true),
        _ => return Err(io::Error::other("unknown scenario").into()),
    };
    let temporary = tempfile::tempdir()?;
    let root = fs::canonicalize(temporary.path())?;
    let source = root.join("source");
    let destination = root.join("adapter/artifact");
    fs::create_dir(root.join("adapter"))?;
    let data = vec![42; 64 * 1024];
    if kind == NodeKind::File {
        fs::write(&source, &data)?;
    } else {
        fs::create_dir(&source)?;
        fs::create_dir(source.join("empty"))?;
        fs::create_dir(source.join("nested"))?;
        fs::create_dir(source.join("nested/empty"))?;
        fs::write(source.join("empty-file"), [])?;
        for i in 0..16 {
            fs::write(source.join(format!("file-{i}")), &data)?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if kind == NodeKind::File {
            fs::set_permissions(&source, fs::Permissions::from_mode(0o640))?;
        } else {
            fs::set_permissions(&source, fs::Permissions::from_mode(0o750))?;
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                fs::set_permissions(
                    entry.path(),
                    fs::Permissions::from_mode(if entry.file_type()?.is_dir() {
                        0o750
                    } else {
                        0o640
                    }),
                )?;
            }
            fs::set_permissions(
                source.join("nested/empty"),
                fs::Permissions::from_mode(0o750),
            )?;
        }
    }
    let store = store(&root)?;
    let mut writer = store.writer(Duration::from_secs(5), &Cancellation::default())?;
    let key = ArtifactKey::new("codex", "command", "qualification")?;
    writer.publish(
        &key,
        graphit_core::artifact_store::PhysicalSource::ExternalSource(&source),
        &destination,
        kind,
        mode,
    )?;
    if changed {
        let file = if kind == NodeKind::File {
            source.clone()
        } else {
            source.join("file-0")
        };
        let modified = fs::metadata(&file)?.modified()?;
        fs::write(&file, vec![43; 64 * 1024])?;
        fs::File::options()
            .write(true)
            .open(file)?
            .set_modified(modified)?;
    }
    let manifest = fs::read(store.manifest_path())?;
    let before = fs::symlink_metadata(&destination)?;
    let expected = if changed {
        PublicationCheck::NeedsPublication
    } else {
        PublicationCheck::VerifiedUnchanged
    };
    let operation = || -> Result<()> {
        ensure(
            black_box(writer.check_publication(
                black_box(&key),
                graphit_core::artifact_store::PhysicalSource::ExternalSource(&source),
                &destination,
                kind,
                mode,
            )?) == expected,
            "unexpected check result",
        )
    };
    for _ in 0..10 {
        operation()?;
    }
    let mut samples = Vec::with_capacity(iterations);
    let loop_start = Instant::now();
    for _ in 0..iterations {
        let start = Instant::now();
        operation()?;
        samples.push(start.elapsed().as_nanos());
    }
    let loop_ns = loop_start.elapsed().as_nanos();
    let timed_ns: u128 = samples.iter().sum();
    samples.sort_unstable();
    let after = fs::symlink_metadata(&destination)?;
    ensure(
        fs::read(store.manifest_path())? == manifest,
        "check wrote metadata",
    )?;
    ensure(
        before.permissions() == after.permissions() && before.modified()? == after.modified()?,
        "check altered destination",
    )?;
    ensure(
        fs::read_dir(root.join("adapter"))?.count() == 1,
        "check created wrappers",
    )?;
    println!(
        "{}",
        serde_json::json!({
            "scenario": scenario, "iterations": iterations, "warmup": 10,
            "timed_ms": timed_ns as f64 / 1e6, "loop_ms": loop_ns as f64 / 1e6,
            "p50_us": samples[(iterations-1)/2] as f64 / 1e3,
            "p95_us": samples[(iterations-1)*95/100] as f64 / 1e3,
            "source_bytes": if kind == NodeKind::File { 65536 } else { 1048576 },
            "source_nodes": if kind == NodeKind::File { 1 } else { 21 },

            "manifest_bytes": manifest.len(), "expected": format!("{expected:?}"),
            "scope": "existing writer; setup excluded from timings, included in process RSS/CPU; no Go baseline or product SLO"
        })
    );
    Ok(())
}

#[cfg(unix)]
fn owned_access() -> Result<()> {
    use std::{
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::Command,
    };
    let temporary = tempfile::tempdir()?;
    let root = fs::canonicalize(temporary.path())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755))?;
    fs::create_dir(root.join("project"))?;
    fs::write(root.join("source"), b"public restricted copy")?;
    fs::set_permissions(root.join("source"), fs::Permissions::from_mode(0o204))?;
    ensure(
        Command::new("chown")
            .arg("0:2000")
            .arg(root.join("source"))
            .status()?
            .success(),
        "source ownership setup failed",
    )?;
    ensure(
        Command::new("chown")
            .arg("1000:1000")
            .arg(root.join("project"))
            .status()?
            .success(),
        "project ownership setup failed",
    )?;
    let status = Command::new(std::env::current_exe()?)
        .arg("owned-access-child")
        .arg(&root)
        .uid(1000)
        .gid(1000)
        .status()?;
    ensure(status.success(), "public ownership fixture failed")
}
#[cfg(unix)]
fn owned_child(root: &Path) -> Result<()> {
    use graphit_core::artifact_store::ArtifactChange;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let source = root.join("source");
    let project = root.join("project");
    let destination = project.join("artifact");
    let metadata = fs::metadata(&source)?;
    ensure(
        metadata.uid() == 0
            && metadata.gid() == 2000
            && metadata.permissions().mode() & 0o777 == 0o204,
        "source fixture differs",
    )?;
    ensure(
        fs::read(&source)? == b"public restricted copy",
        "source is not readable via other-read",
    )?;
    let store = store(&project)?;
    let mut writer = store.writer(Duration::from_secs(5), &Cancellation::default())?;
    let key = ArtifactKey::new("codex", "command", "public-owner")?;
    ensure(
        writer.publish(
            &key,
            graphit_core::artifact_store::PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            COPY,
        )? == ArtifactChange::Created,
        "public creation differs",
    )?;
    let created = fs::metadata(&destination)?;
    ensure(
        created.uid() == 1000 && created.permissions().mode() & 0o777 == 0o204,
        "public destination owner/mode differs",
    )?;
    ensure(
        fs::read(&destination).unwrap_err().kind() == io::ErrorKind::PermissionDenied,
        "destination unexpectedly readable",
    )?;
    let manifest = fs::read(store.manifest_path())?;
    ensure(
        writer.check_publication(
            &key,
            graphit_core::artifact_store::PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            COPY,
        )? == PublicationCheck::NeedsPublication,
        "restricted probe differs",
    )?;
    ensure(
        fs::read(store.manifest_path())? == manifest,
        "restricted probe wrote metadata",
    )?;
    ensure(
        fs::metadata(&destination)?.permissions().mode() & 0o777 == 0o204,
        "probe changed mode",
    )?;
    ensure(
        writer.publish(
            &key,
            graphit_core::artifact_store::PhysicalSource::ExternalSource(&source),
            &destination,
            NodeKind::File,
            COPY,
        )? == ArtifactChange::Unchanged,
        "normal inspection differs",
    )?;
    ensure(
        fs::metadata(&destination)?.permissions().mode() & 0o777 == 0o204,
        "Inspect did not restore mode",
    )?;
    ensure(
        fs::read(&destination).unwrap_err().kind() == io::ErrorKind::PermissionDenied,
        "Inspect left read access",
    )?;
    ensure(
        writer.load()?.is_some_and(|s| s.pending().is_none()),
        "Inspect left pending state",
    )?;
    println!(
        "{}",
        serde_json::json!({"scenario":"public-owner", "created":true, "source_uid":0, "source_gid":2000,
        "destination_uid":1000, "mode":"0204", "probe":"NeedsPublication",
        "publish_after_inspect":"Unchanged",  "restored_mode":true,
        "scope":"real Unix ownership; public API only; no private receipt edits"})
    );
    writer.remove(&key)?;
    Ok(())
}

// Scope birth performs metadata work only; payload requests keep their own original context.
fn project_birth_request() -> std::io::Result<std::sync::Arc<graphit_core::file_io::SnapshotContext>>
{
    let original = std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        Cancellation::default(),
    ));
    Ok(original)
}
