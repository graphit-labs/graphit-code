//! Prepare the engine from the exact Cargo source. CMake owns incremental builds.
use cargo_metadata::{MetadataCommand, Package};
use std::{
    env,
    ffi::OsStr,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct Prepared {
    source: PathBuf,
    generated: PathBuf,
    library: PathBuf,
    compiler: PathBuf,
    target: String,
    _lock: fs::File,
}
impl Prepared {
    pub(crate) fn apply(&self, command: &mut Command) {
        command
            .env("GRAPHIT_LBUG_SOURCE_DIR", &self.source)
            .env("LBUG_INCLUDE_DIR", &self.generated)
            .env("LBUG_LIBRARY_DIR", &self.library)
            // cc-rs tracks this selection; official binding and both bridges use
            // the compiler CMake actually selected, including on first adoption.
            .env("CXX", &self.compiler)
            .env(format!("CXX_{}", self.target), &self.compiler)
            .env(
                format!("CXX_{}", self.target.replace('-', "_")),
                &self.compiler,
            );
    }
}

fn option(args: &[String], name: &str) -> Result<Option<String>, String> {
    let mut values = Vec::new();
    let mut args = args.iter().take_while(|s| s.as_str() != "--");
    while let Some(arg) = args.next() {
        if arg == name {
            values.push(
                args.next()
                    .ok_or_else(|| format!("{name} requires a value"))?
                    .clone(),
            );
        } else if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            values.push(value.to_owned());
        }
    }
    match values.as_slice() {
        [] => Ok(None),
        [value] if !value.is_empty() => Ok(Some(value.clone())),
        _ => Err(format!("{name} must select exactly one value")),
    }
}

fn parallel_jobs(configured: Option<&OsStr>) -> Result<NonZeroUsize, String> {
    match configured {
        Some(value) => value
            .to_str()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| {
                "CMAKE_BUILD_PARALLEL_LEVEL or CARGO_BUILD_JOBS must be a positive integer".into()
            }),
        None => std::thread::available_parallelism().map_err(|error| error.to_string()),
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let status = command
        // Keep Cargo --message-format output exclusively JSON on stdout.
        .stdout(std::io::stderr())
        .status()
        .map_err(|error| format!("{command:?}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?}: {status}"))
    }
}

fn official_package(metadata: &cargo_metadata::Metadata) -> Result<&Package, String> {
    let nodes = &metadata
        .resolve
        .as_ref()
        .ok_or("Cargo resolution missing")?
        .nodes;
    let mut packages = metadata
        .packages
        .iter()
        .filter(|p| p.name.as_str() == "lbug" && nodes.iter().any(|n| n.id == p.id));
    let package = packages.next().ok_or("resolved lbug package missing")?;
    if packages.next().is_some()
        || package.version.to_string() != "0.22.1"
        || package
            .source
            .as_ref()
            .is_none_or(|s| s.repr != "registry+https://github.com/rust-lang/crates.io-index")
    {
        return Err("requires exactly the official registry lbug 0.22.1, without overrides".into());
    }
    Ok(package)
}

fn tool(cache: &str, name: &str) -> Result<PathBuf, String> {
    cache
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.split_once(':')?.0 == name).then(|| PathBuf::from(value))
        })
        .ok_or_else(|| format!("CMake tool missing: {name}"))
}

fn compiler(variable: &str, target: &str, host: &str) -> Option<std::ffi::OsString> {
    [
        format!("{variable}_{target}"),
        format!("{variable}_{}", target.replace('-', "_")),
        format!(
            "{}_{variable}",
            if target == host { "HOST" } else { "TARGET" }
        ),
        variable.to_owned(),
    ]
    .iter()
    .find_map(env::var_os)
}

