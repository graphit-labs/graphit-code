use super::*;
use crate::tests::{capture, context, fixture, policy, save};
use graphit_core::file_lock::Cancellation;
use std::{
    fs,
    io::{self, Read},
    time::{Duration, Instant},
};

#[test]
fn borrowed_storage_names_open_the_actual_independently_named_files() {
    let dir = fixture();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("icebug.json")).unwrap()).unwrap();
    fs::create_dir(dir.path().join("physical")).unwrap();
    for node in manifest["node_tables"].as_array_mut().unwrap() {
        let old = node["shards"][0]["file"].as_str().unwrap();
        let new = format!("physical/{old}.data");
        fs::rename(dir.path().join(old), dir.path().join(&new)).unwrap();
        node["shards"][0]["file"] = new.into();
    }
    for group in manifest["relationships"].as_array_mut().unwrap() {
        for member in group["members"].as_array_mut().unwrap() {
            for key in ["indices", "indptr"] {
                let old = member[key].as_str().unwrap();
                let new = format!("physical/{old}.data");
                fs::rename(dir.path().join(old), dir.path().join(&new)).unwrap();
                member[key] = new.into();
            }
        }
    }
    save(dir.path(), &manifest);
    let graph = capture(dir.path(), context(), policy()).unwrap();
    let schema = graph.schema().unwrap();
    let nodes = schema.node_tables().collect::<Vec<_>>();
    let members = schema.relationships().collect::<Vec<_>>();
    let names = nodes
        .iter()
        .flat_map(|node| node.shards().map(|shard| shard.storage_file()))
        .chain(
            members
                .iter()
                .flat_map(|member| [member.indices_file(), member.indptr_file()]),
        )
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 6);
    for relative in names {
        assert!(relative.starts_with("physical/"));
        let expected = fs::read(dir.path().join(relative)).unwrap();
        let file = graph.open_file(Path::new(relative)).unwrap();
        assert_eq!(file.len(), expected.len() as u64);
        assert_eq!(file.read_range(0, 4).unwrap().as_ref(), b"PAR1");
        let mut stream = file.stream(4).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected[4..]);
    }
}

#[test]
fn invalid_or_uncaptured_paths_do_not_open_another_file() {
    let dir = fixture();
    let graph = capture(dir.path(), context(), policy()).unwrap();
    for relative in ["../File.parquet", "/File.parquet", "missing.parquet", ""] {
        assert!(graph.open_file(Path::new(relative)).is_err(), "{relative}");
    }
    fs::write(dir.path().join("added.parquet"), b"PAR1").unwrap();
    assert!(graph.open_file(Path::new("added.parquet")).is_err());
}

#[test]
fn owned_file_keeps_original_cancellation_and_deadline_after_graph_drop() {
    for expired in [false, true] {
        let dir = fixture();
        let cancel = Cancellation::default();
        let original = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(if expired { 1 } else { 60 }),
            cancel.clone(),
        ));
        let graph = capture(dir.path(), Arc::clone(&original), policy()).unwrap();
        let file = graph.open_file(Path::new("File.parquet")).unwrap();
        drop(graph);
        if expired {
            std::thread::sleep(
                original
                    .deadline()
                    .unwrap()
                    .saturating_duration_since(Instant::now())
                    + Duration::from_millis(1),
            );
        } else {
            cancel.cancel();
        }
        let expected = if expired {
            io::ErrorKind::TimedOut
        } else {
            io::ErrorKind::Interrupted
        };
        assert_eq!(file.read_range(0, 4).err().unwrap().kind(), expected);
        assert_eq!(file.stream(0).err().unwrap().kind(), expected);
    }
}

#[test]
fn changed_file_and_cancelled_open_preserve_causal_errors() {
    let dir = fixture();
    let cancel = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancel.clone(),
    ));
    let graph = capture(dir.path(), Arc::clone(&original), policy()).unwrap();
    let file = graph.open_file(Path::new("File.parquet")).unwrap();
    fs::write(dir.path().join("File.parquet"), b"changed").unwrap();
    assert!(file.read_range(0, 4).is_err());
    assert!(graph.open_file(Path::new("File.parquet")).is_err());
    cancel.cancel();
    assert!(matches!(graph.open_file(Path::new("Function.parquet")),
        Err(DataReadError::Source(ref error)) if error.kind() == io::ErrorKind::Interrupted));
}
