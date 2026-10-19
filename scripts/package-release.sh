#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_dir=$(cd "$script_dir/.." && pwd)
platform=${1:?platform is required}
artifacts_json=${2:?Cargo Release artifact JSONL is required}
output_dir=${3:?output directory is required}
case "$platform" in
  linux-amd64) suffix=; cargo_target=x86_64-unknown-linux-gnu ;;
  darwin-arm64) suffix=; cargo_target=aarch64-apple-darwin ;;
  windows-amd64) suffix=.exe; cargo_target=x86_64-pc-windows-msvc ;;
  *) echo "unsupported release platform: $platform" >&2; exit 1 ;;
esac
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
package_name="graphit-$platform"
stage="$work_dir/$package_name"
mkdir -p "$stage/graphit-runtime"
# Cargo selects the exact optimized executables; do not package an arbitrary bin directory.
node --input-type=module - "$artifacts_json" "$stage" "$suffix" <<'NODE'
import fs from "node:fs";
import path from "node:path";
const [receipt, stage, suffix] = process.argv.slice(2);
const messages = fs.readFileSync(receipt, "utf8").split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
const finished = messages.at(-1);
if (finished?.reason !== "build-finished" || finished.success !== true) {
  throw new Error("Cargo Release build did not finish successfully");
}
const selected = [];
for (const name of ["graphit", "graphit-model-server"]) {
  const artifacts = messages.filter((message) => message.reason === "compiler-artifact" &&
    message.target?.name === name && message.target.kind?.includes("bin") && message.executable);
  if (artifacts.length !== 1) throw new Error(`expected one Cargo Release executable: ${name}`);
  const artifact = artifacts[0];
  const profile = artifact.profile;
  if (!profile || !["1", "2", "3", "s", "z"].includes(profile.opt_level) ||
      profile.debug_assertions !== false || profile.test !== false) {
    throw new Error(`refusing Debug/test or unoptimized executable: ${name}`);
  }
  const stat = fs.statSync(artifact.executable);
  if (path.basename(artifact.executable) !== name + suffix || !stat.isFile() || stat.size === 0) {
    throw new Error(`Cargo Release executable path mismatch: ${name}`);
  }
  if (!suffix) fs.accessSync(artifact.executable, fs.constants.X_OK);
  selected.push([artifact.executable, path.join(stage, name + suffix)]);
}
for (const [source, destination] of selected) fs.copyFileSync(source, destination);
NODE
cp "$repo_dir/LICENSE" "$stage/graphit-runtime/LICENSE"
cp "$repo_dir/docs/development/native-runtime-packaging.md" "$stage/graphit-runtime/NATIVE-RUNTIME.md"
mkdir -p "$stage/graphit-runtime/agents/coordinator"
for source in AGENT.md prompt.md; do
  cp "$repo_dir/templates/agent-instructions/coordinator/$source" "$stage/graphit-runtime/agents/coordinator/$source"
done

# Resolve the exact source packages already used by Cargo, without fetching or building.
cargo metadata --locked --offline --format-version 1 --filter-platform "$cargo_target" \
  --manifest-path "$repo_dir/Cargo.toml" > "$work_dir/cargo-metadata.json"