pub(crate) fn prepare(root: &Path, args: &[String]) -> Result<Prepared, String> {
    if env::var_os("LBUG_SHARED").is_some() {
        return Err(
            "Graphit's Icebug adapter requires the static Ladybug build; unset LBUG_SHARED".into(),
        );
    }
    if args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "--config" || a.starts_with("--config="))
    {
        return Err("Ladybug preparation does not support Cargo --config overrides; use .cargo/config.toml, Cargo environment variables or --target/--target-dir".into());
    }
    let config = cargo_config2::Config::load_with_cwd(root).map_err(|e| e.to_string())?;
    let host = config.host_triple().map_err(|e| e.to_string())?;
    let targets = config
        .build_target_for_config(option(args, "--target")?.as_deref())
        .map_err(|e| e.to_string())?;
    let [target] = targets.as_slice() else {
        return Err("Ladybug preparation requires exactly one Cargo target per invocation".into());
    };
    let target = target.triple().to_owned();
    if !target
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Ladybug requires a Rust target triple".into());
    }
    if target != host && env::var_os("CMAKE_TOOLCHAIN_FILE").is_none() {
        return Err(format!(
            "Ladybug cross compilation {host} -> {target} requires CMAKE_TOOLCHAIN_FILE"
        ));
    }
    if target.contains("msvc")
        && config
            .rustflags(target.as_str())
            .map_err(|e| e.to_string())?
            .is_some_and(|flags| flags.flags.iter().any(|flag| flag.contains("+crt-static")))
    {
        return Err("official lbug binding uses /MD; +crt-static is incompatible".into());
    }
    let mut metadata = MetadataCommand::new();
    metadata.manifest_path(root.join("Cargo.toml"));
    let mut options = vec![
        "--locked".to_owned(),
        "--filter-platform".to_owned(),
        target.clone(),
    ];
    if args.iter().any(|a| a == "--offline" || a == "--frozen") {
        options.push("--offline".into());
    }
    metadata.other_options(options);
    let metadata = metadata.exec().map_err(|e| e.to_string())?;
    let package = official_package(&metadata)?;
    let source = package
        .manifest_path
        .parent()
        .ok_or("lbug manifest parent missing")?
        .join("lbug-src")
        .into_std_path_buf();
    for file in [
        "CMakeLists.txt",
        "src/include/storage/table/external_table_hooks.h",
        "scripts/collect-single-file-header.py",
    ] {
        if !source.join(file).is_file() {
            return Err(format!("official Cargo package lacks {file}"));
        }
    }
    let target_dir = option(args, "--target-dir")?
        .map(PathBuf::from)
        .unwrap_or_else(|| metadata.target_directory.into_std_path_buf());
    let target_dir = if target_dir.is_absolute() {
        target_dir
    } else {
        root.join(target_dir)
    };
    // Ladybug's Release ABI is shared across Rust dev/test/release profiles.
    let build = target_dir
        .join("native/ladybug")
        .join(&target)
        .join("release");
    let cmake = build.join(if target.contains("msvc") {
        "build-ninja"
    } else {
        "build"
    });
    fs::create_dir_all(&build).map_err(|e| e.to_string())?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(build.join("build.lock"))
        .map_err(|e| e.to_string())?;
    // Keep it through the consumer build: another wrapper must not reconfigure
    // the shared engine while the binding/adapter are compiling against it.
    lock.lock().map_err(|e| e.to_string())?;
    // Visual Studio generators record the selected compiler outside CMakeCache.
    let compiler_script = build.join("record-compiler.cmake");
    fs::write(
        &compiler_script,
        "file(WRITE \"${CMAKE_BINARY_DIR}/graphit-cxx.txt\" \"${CMAKE_CXX_COMPILER}\")\n",
    )
    .map_err(|e| e.to_string())?;
    let mut configure = Command::new("cmake");
    configure
        .current_dir(root)
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&cmake)
        .args([
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_SHELL=OFF",
            "-DBUILD_TESTS=OFF",
            "-DBUILD_EXAMPLES=OFF",
            "-DBUILD_SHARED_LBUG=OFF",
            "-DBUILD_STATIC_LBUG=ON",
            "-DBUILD_SINGLE_FILE_HEADER=ON",
            "-DAUTO_UPDATE_GRAMMAR=OFF",
            "-DENABLE_BACKTRACES=OFF",
            "-DENABLE_RUNTIME_CHECKS=OFF",
            "-DFETCHCONTENT_FULLY_DISCONNECTED=ON",
            "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
        ]);
    let mut compiler_include = std::ffi::OsString::from("-DCMAKE_PROJECT_INCLUDE=");
    compiler_include.push(&compiler_script);
    configure.arg(compiler_include);
    if target.contains("linux") {
        // Upstream's MRI script contains unquoted paths. LLVM ar supports
        // spaces there; GNU ar silently truncates those paths.
        for (name, default) in [("CMAKE_AR", "llvm-ar"), ("CMAKE_RANLIB", "llvm-ranlib")] {
            let selected = PathBuf::from(env::var_os(name).unwrap_or_else(|| default.into()));
            let executable = if selected.components().count() > 1 {
                fs::canonicalize(&selected).map_err(|e| e.to_string())?
            } else {
                env::split_paths(&env::var_os("PATH").unwrap_or_default())
                    .map(|directory| directory.join(&selected))
                    .find(|path| path.is_file())
                    .ok_or_else(|| {
                        format!(
                            "required Ladybug build tool not found on PATH: {}",
                            selected.display()
                        )
                    })?
            };
            let mut argument = std::ffi::OsString::from(format!("-D{name}="));
            argument.push(executable);
            configure.arg(argument);
        }
    }
    if target.contains("msvc") {
        // MSBuild adds static dependencies to lbug.lib before upstream's
        // bundling step appends those same archives. Ninja archives only
        // lbug's objects, leaving bundling to the upstream step.
        configure.args(["-G", "Ninja"]);
    } else if let Some(generator) = env::var_os("CMAKE_GENERATOR") {
        configure.arg("-G").arg(generator);
    }
    for (variable, definition) in [("CC", "CMAKE_C_COMPILER"), ("CXX", "CMAKE_CXX_COMPILER")] {
        if let Some(value) = compiler(variable, &target, host) {
            let mut argument = std::ffi::OsString::from(format!("-D{definition}="));
            argument.push(value);
            configure.arg(argument);
        }
    }
    for name in ["CMAKE_TOOLCHAIN_FILE", "OPENSSL_ROOT_DIR"] {
        if let Some(value) = env::var_os(name) {
            let mut argument = std::ffi::OsString::from(format!("-D{name}="));
            argument.push(value);
            configure.arg(argument);
        }
    }
    if let Some(value) = env::var_os("OPENSSL_DIR") {
        let mut argument = std::ffi::OsString::from("-DOPENSSL_ROOT_DIR=");
        argument.push(value);
        configure.arg(argument);
    }
    if target.contains("apple-darwin") {
        configure.arg(if target.starts_with("aarch64-") {
            "-DCMAKE_OSX_ARCHITECTURES=arm64"
        } else {
            "-DCMAKE_OSX_ARCHITECTURES=x86_64"
        });
    }
    if target.contains("msvc") {
        configure.args([
            "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL",
            "-DCMAKE_POLICY_DEFAULT_CMP0091=NEW",
        ]);
    }
    run(&mut configure)?;
    let compiler = PathBuf::from(
        fs::read_to_string(cmake.join("graphit-cxx.txt")).map_err(|e| e.to_string())?,
    );
    if !compiler.is_file() {
        return Err(format!(
            "CMake selected an invalid C++ compiler: {}",
            compiler.display()
        ));
    }
    let mut compile = Command::new("cmake");
    compile.arg("--build").arg(&cmake).args([
        "--config",
        "Release",
        "--target",
        "single_file_header",
        "--parallel",
    ]);
    let jobs =
        env::var_os("CMAKE_BUILD_PARALLEL_LEVEL").or_else(|| env::var_os("CARGO_BUILD_JOBS"));
    compile.arg(parallel_jobs(jobs.as_deref())?.to_string());
    run(&mut compile)?;
    let cache = fs::read_to_string(cmake.join("CMakeCache.txt")).map_err(|e| e.to_string())?;
    let lib_name = if target.contains("msvc") {
        "lbug.lib"
    } else {
        "liblbug.a"
    };
    let native = cmake.join("src");
    let library = if target.contains("linux") {
        let isolated = build.join("link");
        fs::create_dir_all(&isolated).map_err(|e| e.to_string())?;
        isolate_elf(
            &native.join(lib_name),
            &isolated.join(lib_name),
            &isolated,
            &cache,
        )?;
        isolated
    } else {
        native
    };
    let generated = cmake.join("src");
    // The official external-binding branch accepts one include directory and
    // needs both headers. Stage just its C API header beside CMake's lbug.hpp;
    // ordinary/internal headers remain in the read-only Cargo source.
    run(Command::new("cmake")
        .args(["-E", "copy_if_different"])
        .arg(source.join("src/include/c_api/lbug.h"))
        .arg(generated.join("lbug.h")))?;
    if !generated.join("lbug.hpp").is_file() || !library.join(lib_name).is_file() {
        return Err("Ladybug build did not produce required engine/headers".into());
    }
    eprintln!(
        "Ladybug 0.22.1 from Cargo: {} -> {}",
        source.display(),
        build.display()
    );
    Ok(Prepared {
        source,
        generated,
        library,
        compiler,
        target,
        _lock: lock,
    })
}

