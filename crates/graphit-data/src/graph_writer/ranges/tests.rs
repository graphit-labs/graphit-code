use super::*;
use crate::tests::context;
use arrow_array_parquet::{Array, UInt64Array};
use graphit_core::file_lock::Cancellation;
use parquet::file::{reader::FileReader, serialized_reader::SerializedFileReader};
use std::{
    io::{Seek, SeekFrom},
    time::{Duration, Instant},
};

fn node(key: &str, property: (&str, GraphValue)) -> GraphNode {
    GraphNode {
        label: "Function".into(),
        key: key.into(),
        properties: BTreeMap::from([(property.0.to_owned(), property.1)]),
    }
}
fn nodes(directory: &Path, rows: &[GraphNode], original: &Arc<SnapshotContext>) -> PathBuf {
    std::fs::create_dir(directory).unwrap();
    let shards = write_file_nodes(directory, rows, original).unwrap();
    directory.join(&shards[0].file)
}
fn uints(path: &Path, name: &str, range: impl RangeBounds<u64>) -> Vec<u64> {
    let mut output = Vec::new();
    for batch in read_rows(path, range, &context()).unwrap() {
        let batch = batch.unwrap();
        let column = batch
            .column_by_name(name)
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        assert_eq!(column.null_count(), 0);
        output.extend(column.values().iter().copied());
    }
    output
}

#[test]
fn concatenated_node_groups_preserve_encoded_chunks_nullable_union_and_slices() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let first = nodes(
        &root.path().join("a"),
        &[
            node("a1", ("name", GraphValue::String("ação".into()))),
            node("a2", ("name", GraphValue::String("second".into()))),
        ],
        &original,
    );
    let second = nodes(
        &root.path().join("b"),
        &[node("b1", ("line", GraphValue::Int64(9)))],
        &original,
    );
    let output = root.path().join("joined.parquet");
    let positions = concat_parquet_rows(
        &output,
        [
            Ok((first.clone(), 0, 2)),
            Ok((second, 0, 1)),
            Ok((first.clone(), 0, 2)),
        ],
        &original,
    )
    .unwrap();
    assert_eq!(positions, [(0, 2), (2, 1), (3, 2)]);
    let reader = SerializedFileReader::new(File::open(&output).unwrap()).unwrap();
    assert_eq!(reader.num_row_groups(), 3);
    assert_eq!(reader.metadata().file_metadata().num_rows(), 5);
    let mut selected = Vec::new();
    visit_node_shard(&output, "Function", 2..3, &original, |node| {
        selected.push(node);
        Ok(())
    })
    .unwrap();
    assert_eq!(selected, [node("b1", ("line", GraphValue::Int64(9)))]);
    let mut selected = Vec::new();
    visit_node_shard(&output, "Function", 3.., &original, |node| {
        selected.push(node);
        Ok(())
    })
    .unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].key, "a1");
    assert!(!selected[0].properties.contains_key("line"));
    // The official append API preserves encoded column bytes, even when union
    // columns alter physical positions in the destination file.
    let source = SerializedFileReader::new(File::open(&first).unwrap()).unwrap();
    let input = File::open(&first).unwrap();
    let output = File::open(&output).unwrap();
    for column in source.metadata().row_group(0).columns() {
        let copied = reader
            .metadata()
            .row_group(0)
            .columns()
            .iter()
            .find(|candidate| candidate.column_descr().name() == column.column_descr().name())
            .unwrap();
        assert_eq!(column.compressed_size(), copied.compressed_size());
        assert_eq!(
            input
                .get_bytes(
                    column.data_page_offset() as u64,
                    column.compressed_size() as usize
                )
                .unwrap(),
            output
                .get_bytes(
                    copied.data_page_offset() as u64,
                    copied.compressed_size() as usize
                )
                .unwrap()
        );
    }
}