node --input-type=module - "$work_dir/cargo-metadata.json" "$repo_dir" "$stage" "$platform" "$artifacts_json" <<'NODE'
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const [metadataPath, repo, stage, platform, artifactsPath] = process.argv.slice(2);
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
if (!Array.isArray(metadata.resolve?.nodes) || !Array.isArray(metadata.packages)) {
  throw new Error("Cargo metadata must include its resolved dependency graph");
}
const resolved = new Set(metadata.resolve.nodes.map((node) => node.id));
const sources = {};
const copyNotice = (source, destination) => {
  const stat = fs.lstatSync(source);
  if (!stat.isFile() || stat.size === 0) throw new Error("missing regular native source notice");
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
};
const llama = metadata.packages.filter((pkg) => pkg.name === "llama-cpp-sys-2" && resolved.has(pkg.id));
if (llama.length !== 1 || llama[0].version !== "0.1.161" ||
    !llama[0].source?.endsWith("#424d1ffe6c1c4212d5da8e6075f473e33e35fb7f")) {
  throw new Error("expected the locked upstream llama-cpp-sys-2 dependency");
}
const llamaRoot = path.dirname(llama[0].manifest_path);
const llamaNotices = path.join(stage, "graphit-runtime/licenses/cargo/llama-cpp-sys-2");
for (const [relative, notice] of [["llama.cpp/LICENSE", "llama.cpp/LICENSE"],
    ["llama.cpp/licenses/LICENSE-jsonhpp", "llama.cpp/licenses/LICENSE-jsonhpp"],
    ["../LICENSE-MIT", "LICENSE-MIT"], ["../LICENSE-APACHE", "LICENSE-APACHE"]]) {
  copyNotice(path.join(llamaRoot, relative), path.join(llamaNotices, notice));
}
sources["llama-cpp-sys-2"] = { version: llama[0].version, source: llama[0].source };
// Redistribution notices are retained for the optional Cargo-built CUDA backend.
copyNotice(path.join(repo, "crates/graphit-ai/licenses/NVIDIA-CUDA-LICENSE"),
  path.join(stage, "graphit-runtime/licenses/nvidia/LICENSE"));
for (const [name, version] of [
  ["lance", "14.0.2"],
  ["lbug", "0.22.1"],
]) {
  const packages = metadata.packages.filter((pkg) => pkg.name === name && resolved.has(pkg.id));
  if (packages.length !== 1 || packages[0].version !== version) {
    throw new Error(`expected exactly one resolved ${name} ${version} Cargo package`);
  }
  const pkg = packages[0];
  if (pkg.source !== "registry+https://github.com/rust-lang/crates.io-index") {
    throw new Error("Native database SDKs must resolve from the official crates.io registry");
  }
  const sourceRoot = path.dirname(pkg.manifest_path);
  const destination = path.join(stage, "graphit-runtime/licenses/cargo", name);
  if (name === "lance") {
    // The published crate omits the repository's license. Retain the exact
    // upstream notice in Code; no fork or source download is needed to package.
    copyNotice(path.join(repo, "crates/graphit-data/licenses/lance.LICENSE"), path.join(destination, "LICENSE"));
  } else {
    copyNotice(path.join(repo, "crates/graphit-ast/licenses/ladybug.LICENSE"), path.join(destination, "LICENSE"));
  }
  if (name === "lbug") {
    const thirdParty = "lbug-src/third_party";
    let notices = 0;
    const retain = (relative) => {
      for (const entry of fs.readdirSync(path.join(sourceRoot, relative), { withFileTypes: true })) {
        const child = path.join(relative, entry.name);
        if (entry.isDirectory()) retain(child);
        else if (entry.isFile() && /^(LICENSE|NOTICE|COPYING)/.test(entry.name)) {
          copyNotice(path.join(sourceRoot, child), path.join(destination, child));
          notices++;
        }
      }
    };
    retain(thirdParty);
    if (notices === 0) throw new Error("missing Ladybug third-party notices");
  }
  sources[name] = { version: pkg.version, source: pkg.source };
}
if (platform === "linux-amd64") {
  for (const [name, notices] of [
    ["tikv-jemallocator", ["LICENSE-APACHE", "LICENSE-MIT"]],
    ["tikv-jemalloc-sys", ["LICENSE-APACHE", "LICENSE-MIT", "jemalloc/COPYING"]],
  ]) {
    const packages = metadata.packages.filter((pkg) => pkg.name === name && resolved.has(pkg.id));
    if (packages.length !== 1 || (name === "tikv-jemallocator" && packages[0].version !== "0.7.0")) {
      throw new Error(`expected one resolved Linux allocator Cargo package: ${name}`);
    }
    const pkg = packages[0];
    if (typeof pkg.source !== "string" || !pkg.source.startsWith("registry+")) {
      throw new Error("Linux allocator must use its locked registry source");
    }
    const sourceRoot = path.dirname(pkg.manifest_path);
    const destination = path.join(stage, "graphit-runtime/licenses/cargo", name);
    for (const notice of notices) {
      copyNotice(path.join(sourceRoot, notice), path.join(destination, notice));
    }
    sources[name] = { version: pkg.version, source: pkg.source };
  }
}
fs.writeFileSync(path.join(stage, "graphit-runtime/build.json"), JSON.stringify({
  platform,
  cargo_lock_sha256: createHash("sha256").update(fs.readFileSync(path.join(repo, "Cargo.lock"))).digest("hex"),
  native_sources: sources,
}, null, 2) + "\n");
NODE

