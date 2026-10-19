//! Local release qualification; setup/warmup and process costs are distinct.

use graphit_core::{
    local_files::read_regular,
    native_path::{MAX_PATH_BYTES, NativePath},
};
use std::{
    error::Error,
    fs::{self, File},
    hint::black_box,
    io,
    path::PathBuf,
    time::Instant,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let scenario = arguments
        .next()
        .ok_or_else(|| io::Error::other("scenario required: path, path-cap, read, sparse"))?;
    let iterations = arguments
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1000);
    if !(1..=1_000_000).contains(&iterations) || arguments.next().is_some() {
        return Err(io::Error::other("expected a scenario and 1..1000000 iterations").into());
    }
    let root = tempfile::tempdir()?;
    let file = root.path().join("metadata");
    let path_units = if scenario == "path-cap" {
        MAX_PATH_BYTES / if cfg!(windows) { 2 } else { 1 }
    } else {
        80
    };
    let prefix = if cfg!(windows) { "C:\\" } else { "/" };
    let path = NativePath::new(PathBuf::from(format!(
        "{prefix}{}",
        "x".repeat(path_units - prefix.len())
    )))?;
    let (payload_bytes, serialized_bytes) = match scenario.as_str() {
        "path" | "path-cap" => (
            path_units * if cfg!(windows) { 2 } else { 1 },
            serde_json::to_vec(&path)?.len(),
        ),
        "read" => {
            fs::write(&file, vec![42; 64 * 1024])?;
            (64 * 1024, 0)
        }
        "sparse" => {
            File::create(&file)?.set_len(128 * 1024 * 1024)?;
            (0, 0)
        }
        _ => return Err(io::Error::other("unknown scenario").into()),
    };
    let operation = || -> Result<(), Box<dyn Error>> {
        match scenario.as_str() {
            "path" | "path-cap" => {
                let encoded = serde_json::to_vec(black_box(&path))?;
                let decoded: NativePath = serde_json::from_slice(black_box(&encoded))?;
                if decoded != path {
                    return Err(io::Error::other("path roundtrip differs").into());
                }
                black_box(decoded);
            }
            "read" => {
                let (bytes, _) = read_regular(black_box(&file), 64 * 1024)?
                    .ok_or_else(|| io::Error::other("fixture missing"))?;
                if bytes.len() != 64 * 1024 {
                    return Err(io::Error::other("read differs").into());
                }
                black_box(bytes);
            }
            "sparse" => {
                if read_regular(black_box(&file), 4 * 1024).unwrap_err().kind()
                    != io::ErrorKind::InvalidData
                {
                    return Err(io::Error::other("sparse budget was not rejected").into());
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    };
    for _ in 0..10 {
        operation()?;
    }
    let mut samples = Vec::with_capacity(iterations);
    let started = Instant::now();
    for _ in 0..iterations {
        let operation_started = Instant::now();
        operation()?;
        samples.push(operation_started.elapsed().as_nanos());
    }
    let loop_ns = started.elapsed().as_nanos();
    let timed_ns: u128 = samples.iter().sum();
    samples.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "scenario": scenario, "iterations": iterations, "warmups": 10,
            "timed_ns": timed_ns, "loop_ns": loop_ns,
            "p50_ns": samples[(iterations * 50).div_ceil(100) - 1],
            "p95_ns": samples[(iterations * 95).div_ceil(100) - 1],
            "payload_bytes": payload_bytes, "serialized_bytes": serialized_bytes,
            "operation_writes": 0,
            "scope": "local data I/O under existing context; setup and warmup excluded from operation/loop, included in process"
        })
    );
    Ok(())
}
