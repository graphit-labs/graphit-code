import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "graphit-native-packaging-"));
const project = path.join(temp, "project");
const bin = path.join(temp, "tools");
const hash = (file) => createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const run = (command, args, env = {}) =>
  spawnSync(command, args, {
    cwd: project,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: bin + path.delimiter + process.env.PATH,
      ...env,
    },
  });
const okay = (result) => assert.equal(result.status, 0, result.stderr || result.error?.message);
const write = (name, text) => {
  fs.mkdirSync(path.dirname(name), { recursive: true });
  fs.writeFileSync(name, text);
};
const output = (name) => path.join(temp, name);
const ggufSource = path.join(temp, "cargo-cache/binding/llama-cpp-sys-2");
const ggufNotices = ["LICENSE", "licenses/LICENSE-jsonhpp"];
const llamaSource = "git+https://github.com/utilityai/llama-cpp-rs?rev=424d1ffe6c1c4212d5da8e6075f473e33e35fb7f#424d1ffe6c1c4212d5da8e6075f473e33e35fb7f";
const allocatorNotices = [
  "tikv-jemallocator/LICENSE-APACHE", "tikv-jemallocator/LICENSE-MIT",
  "tikv-jemalloc-sys/LICENSE-APACHE", "tikv-jemalloc-sys/LICENSE-MIT", "tikv-jemalloc-sys/jemalloc/COPYING",
];
const jemallocSysVersion = "0.7.1+5.3.1-0-g81034ce1f1373e37dc865038e1bc8eeecf559ce8";
const registrySource = "registry+https://github.com/rust-lang/crates.io-index";

const artifactsReceipt = output("release-artifacts.jsonl");
const artifactMessages = () => [
  ...["graphit", "graphit-model-server"].map((name) => ({
    reason: "compiler-artifact", target: { name, kind: ["bin"] },
    executable: path.join(output("binaries"), name),
    profile: { opt_level: "3", debug_assertions: false, test: false },
  })),
  { reason: "build-script-executed", package_id: "unrelated-package", out_dir: output("unrelated-build") },
  { reason: "build-finished", success: true },
];
const receipt = (mutate = () => {}) => {
  const messages = artifactMessages();
  mutate(messages);
  write(artifactsReceipt, messages.map((message) => JSON.stringify(message)).join("\n") + "\n");
};
let passed = 0;
const selectedChecks = process.argv.slice(2);
const check = (label, fn) => {
  if (selectedChecks.length && !selectedChecks.some((selected) => label.includes(selected))) return;
  fn();
  passed++;
  console.log("PASS " + label);
};