# Linux redistributable dependencies use the platform loader, never a shell launcher.
# Only staging copies are patched; the selected Cargo artifacts remain untouched.
if [ "$platform" = linux-amd64 ]; then
  node --input-type=module - "$stage" "$artifacts_json" "$work_dir/cargo-metadata.json" <<'NODE'
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
const [stage, receipt, metadataPath] = process.argv.slice(2);
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
const selected = metadata.packages.find((pkg) => pkg.name === "llama-cpp-sys-2" && metadata.resolve.nodes.some((node) => node.id === pkg.id));
const messages = fs.readFileSync(receipt, "utf8").trim().split(/\r?\n/).map((line) => JSON.parse(line));
const builds = messages.filter((message) => message.reason === "build-script-executed" && message.package_id === selected.id);
if (builds.length > 1) throw new Error("ambiguous Cargo engine build output");
const engineRoot = builds.length ? fs.realpathSync(builds[0].out_dir) : undefined;
const needsCuda = builds.some((build) => build.linked_libs?.some((name) => name.endsWith("ggml-cuda")));
const toolkitRoot = needsCuda && process.env.CUDA_PATH ? fs.realpathSync(process.env.CUDA_PATH) : undefined;
const below = (file, directory) => directory && file.startsWith(directory + path.sep);
const search = [engineRoot && path.join(engineRoot, "lib"), engineRoot && path.join(engineRoot, "lib64"),
  toolkitRoot && path.join(toolkitRoot, "lib64"), toolkitRoot && path.join(toolkitRoot, "targets/x86_64-linux/lib")]
  .filter((directory) => directory && fs.existsSync(directory));