// Required only when linking Ladybug's bundled ANTLR/C libraries together with
// Graphit's different ANTLR runtime and Lance. No source or public API changes.
fn isolate_elf(source: &Path, destination: &Path, work: &Path, cache: &str) -> Result<(), String> {
    let input = fs::metadata(source)
        .map_err(|e| e.to_string())?
        .modified()
        .map_err(|e| e.to_string())?;
    if fs::metadata(destination)
        .and_then(|m| m.modified())
        .is_ok_and(|m| m >= input)
    {
        return Ok(());
    }
    let merged = work.join("lbug.o");
    run(Command::new(tool(cache, "CMAKE_LINKER")?)
        .args(["-r", "--whole-archive"])
        .arg(source)
        .args(["--no-whole-archive", "-o"])
        .arg(&merged))?;
    let output = Command::new(tool(cache, "CMAKE_NM")?)
        .args(["--defined-only", "--format=posix"])
        .arg(&merged)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("nm Ladybug archive failed".into());
    }
    let listing = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let (rename, keep) = symbol_rules(&listing);
    let rename_file = work.join("private-antlr-symbols.txt");
    let keep_file = work.join("keep-global.txt");
    fs::write(&rename_file, rename).map_err(|e| e.to_string())?;
    fs::write(&keep_file, keep).map_err(|e| e.to_string())?;
    run(Command::new(tool(cache, "CMAKE_OBJCOPY")?)
        .arg(format!("--redefine-syms={}", rename_file.display()))
        .arg(&merged))?;
    run(Command::new(tool(cache, "CMAKE_OBJCOPY")?)
        .arg(format!("--keep-global-symbols={}", keep_file.display()))
        .arg(&merged))?;
    // ar replaces this one member; no stale archive members can accumulate.
    run(Command::new(tool(cache, "CMAKE_AR")?)
        .arg("rcs")
        .arg(destination)
        .arg(&merged))?;
    fs::remove_file(merged).map_err(|e| e.to_string())?;
    Ok(())
}
fn symbol_rules(listing: &str) -> (String, String) {
    let mut rename = std::collections::BTreeSet::new();
    let mut keep = std::collections::BTreeSet::new();
    for line in listing.lines() {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind)) = (fields.next(), fields.next()) else {
            continue;
        };
        let private = ["6antlr4", "8antlrcpp", "10XPathLexer"]
            .iter()
            .any(|part| name.contains(part));
        let strong = matches!(kind, "T" | "D" | "B" | "R");
        if private {
            let alias = format!("graphit_lbug_private_{name}");
            rename.insert(format!("{name} {alias}\n"));
            if !strong {
                keep.insert(format!("{alias}\n"));
            }
        } else if !strong || name.starts_with("_Z") || name.starts_with("lbug_") {
            keep.insert(format!("{name}\n"));
        }
    }
    (rename.into_iter().collect(), keep.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_build_uses_available_cpus_or_an_explicit_positive_count() {
        assert_eq!(
            parallel_jobs(None).unwrap(),
            std::thread::available_parallelism().unwrap()
        );
        assert_eq!(parallel_jobs(Some(OsStr::new("7"))).unwrap().get(), 7);
        for value in ["", "0", "-1", "not-a-number"] {
            assert!(parallel_jobs(Some(OsStr::new(value))).is_err());
        }
    }
    #[test]
    fn target_options_preserve_spaces_and_reject_multiple_targets() {
        assert_eq!(
            option(&["--target-dir=a space/é".into()], "--target-dir")
                .unwrap()
                .as_deref(),
            Some("a space/é")
        );
        assert!(option(&["--target".into()], "--target").is_err());
        assert!(option(&["--target=a".into(), "--target=b".into()], "--target").is_err());
        assert_eq!(
            option(&["--".into(), "--target=a".into()], "--target").unwrap(),
            None
        );
    }
    #[test]
    fn isolate_only_conflicting_private_symbols_preserve_public_api() {
        let (rename, keep) = symbol_rules(
            "_ZN6antlr4x T 0 0\n_ZTVN6antlr4x V 0 0\nZSTD_compress T 0 0\nlbug_query T 0 0\n_ZNlbug_public T 0 0\n",
        );
        assert!(rename.contains("_ZTVN6antlr4x graphit_lbug_private__ZTVN6antlr4x"));
        assert!(keep.contains("graphit_lbug_private__ZTVN6antlr4x"));
        assert!(!keep.contains("ZSTD_compress"));
        assert!(!keep.contains("graphit_lbug_private__ZN6antlr4x\n"));
        assert!(keep.contains("lbug_query\n") && keep.contains("_ZNlbug_public\n"));
    }
}