#[test]
fn fact_ranges_support_whole_inclusive_exclusive_and_empty_without_other_rows() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let first = root.path().join("first.parquet");
    let second = root.path().join("second.parquet");
    write_fact_rows(&first, [Ok("first".into())], &original).unwrap();
    write_fact_rows(
        &second,
        [Ok("second".into()), Ok("third".into())],
        &original,
    )
    .unwrap();
    let output = root.path().join("facts.parquet");
    assert_eq!(
        concat_parquet_rows(&output, [Ok((first, 0, 1)), Ok((second, 0, 2))], &original).unwrap(),
        [(0, 1), (1, 2)]
    );
    let mut values = Vec::new();
    visit_fact_rows(&output, 1..=1, &original, |row| {
        values.push(row.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(values, ["second"]);
    let mut count = 0;
    visit_fact_rows(&output, .., &original, |_| {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 3);
    visit_fact_rows(&output, 3..3, &original, |_| {
        panic!("empty slice leaked a row")
    })
    .unwrap();
    let mut values = Vec::new();
    visit_fact_rows(
        &output,
        (Bound::Excluded(0), Bound::Excluded(2)),
        &original,
        |row| {
            values.push(row.to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(values, ["second"]);
    assert!(
        visit_fact_rows(
            &output,
            (Bound::Included(2), Bound::Excluded(1)),
            &original,
            |_| Ok(())
        )
        .is_err()
    );
    assert!(visit_fact_rows(&output, 0..4, &original, |_| Ok(())).is_err());
    assert!(visit_fact_rows(&output, ..=u64::MAX, &original, |_| Ok(())).is_err());
    assert!(
        visit_fact_rows(
            &output,
            (Bound::Excluded(u64::MAX), Bound::Unbounded),
            &original,
            |_| Ok(())
        )
        .is_err()
    );
}

#[test]
fn selected_node_group_does_not_decode_corrupt_unselected_group() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let rows = (0..1025)
        .map(|row| node(&format!("key-{row}"), ("line", GraphValue::Int64(row))))
        .collect::<Vec<_>>();
    let input = nodes(&root.path().join("nodes"), &rows, &original);
    let reader = SerializedFileReader::new(File::open(&input).unwrap()).unwrap();
    assert_eq!(reader.num_row_groups(), 2);
    let offset = reader.metadata().row_group(0).column(0).data_page_offset() as u64;
    drop(reader);
    let mut file = OpenOptions::new().write(true).open(&input).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&[0xff; 8]).unwrap();
    let mut selected = Vec::new();
    visit_node_shard(&input, "Function", 1024..1025, &original, |row| {
        selected.push(row);
        Ok(())
    })
    .unwrap();
    assert_eq!(selected, [rows[1024].clone()]);
    assert_eq!(parquet_rows(&input, &original).unwrap(), 1025);
    assert!(visit_node_shard(&input, "Function", .., &original, |_| Ok(())).is_err());
}

#[test]
fn concatenation_rejects_partial_groups_schema_conflicts_and_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let first = nodes(
        &root.path().join("a"),
        &[
            node("a", ("value", GraphValue::Int64(1))),
            node("b", ("value", GraphValue::Int64(2))),
        ],
        &original,
    );
    let second = nodes(
        &root.path().join("b"),
        &[node("c", ("value", GraphValue::String("wrong".into())))],
        &original,
    );
    let output = root.path().join("output.parquet");
    assert!(concat_parquet_rows(&output, [Ok((first.clone(), 1, 1))], &original).is_err());
    assert!(!output.exists());
    assert!(
        concat_parquet_rows(
            &output,
            [Ok((first.clone(), 0, 2)), Ok((second, 0, 1))],
            &original
        )
        .is_err()
    );
    assert!(!output.exists());
    assert!(concat_parquet_rows(&output, [Ok((first.clone(), u64::MAX, 1))], &original).is_err());
    std::fs::write(&output, b"untouched").unwrap();
    assert!(concat_parquet_rows(&output, [Ok((first, 0, 2))], &original).is_err());
    assert_eq!(std::fs::read(output).unwrap(), b"untouched");
}

#[test]
fn csr_iterators_sort_and_concatenation_preserves_member_local_pointers() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let first = root.path().join("a");
    let second = root.path().join("b");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    assert_eq!(
        write_csr_shard(
            &first,
            2,
            3,
            [
                Ok((1, 2, "c".into())),
                Ok((0, 1, "b".into())),
                Ok((0, 0, "a".into()))
            ],
            &original
        )
        .unwrap(),
        3
    );
    assert_eq!(
        write_csr_shard(&second, 1, 2, [Ok((0, 1, "d".into()))], &original).unwrap(),
        1
    );
    let pointers = root.path().join("indptr");
    let indices = root.path().join("indices");
    assert_eq!(
        concat_parquet_rows(
            &pointers,
            [
                Ok((first.join("edges.indptr"), 0, 3)),
                Ok((second.join("edges.indptr"), 0, 2))
            ],
            &original
        )
        .unwrap(),
        [(0, 3), (3, 2)]
    );
    assert_eq!(
        concat_parquet_rows(
            &indices,
            [
                Ok((first.join("edges.indices"), 0, 3)),
                Ok((second.join("edges.indices"), 0, 1))
            ],
            &original
        )
        .unwrap(),
        [(0, 3), (3, 1)]
    );
    assert_eq!(uints(&pointers, "ptr", ..), [0, 2, 3, 0, 1]);
    assert_eq!(uints(&pointers, "ptr", 3..5), [0, 1]);
    assert_eq!(uints(&indices, "target", ..), [0, 1, 2, 1]);
    assert_eq!(
        SerializedFileReader::new(File::open(pointers).unwrap())
            .unwrap()
            .num_row_groups(),
        2
    );
}

#[test]
fn csr_rejects_duplicate_uids_invalid_bounds_and_fallible_input() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    assert!(
        write_csr_shard(
            root.path(),
            1,
            1,
            [Ok((0, 0, "same".into())), Ok((0, 0, "same".into()))],
            &original
        )
        .is_err()
    );
    assert!(write_csr_shard(root.path(), 1, 1, [Ok((1, 0, "outside".into()))], &original).is_err());
    assert!(write_csr_shard(root.path(), 1, 1, [Ok((0, 1, "outside".into()))], &original).is_err());
    assert!(write_csr_shard(root.path(), 1, 1, [Ok((0, 0, "".into()))], &original).is_err());
    assert!(write_csr_shard(root.path(), u64::MAX, 1, [], &original).is_err());
    let error = write_csr_shard(
        root.path(),
        1,
        1,
        [
            Ok((0, 0, "valid".into())),
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture")),
        ],
        &original,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert!(!root.path().join("edges.indices").exists());
}

#[test]
fn failures_and_original_cancellation_never_publish_concatenated_output() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let input = root.path().join("input");
    write_fact_rows(&input, [Ok("first".into())], &original).unwrap();
    let output = root.path().join("output");
    assert!(
        concat_parquet_rows(
            &output,
            [
                Ok((input.clone(), 0, 1)),
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"))
            ],
            &original
        )
        .is_err()
    );
    assert!(!output.exists());
    let cancellation = Cancellation::default();
    let cancelled = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    ));
    let items = std::iter::once_with(|| {
        cancellation.cancel();
        Ok((input.clone(), 0, 1))
    });
    assert!(concat_parquet_rows(&output, items, &cancelled).is_err());
    assert!(!output.exists());
    assert!(visit_fact_rows(&input, .., &cancelled, |_| panic!("cancelled visitor ran")).is_err());
    let cancellation = Cancellation::default();
    let cancelled = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancellation.clone(),
    ));
    let items = std::iter::once_with(|| {
        cancellation.cancel();
        Ok((0, 0, "edge".into()))
    });
    assert!(write_csr_shard(root.path(), 1, 1, items, &cancelled).is_err());
    assert!(!root.path().join("edges.indices").exists());
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert!(parquet_rows(&input, &expired).is_err());
    assert!(visit_fact_rows(&input, .., &expired, |_| panic!("expired visitor ran")).is_err());
    assert!(concat_parquet_rows(&output, [Ok((input, 0, 1))], &expired).is_err());
    assert!(!output.exists());
}