const runtime = path.join(stage, "graphit-runtime");
const libraries = path.join(runtime, "lib");
const notices = path.join(runtime, "licenses/linux");
const environment = { ...process.env, LC_ALL: "C" };
delete environment.LD_LIBRARY_PATH;
delete environment.LD_PRELOAD;
const run = (command, args, env = environment) => {
  const result = spawnSync(command, args, { encoding: "utf8", env });
  if (result.error || result.status !== 0) throw new Error(`Linux runtime tool failed: ${command}: ${result.error?.code ?? result.stderr.trim()}`);
  return result.stdout.trim();
};
run("patchelf", ["--version"]);
const host = new Set(["libc.so.6", "libm.so.6", "libdl.so.2", "libpthread.so.0", "librt.so.1", "ld-linux-x86-64.so.2"]);
const engineNames = new Set(["libllama.so", "libggml.so", "libggml-base.so", "libggml-cpu.so", "libggml-cuda.so"]);
const nvidiaNames = new Set(["libcudart.so.12", "libcublas.so.12", "libcublasLt.so.12"]);
const redistributable = new Set(["libstdc++.so.6", "libgcc_s.so.1", "libssl.so.3", "libcrypto.so.3", "libz.so.1", "libzstd.so.1"]);
const resolved = new Map();
const hash = (file) => {
  const digest = createHash("sha256"), buffer = Buffer.alloc(65536), fd = fs.openSync(file, "r");
  try { for (let size; (size = fs.readSync(fd, buffer)) > 0;) digest.update(buffer.subarray(0, size)); }
  finally { fs.closeSync(fd); }
  return digest.digest("hex");
};
const copy = (source, destination) => {
  if (!fs.statSync(source).isFile() || fs.statSync(source).size === 0) throw new Error("missing Linux runtime library or license");
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
};
const inspect = (file) => {
  for (const line of run("ldd", [file], { ...environment, LD_LIBRARY_PATH: search.join(path.delimiter) }).split("\n")) {
    if (line.includes("=> not found")) throw new Error("unresolved Linux runtime dependency");
    const match = line.trim().match(/^(\S+) => (\/[^\n]+?) \(0x[0-9a-f]+\)$/);
    if (!match) {
      if (/^\s*(linux-vdso\.so\.1|\/[^\n]*\/ld-linux-x86-64\.so\.2)\s+\(0x[0-9a-f]+\)$/.test(line)) continue;
      throw new Error("unrecognized Linux dependency resolution");
    }
    const [, name, source] = match;
    if (host.has(name)) continue;
    if (!redistributable.has(name) && !engineNames.has(name) && !nvidiaNames.has(name)) throw new Error(`unsupported Linux runtime dependency: ${name}`);
    const real = fs.realpathSync(source);
    if (resolved.has(name) && resolved.get(name) !== real) throw new Error("conflicting Linux runtime dependency");
    resolved.set(name, real);
  }
};
const executables = ["graphit", "graphit-model-server"];
for (const name of executables) inspect(path.join(stage, name));
const packages = new Map();
const manifest = { platform: "linux-amd64", host_libraries: [...host].sort(), libraries: [], executables: [] };
for (const [name, source] of [...resolved].sort(([a], [b]) => a.localeCompare(b))) {
  if (engineNames.has(name) || nvidiaNames.has(name)) {
    const engine = engineNames.has(name);
    if (!below(source, engine ? engineRoot : toolkitRoot)) throw new Error("native runtime is outside its selected Cargo build/toolkit");
    const destination = path.join(libraries, name);
    copy(source, destination);
    const input_sha256 = hash(destination);
    if (run("patchelf", ["--print-soname", destination]) !== name) throw new Error("native runtime SONAME mismatch");
    run("patchelf", ["--set-rpath", "$ORIGIN", destination]);
    const origin = engine ? { kind: "cargo", version: selected.version, source: selected.source } :
      { kind: "nvidia", file: path.basename(source), notice: "licenses/nvidia/LICENSE" };
    manifest.libraries.push({ soname: name, origin, input_sha256, sha256: hash(destination), runpath: "$ORIGIN" });
    continue;
  }
  // usrmerge can resolve /lib to /usr/lib while dpkg retains the /lib spelling.
  let ownedPath = source;
  let found = spawnSync("dpkg-query", ["--search", ownedPath], { encoding: "utf8", env: environment });
  if (found.status !== 0 && source.startsWith("/usr/lib/") && fs.realpathSync(source.slice(4)) === source) {
    ownedPath = source.slice(4);
    found = spawnSync("dpkg-query", ["--search", ownedPath], { encoding: "utf8", env: environment });
  }
  if (found.error || found.status !== 0) throw new Error("Linux library has no distribution package owner");
  const owner = found.stdout.trim().split("\n");
  if (owner.length !== 1 || !owner[0].endsWith(": " + ownedPath)) throw new Error("Linux library must have one installed distribution package owner");
  const binaryPackage = owner[0].slice(0, -ownedPath.length - 2);
  const fields = run("dpkg-query", ["--show", "--showformat=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}", binaryPackage]).split("\t");
  if (fields.length !== 4 || fields.some((value) => !value || !/^[a-zA-Z0-9.+:~_-]+$/.test(value))) throw new Error("invalid Linux distribution package provenance");
  const [pkg, version, sourcePackage, sourceVersion] = fields;
  const packageName = pkg.split(":")[0];
  if (!packages.has(pkg)) {
    const copyright = `/usr/share/doc/${packageName}/copyright`;
    copy(copyright, path.join(notices, packageName, "copyright"));
    // Debian copyright files refer to these canonical full license texts.
    for (const entry of fs.readdirSync("/usr/share/common-licenses")) {
      copy(path.join("/usr/share/common-licenses", entry), path.join(notices, "common-licenses", entry));
    }
    packages.set(pkg, { package: pkg, version, source_package: sourcePackage, source_version: sourceVersion });
  }
  const destination = path.join(libraries, name);
  copy(source, destination);
  const input_sha256 = hash(destination);
  if (run("patchelf", ["--print-soname", destination]) !== name) throw new Error("Linux runtime SONAME mismatch");
  run("patchelf", ["--set-rpath", "$ORIGIN", destination]);
  manifest.libraries.push({ soname: name, ...packages.get(pkg), input_sha256, sha256: hash(destination), runpath: "$ORIGIN" });
}
for (const name of executables) {
  const file = path.join(stage, name);
  const input_sha256 = hash(file);
  run("patchelf", ["--set-rpath", "$ORIGIN/graphit-runtime/lib", file]);
  manifest.executables.push({ name, input_sha256, sha256: hash(file), runpath: "$ORIGIN/graphit-runtime/lib" });
}
// Check direct and transitive resolution after patching, with no environment override.
for (const file of [...executables.map((name) => path.join(stage, name)), ...[...resolved.keys()].map((name) => path.join(libraries, name))]) {
  for (const line of run("ldd", [file]).split("\n")) {
    const match = line.trim().match(/^(\S+) => (\/[^\n]+?) \(0x[0-9a-f]+\)$/);
    if (match && !host.has(match[1]) && fs.realpathSync(match[2]) !== fs.realpathSync(path.join(libraries, match[1]))) throw new Error("Linux runtime resolved outside the package");
    if (line.includes("=> not found")) throw new Error("unresolved packaged Linux dependency");
  }
}
manifest.required_glibc_versions = [...new Set([
  ...executables.map((name) => path.join(stage, name)),
  ...[...resolved.keys()].map((name) => path.join(libraries, name)),
].flatMap((file) => [...run("readelf", ["--version-info", file]).matchAll(/\bGLIBC_(\d+(?:\.\d+)+)\b/g)].map((match) => match[1])))].sort((a, b) => a.localeCompare(b, "en", { numeric: true }));
fs.writeFileSync(path.join(runtime, "linux-libraries.json"), JSON.stringify(manifest, null, 2) + "\n");
NODE
fi