try {
  fs.mkdirSync(project, { recursive: true });
  fs.mkdirSync(bin);
  for (const script of ["package-release.sh", "package-platform-runtime.mjs"]) {
    write(path.join(project, "scripts", script), fs.readFileSync(path.join(repo, "scripts", script)));
  }
  write(path.join(project, "install.sh"), fs.readFileSync(path.join(repo, "install.sh")));
  write(path.join(project, "LICENSE"), "product fixture license\n");
  for (const notice of ggufNotices) write(path.join(ggufSource, "llama.cpp", notice), "upstream notice: " + notice);
  for (const notice of ["LICENSE-MIT", "LICENSE-APACHE"]) write(path.join(ggufSource, "..", notice), "binding notice: " + notice);
  write(path.join(project, "crates/graphit-ai/licenses/NVIDIA-CUDA-LICENSE"), "NVIDIA redistribution notice");
  for (const notice of ["crates/graphit-ast/src/antlr_engine/cpp/vendor/antlr4/LICENSE.txt", "crates/graphit-ast/src/antlr_engine/cpp/generated/GRAMMAR_NOTICES.md", "crates/graphit-ast/grammars/tree_sitter/test/LICENSE"]) {
    write(path.join(project, notice), "fixture native source notice: " + notice + "\n");
  }
  write(path.join(project, "Cargo.lock"), "fixture dependency pin\n");
  // Cargo sources live outside the product checkout, with no vendor or sibling lookup.
  const cargoSources = path.join(temp, "cargo-cache", "resolved-forks");
  for (const notice of ["lbug/lbug-src/third_party/test/LICENSE", ...allocatorNotices]) {
    write(path.join(cargoSources, notice), "fixture resolved source notice: " + notice + "\n");
  }
  write(path.join(project, "crates/graphit-data/licenses/lance.LICENSE"), "fixture official Lance notice\n");
  write(path.join(project, "crates/graphit-ast/licenses/ladybug.LICENSE"), "fixture official Ladybug notice\n");
  const cargoMetadata = path.join(temp, "cargo-metadata.json");
  const resolvedPackages = [
    { id: "resolved-llama", name: "llama-cpp-sys-2", version: "0.1.161", source: llamaSource, manifest_path: path.join(ggufSource, "Cargo.toml") },
    { id: "resolved-lance", name: "lance", version: "14.0.2", source: registrySource, manifest_path: path.join(cargoSources, "lance/Cargo.toml") },
    { id: "resolved-lbug", name: "lbug", version: "0.22.1", source: registrySource, manifest_path: path.join(cargoSources, "lbug/Cargo.toml") },
    { id: "resolved-graphit-ai", name: "graphit-ai", version: "0.1.0", source: null, manifest_path: path.join(project, "crates/graphit-ai/Cargo.toml") },
    { id: "resolved-tikv-jemallocator", name: "tikv-jemallocator", version: "0.7.0", source: registrySource, manifest_path: path.join(cargoSources, "tikv-jemallocator/Cargo.toml") },
    { id: "resolved-tikv-jemalloc-sys", name: "tikv-jemalloc-sys", version: jemallocSysVersion, source: registrySource, manifest_path: path.join(cargoSources, "tikv-jemalloc-sys/Cargo.toml") },
  ];
  for (const pkg of resolvedPackages) write(pkg.manifest_path, "[package]\n");
  const metadata = {
    packages: [...resolvedPackages, { id: "unresolved-upstream-lance", name: "lance", version: "14.0.2", source: "registry+https://github.com/rust-lang/crates.io-index", manifest_path: path.join(temp, "unresolved-upstream/Cargo.toml") }],
    resolve: { nodes: resolvedPackages.map((pkg) => ({ id: pkg.id })) },
  };
  write(cargoMetadata, JSON.stringify(metadata));
  write(path.join(project, "Cargo.toml"), "[workspace]\n");
  process.env.FIXTURE_CARGO_METADATA = cargoMetadata;
  process.env.FIXTURE_CARGO_MANIFEST = path.join(project, "Cargo.toml");
  process.env.FIXTURE_CARGO_CALLS = path.join(temp, "cargo-calls.jsonl");
  write(path.join(bin, "cargo"), `#!/usr/bin/env node
const fs = require("node:fs");
const assert = require("node:assert/strict");
assert.deepEqual(process.argv.slice(2), ["metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", "x86_64-unknown-linux-gnu", "--manifest-path", process.env.FIXTURE_CARGO_MANIFEST]);
fs.appendFileSync(process.env.FIXTURE_CARGO_CALLS, JSON.stringify(process.argv.slice(2)) + "\\n");
if (process.env.FIXTURE_CARGO_FAIL === "1") { process.stderr.write("fixture offline Cargo metadata unavailable"); process.exit(42); }
process.stdout.write(fs.readFileSync(process.env.FIXTURE_CARGO_METADATA));
`);
  fs.chmodSync(path.join(bin, "cargo"), 0o755);
  const linuxLibraries = output("linux-libraries");
  const linuxNames = ["libstdc++.so.6", "libgcc_s.so.1", "libssl.so.3", "libcrypto.so.3", "libz.so.1", "libzstd.so.1"];
  for (const name of linuxNames) write(path.join(linuxLibraries, name), "controlled Linux library: " + name);
  process.env.FIXTURE_LINUX_LIBRARIES = linuxLibraries;
  const tool = (name, source) => {
    write(path.join(bin, name), "#!/usr/bin/env node\n" + source);
    fs.chmodSync(path.join(bin, name), 0o755);
  };
  tool("patchelf", `
import fs from "node:fs";
import path from "node:path";
const [operation, value, file] = process.argv.slice(2);
if (process.env.FIXTURE_LINUX_ISSUE === "patch-tool") process.exit(42);
if (operation === "--version") console.log("controlled patchelf");
else if (operation === "--print-soname") console.log(process.env.FIXTURE_LINUX_ISSUE === "soname" ? "wrong.so" : path.basename(value));
else if (operation === "--set-rpath") fs.appendFileSync(file, "\\nRUNPATH=" + value);
else process.exit(43);
`);
  tool("ldd", `
import fs from "node:fs";
import path from "node:path";
const file = process.argv[2];
if (process.env.FIXTURE_LINUX_ISSUE === "missing-library") { console.log("libssl.so.3 => not found"); process.exit(0); }
if (process.env.FIXTURE_LINUX_ISSUE === "unknown-library") { console.log("libunknown.so.1 => /usr/lib/libunknown.so.1 (0x1234)"); process.exit(0); }
if (process.env.FIXTURE_LINUX_ISSUE === "openblas") { console.log("libopenblas.so.0 => /usr/lib/libopenblas.so.0 (0x1234)"); process.exit(0); }
const patched = fs.readFileSync(file, "utf8").includes("RUNPATH=");
const root = path.basename(path.dirname(file)) === "lib" ? path.dirname(file) : path.join(path.dirname(file), "graphit-runtime/lib");
const directory = patched && process.env.FIXTURE_LINUX_ISSUE !== "outside-package" ? root : process.env.FIXTURE_LINUX_LIBRARIES;
for (const name of ${JSON.stringify(linuxNames)}) console.log(name + " => " + path.join(directory, name) + " (0x1234)");
if (process.env.FIXTURE_NATIVE_ENGINE) {
  const selected = process.env.FIXTURE_LINUX_ISSUE === "outside-engine" ? process.env.FIXTURE_NATIVE_OUTSIDE : process.env.FIXTURE_NATIVE_ENGINE;
  for (const name of ["libllama.so", "libggml.so", "libggml-base.so", "libggml-cpu.so", "libggml-cuda.so"])
    console.log(name + " => " + path.join(patched ? root : path.join(selected, "lib"), name) + " (0x1234)");
  for (const name of ["libcudart.so.", "libcublas.so.", "libcublasLt.so."].map(name => name + (process.env.FIXTURE_CUDA_MAJOR || "12")))
    console.log(name + " => " + path.join(patched ? root : path.join(process.env.CUDA_PATH, "lib64"), name) + " (0x1234)");
}

console.log("libc.so.6 => /lib/libc.so.6 (0x1234)\\nlibm.so.6 => /lib/libm.so.6 (0x1234)\\n/lib64/ld-linux-x86-64.so.2 (0x1234)");
`);
  tool("dpkg-query", `
import path from "node:path";
const args = process.argv.slice(2);
if (args[0] === "--search") console.log("zlib1g:amd64: " + args[1]);
else console.log((process.env.FIXTURE_LINUX_ISSUE === "missing-license" ? "no-such-runtime-package" : "zlib1g:amd64") + "\\t1.3.1\\tzlib\\t1.3.1");
`);
  tool("readelf", `console.log("Name: GLIBC_2.34");`);
  const withMetadata = (change, fn) => {
    const original = fs.readFileSync(cargoMetadata);
    try {
      const value = JSON.parse(original);
      change(value);
      write(cargoMetadata, JSON.stringify(value));
      fn();
    } finally {
      write(cargoMetadata, original);
    }
  };
  write(path.join(project, "docs/development/native-runtime-packaging.md"), "fixture runtime instructions\n");
  for (const source of ["AGENT.md", "prompt.md"]) {
    write(path.join(project, "templates/agent-instructions/coordinator", source), fs.readFileSync(path.join(repo, "templates/agent-instructions/coordinator", source)));
  }
  write(path.join(bin, "curl"), "#!/usr/bin/env bash\nset -eu\nurl=\nfor arg in \"$@\"; do\n  destination=$arg\n  case \"$arg\" in https://*) url=$arg ;; esac\ndone\nprintf 'download\\n' >> \"$FIXTURE_REQUESTS\"\ncase \"$url\" in\n  *.tar.gz.sha256) cp \"$FIXTURE_RELEASE_SHA\" \"$destination\" ;;\n  *.tar.gz) cp \"$FIXTURE_RELEASE\" \"$destination\" ;;\n  *) exit 44 ;;\nesac\n");
  fs.chmodSync(path.join(bin, "curl"), 0o755);
  process.env.FIXTURE_REQUESTS = path.join(temp, "requests");
  write(process.env.FIXTURE_REQUESTS, "");

  const binaries = output("binaries");
  for (const name of ["graphit", "graphit-model-server"]) {
    write(path.join(binaries, name), "controlled executable payload\n");
    fs.chmodSync(path.join(binaries, name), 0o755);
  }
  receipt();
  // Installer checks consume an immutable baseline, independently of whether
  // the release-consumer assertion below was selected or ran first.
  const baseline = output("baseline-dist");
  okay(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, baseline]));

  check("release consumer includes both executables, runtime and valid checksums", () => {
    receipt();
    const dist = output("dist");
    okay(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, dist]));
    const release = path.join(dist, "graphit-linux-amd64.tar.gz");
    assert.equal(fs.readFileSync(release + ".sha256", "utf8").split("  ")[0], hash(release));
    const extracted = output("release");
    fs.mkdirSync(extracted);
    okay(run("tar", ["-xzf", release, "-C", extracted]));
    const root = path.join(extracted, "graphit-linux-amd64");
    assert.ok(fs.existsSync(path.join(root, "graphit-model-server")));
    for (const source of ["AGENT.md", "prompt.md"]) {
      assert.deepEqual(fs.readFileSync(path.join(root, "graphit-runtime/agents/coordinator", source)), fs.readFileSync(path.join(repo, "templates/agent-instructions/coordinator", source)));
    }
    assert.equal(fs.readFileSync(path.join(root, "graphit-runtime/licenses/cargo/lance/LICENSE"), "utf8"), "fixture official Lance notice\n");
    assert.ok(fs.existsSync(path.join(root, "graphit-runtime/licenses/crates/graphit-ast/src/antlr_engine/cpp/generated/GRAMMAR_NOTICES.md")));
    assert.ok(!fs.existsSync(path.join(root, "graphit-runtime/onnxruntime")));
    for (const notice of ggufNotices) {
      assert.deepEqual(fs.readFileSync(path.join(root, "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp", notice)), fs.readFileSync(path.join(ggufSource, "llama.cpp", notice)));
    }
    const build = JSON.parse(fs.readFileSync(path.join(root, "graphit-runtime/build.json")));
    assert.equal(build.cargo_lock_sha256, hash(path.join(project, "Cargo.lock")));
    assert.equal(build.native_manifest, undefined);
    assert.equal(build.native_accelerators, undefined);
    assert.deepEqual(build.native_sources, {
      "llama-cpp-sys-2": { version: "0.1.161", source: llamaSource },
      lance: { version: "14.0.2", source: registrySource }, lbug: { version: "0.22.1", source: registrySource },
      "tikv-jemallocator": { version: "0.7.0", source: registrySource },
      "tikv-jemalloc-sys": { version: jemallocSysVersion, source: registrySource },
    });
    for (const notice of allocatorNotices) {
      assert.deepEqual(fs.readFileSync(path.join(root, "graphit-runtime/licenses/cargo", notice)), fs.readFileSync(path.join(cargoSources, notice)));
    }
    assert.ok(!JSON.stringify(build).includes(temp));
    assert.ok(!fs.existsSync(path.join(project, "vendor")));
    const linux = JSON.parse(fs.readFileSync(path.join(root, "graphit-runtime/linux-libraries.json")));
    assert.deepEqual(linux.host_libraries, ["ld-linux-x86-64.so.2", "libc.so.6", "libdl.so.2", "libm.so.6", "libpthread.so.0", "librt.so.1"]);
    assert.deepEqual(linux.required_glibc_versions, ["2.34"]);
    assert.deepEqual(linux.libraries.map((item) => item.soname).sort(), [...linuxNames].sort());
    for (const item of linux.libraries) {
      assert.equal(hash(path.join(root, "graphit-runtime/lib", item.soname)), item.sha256);
      assert.equal(item.input_sha256, hash(path.join(linuxLibraries, item.soname)));
      assert.notEqual(item.input_sha256, item.sha256);
      assert.equal(item.runpath, "$ORIGIN");
    }
    for (const item of linux.executables) {
      assert.equal(item.input_sha256, hash(path.join(binaries, item.name)));
      assert.equal(item.sha256, hash(path.join(root, item.name)));
      assert.notEqual(item.input_sha256, item.sha256);
      assert.equal(item.runpath, "$ORIGIN/graphit-runtime/lib");
      assert.ok(!fs.readFileSync(path.join(binaries, item.name), "utf8").includes("RUNPATH="), "Cargo output remains untouched");
    }
    assert.ok(fs.existsSync(path.join(root, "graphit-runtime/licenses/linux/zlib1g/copyright")));
    assert.ok(fs.existsSync(path.join(root, "graphit-runtime/licenses/linux/common-licenses/GPL-3")));
    assert.ok(!JSON.stringify(linux).includes(temp));
    assert.equal(fs.readFileSync(path.join(root, "graphit-runtime/licenses/cargo/lbug/LICENSE"), "utf8"), "fixture official Ladybug notice\n");
    assert.ok(fs.existsSync(path.join(root, "graphit-runtime/licenses/cargo/lbug/lbug-src/third_party/test/LICENSE")));
    fs.unlinkSync(path.join(binaries, "graphit-model-server"));
    assert.notEqual(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, output("missing-bin")]).status, 0);
    assert.ok(!fs.existsSync(output("missing-bin")));
    write(path.join(binaries, "graphit-model-server"), "controlled executable payload\n");
    fs.chmodSync(path.join(binaries, "graphit-model-server"), 0o755);
    fs.unlinkSync(path.join(project, "crates/graphit-data/licenses/lance.LICENSE"));
    assert.notEqual(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, output("missing-native-notice")]).status, 0);
    assert.ok(!fs.existsSync(output("missing-native-notice")));
    write(path.join(project, "crates/graphit-data/licenses/lance.LICENSE"), "fixture official Lance notice\n");
  });

  for (const notice of allocatorNotices) {
    check("Linux jemalloc missing notice " + notice + " fails before publication", () => {
      const file = path.join(cargoSources, notice);
      const original = fs.readFileSync(file);
      fs.unlinkSync(file);
      try {
        const destination = output("missing-allocator-" + notice.replaceAll("/", "-"));
        const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]);
        assert.notEqual(result.status, 0);
        assert.match(result.stderr, /ENOENT/);
        assert.ok(!fs.existsSync(destination));
      } finally { write(file, original); }
    });
  }

  for (const major of ["12", "13"]) check("Cargo shared engine and CUDA " + major + " runtime use selected build output and relative loader paths", () => {
    const engine = output("selected-engine");
    const toolkit = output("selected-toolkit-" + major);
    const outside = output("unselected-engine");
    const engineNames = ["libllama.so", "libggml.so", "libggml-base.so", "libggml-cpu.so", "libggml-cuda.so"];
    const cudaNames = ["libcudart.so.", "libcublas.so.", "libcublasLt.so."].map(name => name + major);
    for (const root of [engine, outside]) for (const name of engineNames) write(path.join(root, "lib", name), "native engine: " + name);
    for (const name of cudaNames) write(path.join(toolkit, "lib64", name), "NVIDIA runtime: " + name);
    const env = { FIXTURE_NATIVE_ENGINE: engine, FIXTURE_NATIVE_OUTSIDE: outside, CUDA_PATH: toolkit, FIXTURE_CUDA_MAJOR: major };
    const select = () => receipt((messages) => messages.splice(-1, 0, {
      reason: "build-script-executed", package_id: "resolved-llama", out_dir: engine, linked_libs: ["dylib=ggml-cuda"],
    }));
    select();
    const dist = output("dynamic-dist-" + major);
    okay(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, dist], env));
    const extract = output("dynamic-release-" + major);
    fs.mkdirSync(extract);
    okay(run("tar", ["-xzf", path.join(dist, "graphit-linux-amd64.tar.gz"), "-C", extract]));
    const root = path.join(extract, "graphit-linux-amd64/graphit-runtime");
    const manifest = JSON.parse(fs.readFileSync(path.join(root, "linux-libraries.json")));
    for (const name of [...engineNames, ...cudaNames]) {
      const entry = manifest.libraries.find((entry) => entry.soname === name);
      assert.equal(entry.origin.kind, engineNames.includes(name) ? "cargo" : "nvidia");
      assert.equal(entry.runpath, "$ORIGIN");
      assert.equal(hash(path.join(root, "lib", name)), entry.sha256);
      if (entry.origin.kind === "cargo") assert.equal(entry.origin.source, llamaSource);
    }
    assert.ok(!JSON.stringify(manifest).includes(temp));
    const wrong = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, output("outside-engine-dist")], { ...env, FIXTURE_LINUX_ISSUE: "outside-engine" });
    assert.notEqual(wrong.status, 0);
    assert.match(wrong.stderr, /outside its selected Cargo/);
    receipt((messages) => messages.splice(-1, 0, ...[1, 2].map(() => ({ reason: "build-script-executed", package_id: "resolved-llama", out_dir: engine }))));
    const ambiguous = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, output("ambiguous-engine-dist")], env);
    assert.notEqual(ambiguous.status, 0);
    assert.match(ambiguous.stderr, /ambiguous Cargo engine/);
    receipt();
  });

  for (const issue of ["missing", "empty"]) {
    check("Cargo llama notice " + issue + " rejects incomplete sources", () => {
      const notice = path.join(ggufSource, "llama.cpp/LICENSE");
      const original = fs.readFileSync(notice);
      try {
        if (issue === "missing") fs.unlinkSync(notice); else write(notice, "");
        const destination = output("llama-notice-" + issue);
        const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]);
        assert.notEqual(result.status, 0);
        assert.ok(!fs.existsSync(destination));
      } finally { write(notice, original); }
    });
  }

  for (const issue of ["patch-tool", "missing-library", "unknown-library", "openblas", "soname", "missing-license", "outside-package"]) {
    check("Linux " + issue + " fails before archive publication", () => {
      const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
      const destination = output("linux-rejected-" + issue);
      const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination], { FIXTURE_LINUX_ISSUE: issue });
      assert.notEqual(result.status, 0);
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
      assert.ok(!fs.existsSync(destination));
    });
  }

  check("empty Cargo executable fails before archive publication", () => {
    const binary = path.join(output("binaries"), "graphit");
    const original = fs.readFileSync(binary);
    try {
      write(binary, "");
      const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
      const destination = output("empty-executable");
      assert.notEqual(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]).status, 0);
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
      assert.ok(!fs.existsSync(destination));
    } finally { write(binary, original); }
  });

  for (const [label, mutate] of [
    ["Debug", (messages) => { messages[0].profile.opt_level = "0"; messages[0].profile.debug_assertions = true; }],
    ["test", (messages) => { messages[0].profile.test = true; }],
    ["missing", (messages) => { messages.splice(0, 1); }],
    ["ambiguous", (messages) => { messages.push(messages[0]); }],
    ["path mismatch", (messages) => { messages[0].executable = path.join(output("binaries"), "graphit-model-server"); }],
    ["unfinished build", (messages) => { messages.at(-1).success = false; }],
  ]) {
    check(`Cargo ${label} executable fails before archive publication`, () => {
      receipt(mutate);
      try {
        const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
        const destination = output("rejected-artifact-" + label.replaceAll(" ", "-"));
        const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]);
        assert.notEqual(result.status, 0);
        assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
        assert.ok(!fs.existsSync(destination));
      } finally { receipt(); }
    });
  }

  check("Ladybug fork sources cannot replace the official registry package", () => {
    withMetadata((value) => { value.packages[1].source = "git+https://github.com/graphit-labs/graphit-libs?rev=" + "a".repeat(40); }, () => {
      const destination = output("rejected-lbug-fork");
      const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]);
      assert.notEqual(result.status, 0);
      assert.ok(!fs.existsSync(destination));
    });
  });

  for (const issue of ["missing-resolve", "unresolved-package", "ambiguous-package", "wrong-version", "empty-source", "local-git-source", "fork-source", "path-source"]) {
    check("Cargo " + issue + " fails before archive publication", () => {
      const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
      withMetadata((value) => {
        if (issue === "missing-resolve") value.resolve = null;
        if (issue === "unresolved-package") value.resolve.nodes = value.resolve.nodes.filter((node) => node.id !== "resolved-lance");
        if (issue === "ambiguous-package") value.resolve.nodes.push({ id: "unresolved-upstream-lance" });
        if (issue === "wrong-version") value.packages[0].version = "14.0.1";
        if (issue === "fork-source") value.packages[0].source = "git+https://github.com/graphit-labs/graphit-libs#" + "a".repeat(40);
        if (issue === "path-source") value.packages[0].source = null;
        if (issue === "empty-source") value.packages[0].source = "";
        if (issue === "local-git-source") value.packages[0].source = "git+file:" + cargoSources + "#" + "a".repeat(40);
      }, () => {
        const destination = output(issue);
        assert.notEqual(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]).status, 0);
        assert.ok(!fs.existsSync(destination));
      });
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
    });
  }

  check("Cargo metadata failure cannot fetch dependencies or fall back to source discovery", () => {
    const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
    const destination = output("cargo-unavailable");
    const result = run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination], { FIXTURE_CARGO_FAIL: "1" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /offline Cargo metadata unavailable/);
    assert.ok(!fs.existsSync(destination));
    assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
  });

  check("missing Ladybug license rejects incomplete sources", () => {
    const provenance = path.join(project, "crates/graphit-ast/licenses/ladybug.LICENSE");
    const contents = fs.readFileSync(provenance);
    const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
    fs.unlinkSync(provenance);
    try {
      const destination = output("missing-ladybug-license");
      assert.notEqual(run("bash", [path.join(project, "scripts/package-release.sh"), "linux-amd64", artifactsReceipt, destination]).status, 0);
      assert.ok(!fs.existsSync(destination));
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
    } finally { write(provenance, contents); }
  });

  check("Unix product installer preserves the complete namespaced release layout", () => {
    const release = path.join(baseline, "graphit-linux-amd64.tar.gz");
    const destination = output("user-bin");
    fs.mkdirSync(destination);
    write(path.join(destination, "LICENSE"), "unrelated installed product license\n");
    const result = run("sh", [path.join(project, "install.sh"), "--dir", destination, "--version", "v9.9.9"], {
      FIXTURE_RELEASE: release,
      FIXTURE_RELEASE_SHA: release + ".sha256",
    });
    okay(result);
    for (const name of ["graphit", "graphit-model-server", "graphit-runtime/LICENSE", "graphit-runtime/agents/coordinator/AGENT.md", "graphit-runtime/agents/coordinator/prompt.md", "graphit-runtime/licenses/cargo/lance/LICENSE", "graphit-runtime/linux-libraries.json", "graphit-runtime/lib/libssl.so.3", "graphit-runtime/licenses/linux/zlib1g/copyright"]) {
      assert.ok(fs.existsSync(path.join(destination, name)), name);
    }
    for (const notice of ggufNotices) {
      assert.deepEqual(fs.readFileSync(path.join(destination, "graphit-runtime/licenses/cargo/llama-cpp-sys-2/llama.cpp", notice)), fs.readFileSync(path.join(ggufSource, "llama.cpp", notice)));
    }
    assert.equal(fs.readFileSync(path.join(destination, "LICENSE"), "utf8"), "unrelated installed product license\n");
    assert.match(result.stdout, /Installed graphit and graphit-model-server/);
    assert.ok(!fs.existsSync(path.join(destination, "graphit-runtime/onnxruntime")));
  });

  check("Unix product installer rejects a bad release digest before modifying the destination", () => {
    const release = path.join(baseline, "graphit-linux-amd64.tar.gz");
    const badDigest = output("bad-release-sha");
    write(badDigest, "0".repeat(64) + "  graphit-linux-amd64.tar.gz\n");
    const destination = output("bad-user-install");
    const result = run("sh", [path.join(project, "install.sh"), "--dir", destination], {
      FIXTURE_RELEASE: release,
      FIXTURE_RELEASE_SHA: badDigest,
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /checksum mismatch/);
    assert.ok(!fs.existsSync(destination));
  });

  check("Unix local installer verifies the complete package offline and replaces active files by rename", () => {
    const release = path.join(baseline, "graphit-linux-amd64.tar.gz");
    const destination = output("local-user-bin");
    fs.mkdirSync(destination);
    const executable = path.join(destination, "graphit");
    fs.copyFileSync(process.execPath, executable);
    fs.chmodSync(executable, 0o755);
    const previousExecutable = fs.statSync(executable).ino;
    const library = path.join(destination, "graphit-runtime/lib/libssl.so.3");
    write(library, "previous mapped library\n");
    write(path.join(destination, "graphit-runtime/custom.txt"), "user content\n");
    const descriptor = fs.openSync(library, "r");
    const previousLibrary = fs.fstatSync(descriptor).ino;
    const active = spawn(executable, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
    try {
      assert.ok(active.pid);
      const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
      okay(run("sh", [path.join(project, "install.sh"), "--archive", release, "--dir", destination], { VERSION: "" }));
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
      assert.notEqual(fs.statSync(executable).ino, previousExecutable);
      assert.notEqual(fs.statSync(library).ino, previousLibrary);
      assert.equal(fs.readFileSync(descriptor, "utf8"), "previous mapped library\n");
      assert.notEqual(fs.readFileSync(library, "utf8"), "previous mapped library\n");
      assert.equal(fs.readFileSync(path.join(destination, "graphit-runtime/custom.txt"), "utf8"), "user content\n");
      process.kill(active.pid, 0);
      assert.ok(fs.existsSync(path.join(destination, "graphit-runtime/linux-libraries.json")));
      assert.ok(fs.existsSync(path.join(destination, "graphit-model-server")));
    } finally {
      fs.closeSync(descriptor);
      active.kill();
    }
  });

  check("Unix local installer rejects checksum errors and ambiguous selection without network or effects", () => {
    const release = path.join(baseline, "graphit-linux-amd64.tar.gz");
    const badRelease = output("local-bad-package.tar.gz");
    fs.copyFileSync(release, badRelease);
    write(badRelease + ".sha256", "0".repeat(64) + "  graphit-linux-amd64.tar.gz\n");
    const destination = output("local-rejected-bin");
    const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
    for (const args of [["--archive", badRelease], ["--archive", release, "--version", "v1"], ["--archive"]]) {
      const result = run("sh", [path.join(project, "install.sh"), "--dir", destination, ...args], { VERSION: "" });
      assert.notEqual(result.status, 0);
      assert.ok(!fs.existsSync(destination));
      assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
    }
  });

  check("Make install consumes the complete Release package rather than development executables", () => {
    const result = spawnSync("make", ["-n", "install"], { cwd: repo, encoding: "utf8" });
    okay(result);
    assert.match(result.stdout, /build --locked --release/);
    assert.match(result.stdout, /package-release\.sh/);
    assert.match(result.stdout, /install\.sh --archive/);
    assert.doesNotMatch(result.stdout, /debug\/graphit/);
    const windows = spawnSync("make", ["-n", "install", "PLATFORM=windows-amd64"], { cwd: repo, encoding: "utf8" });
    okay(windows);
    assert.match(windows.stdout, /install\.ps1 -Archive/);
    assert.doesNotMatch(windows.stdout, /sh install\.sh/);
  });

  if (spawnSync("pwsh", ["-NoProfile", "-Command", "$PSVersionTable.PSVersion.Major"], { encoding: "utf8" }).status === 0) {
    check("PowerShell local installer requires the Windows DLL manifest and its complete payload offline", () => {
      const fixture = output("windows-installer");
      const root = path.join(fixture, "graphit-windows-amd64");
      const destination = output("windows-user-bin");
      for (const name of ["graphit.exe", "graphit-model-server.exe", "graphit-runtime/LICENSE", "graphit-runtime/build.json", "vcruntime140.dll"]) {
        write(path.join(root, name), "controlled Windows package file\n");
      }
      const manifest = path.join(root, "graphit-runtime/windows-libraries.json");
      const value = { platform: "windows-amd64", executables: [{ file: "graphit.exe" }, { file: "graphit-model-server.exe" }], libraries: [{ name: "vcruntime140.dll", file: "vcruntime140.dll" }] };
      const archive = path.join(fixture, "graphit-windows-amd64.tar.gz");
      const wrapper = output("windows-install-probe.ps1");
      write(wrapper, `param($Installer, $Archive, $Destination)
$ErrorActionPreference = 'Stop'
function Invoke-WebRequest { throw 'Offline installation must not access the network' }
& $Installer -Archive $Archive -Dir $Destination
`);
      const install = () => {
        okay(run("tar", ["-czf", archive, "-C", fixture, "graphit-windows-amd64"]));
        write(archive + ".sha256", hash(archive) + "  graphit-windows-amd64.tar.gz\n");
        return run("pwsh", ["-NoProfile", "-File", wrapper, "-Installer", path.join(repo, "install.ps1"), "-Archive", archive, "-Destination", destination], { PROCESSOR_ARCHITECTURE: "AMD64", VERSION: "" });
      };
      // Invalid packages must fail before the installation directory is created.
      assert.notEqual(install().status, 0);
      assert.ok(!fs.existsSync(destination));
      write(manifest, JSON.stringify({ ...value, platform: "darwin-arm64" }));
      assert.notEqual(install().status, 0);
      assert.ok(!fs.existsSync(destination));
      write(manifest, JSON.stringify(value));
      fs.unlinkSync(path.join(root, "vcruntime140.dll"));
      assert.notEqual(install().status, 0);
      assert.ok(!fs.existsSync(destination));
      write(path.join(root, "vcruntime140.dll"), "controlled DLL\n");
      okay(install());
      assert.equal(fs.readFileSync(path.join(destination, "vcruntime140.dll"), "utf8"), "controlled DLL\n");
      assert.ok(fs.existsSync(path.join(destination, "graphit-runtime/windows-libraries.json")));
    });
  }

  check("macOS installer rejects a package without runtime metadata before touching its destination", () => {
    const fixture = output("macos-installer");
    const root = path.join(fixture, "graphit-darwin-arm64");
    const destination = output("macos-user-bin");
    for (const name of ["graphit", "graphit-model-server", "graphit-runtime/LICENSE", "graphit-runtime/build.json"]) {
      write(path.join(root, name), "controlled macOS package file\n");
    }
    const archive = path.join(fixture, "graphit-darwin-arm64.tar.gz");
    okay(run("tar", ["-czf", archive, "-C", fixture, "graphit-darwin-arm64"]));
    write(archive + ".sha256", hash(archive) + "  graphit-darwin-arm64.tar.gz\n");
    const uname = path.join(bin, "uname");
    write(uname, '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; *) exit 1;; esac\n');
    fs.chmodSync(uname, 0o755);
    try {
      const result = run("sh", [path.join(project, "install.sh"), "--archive", archive, "--dir", destination], { VERSION: "" });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /missing macOS runtime metadata/);
      assert.ok(!fs.existsSync(destination));
    } finally { fs.unlinkSync(uname); }
  });

  check("Unix installer rejects a missing directory argument before network IO", () => {
    const requests = fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8");
    const result = run("sh", [path.join(project, "install.sh"), "--dir"]);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /requires a path/);
    assert.equal(fs.readFileSync(process.env.FIXTURE_REQUESTS, "utf8"), requests);
  });

  assert.ok(passed > 0, "no native packaging checks selected");
  console.log(passed + " native packaging checks passed; controlled payloads are not native ABI evidence.");
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