#[test]
fn repeated_path_slices_reuse_the_operation_footer_for_both_passes() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    let input = root.path().join("input");
    write_fact_rows(&input, [Ok("one".into())], &original).unwrap();
    let output = root.path().join("output");
    let inputs = (0..128).map(|index| {
        if index == 1 {
            // Once the first slice has loaded its footer, make any further
            // footer read fail. Encoded column chunks remain unchanged.
            let mut file = OpenOptions::new().write(true).open(&input).unwrap();
            file.seek(SeekFrom::End(-8)).unwrap();
            file.write_all(&[0; 8]).unwrap();
        }
        Ok((input.clone(), 0, 1))
    });
    let offsets = concat_parquet_rows(&output, inputs, &original).unwrap();
    assert_eq!(
        offsets,
        (0..128).map(|offset| (offset, 1)).collect::<Vec<_>>()
    );
    assert_eq!(parquet_rows(&output, &original).unwrap(), 128);
    let mut values = Vec::new();
    visit_fact_rows(&output, 127.., &original, |value| {
        values.push(value.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(values, ["one"]);
}

#[test]
fn empty_csr_and_zero_length_slices_preserve_local_pointer_zero() {
    let root = tempfile::tempdir().unwrap();
    let original = context();
    assert_eq!(
        write_csr_shard(root.path(), 0, 0, [], &original).unwrap(),
        0
    );
    let indices = root.path().join("edges.indices");
    let indptr = root.path().join("edges.indptr");
    assert_eq!(parquet_rows(&indices, &original).unwrap(), 0);
    assert_eq!(uints(&indptr, "ptr", ..), [0]);
    let output = root.path().join("empty-joined");
    assert_eq!(
        concat_parquet_rows(&output, [Ok((indices, 0, 0))], &original).unwrap(),
        [(0, 0)]
    );
    assert_eq!(parquet_rows(&output, &original).unwrap(), 0);
}