# Stage platform redistributables and validate the complete native dependency closure.
if [ "$platform" = darwin-arm64 ] || [ "$platform" = windows-amd64 ]; then
  node "$script_dir/package-platform-runtime.mjs" "$platform" "$stage" graphit-runtime "graphit$suffix" "graphit-model-server$suffix"
fi

# Preserve the product's own native grammar notices at their repository-relative paths.
notice_roots=(
  crates/graphit-ast/src/antlr_engine/cpp/vendor/antlr4/LICENSE.txt
  crates/graphit-ast/src/antlr_engine/cpp/generated/GRAMMAR_NOTICES.md
  crates/graphit-ast/grammars/tree_sitter
)
(cd "$repo_dir" && find "${notice_roots[@]}" -type f \( -name 'LICENSE*' -o -name 'NOTICE*' -o -name 'COPYING*' -o -name GRAMMAR_NOTICES.md \) -print0) |
  while IFS= read -r -d '' notice; do
    destination="$stage/graphit-runtime/licenses/$notice"
    mkdir -p "$(dirname "$destination")"
    cp "$repo_dir/$notice" "$destination"
  done

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}
mkdir -p "$output_dir"
archive="$output_dir/$package_name.tar.gz"
tar -C "$work_dir" -czf "$archive" "$package_name"
printf '%s  %s\n' "$(hash_file "$archive")" "$(basename "$archive")" > "$archive.sha256"
printf '%s\n' "$archive"
