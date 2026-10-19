#!/usr/bin/env python3
"""Exercise an existing Linux Graphit executable in a disposable installation.

This is a product journey, not a substitute for native platform qualification.
The default stage deliberately disables inference/Dream; its result lists that
remaining coverage rather than treating Raw/Graph publication as model proof.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def native(path):
    value = str(path)
    value.encode("utf-8", errors="strict")
    return value


class Journey:
    def __init__(self, executable, root, versions=False, model_server=None, ort_library=None):
        self.executable = executable
        self.root = root
        self.installation = root / "installation"
        self.outside = root / "outside"
        self.outside.mkdir()
        runtime = root / "runtime"
        runtime.mkdir()
        self.environment = {
            "PATH": os.environ.get("PATH", ""), "HOME": str(root),
            "XDG_RUNTIME_DIR": str(runtime), "XDG_CONFIG_HOME": str(runtime),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(runtime / "absent-bus"),
        }
        self.daemon = None
        self.proxies = []
        self.observations = []
        self.versions = versions
        self.model_server = model_server
        self.ort_library = ort_library
        self.model_pid = None
        if model_server:
            self.environment["PATH"] = str(model_server.parent) + os.pathsep + self.environment["PATH"]

    def configure_model(self):
        # The exact small ONNX/tokenizer assets and manifest contract used by
        # graphit-daemon/tests/support/model_fixture.rs; no download or mock SDK.
        fixture = Path(__file__).resolve().parents[1] / "crates/graphit-ai/tests/fixtures/local_runtime"
        bundle = self.installation / "models/tiny-cpu"
        bundle.mkdir(parents=True)
        shutil.copyfile(fixture / "embedding_rank2.onnx", bundle / "model.onnx")
        shutil.copyfile(fixture / "tokenizer.json", bundle / "tokenizer.json")
        manifest = {
            "schema": "graphit-model/v1", "id": "embedding_rank2", "revision": "fixture-1",
            "task": "embedding", "runtime": "onnx", "artifacts": [],
            "model_role": "model", "tokenizer_role": "tokenizer", "external_data_roles": [],
            "inputs": {"input_ids": "input_ids", "attention_mask": "attention_mask"},
            "output": "sentence_embedding",
            "text": {"max_tokens": 8, "truncation": "longest_first", "padding": "longest",
                     "query_prefix": "", "document_prefix": ""},
            "inference": {"dimensions": 4, "pooling": "none", "normalize": False},
        }
        for role, filename, format_name in (("model", "model.onnx", "onnx"),
                                            ("tokenizer", "tokenizer.json", "huggingface_json")):
            manifest["artifacts"].append({
                "role": role, "path": filename,
                "digest": list(hashlib.sha256((bundle / filename).read_bytes()).digest()),
                "minimum_bytes": 1, "required": True, "format": format_name})
        (bundle / "model.json").write_text(json.dumps(manifest))
        settings = {"slots": [{
            "name": "embedding", "selection": {
                "id": "embedding_rank2", "revision": "fixture-1", "task": "embedding"},
            "bundle": str(bundle), "manifest_relative": "model.json",
            "runtime": {"backend": "onnx", "native_library": str(self.ort_library),
                        "provider": "cpu", "intra_threads": 1, "inter_threads": 1,
                        "padding": None}}]}
        self.command("config", "set", "--global", "daemon.models", json.dumps(settings))
        self.command("provider", "add", "cpu", "--type", "local", "--embedding-mode", "local",
                     "--embedding-device", "cpu", "--rerank-mode", "disabled", "--deny-aws-credential-chain")
        self.command("login", "--provider", "cpu", "--profile", "local", "--username", "cab0 fixture",
                     "--mcp-key", secrets.token_hex(32), "--non-interactive")
        self.command("account", "use", "local")

    def shared_model(self):
        result = subprocess.run(["ps", "--ppid", str(self.daemon.pid), "-o", "pid="],
                                capture_output=True, text=True, timeout=10)
        pids = []
        for item in result.stdout.split():
            pid = int(item)
            try:
                executable = Path(os.readlink(f"/proc/{pid}/exe"))
            except FileNotFoundError:
                continue
            if executable.name == "graphit-model-server":
                pids.append(pid)
        assert len(pids) == 1, "two PROJECTs must use one real model child"
        if self.model_pid is None:
            self.model_pid = pids[0]
        assert pids == [self.model_pid], "unrelated PROJECT work must retain the shared model child"

    def command(self, *arguments, cwd=None, ok=True):
        result = subprocess.run(
            [self.executable, "--global-dir", str(self.installation), *arguments],
            cwd=cwd or self.outside, env=self.environment,
            capture_output=True, text=True, timeout=30,
        )
        if ok and result.returncode:
            raise AssertionError(f"{arguments[:2]} failed: {result.stderr}")
        return result

    def observe(self, name, **values):
        self.observations.append({"name": name, **values})
        print(json.dumps(self.observations[-1]), flush=True)

    def ready(self, operation, predicate, label):
        def report_failure(name, result):
            try:
                last = json.loads(result.stdout)
            except ValueError:
                last = {}
            if not isinstance(last, dict):
                last = {}
            selection = last.get("selection") or {}
            result_data = last.get("native") or {}
            if not isinstance(selection, dict):
                selection = {}
            if not isinstance(result_data, dict):
                result_data = {}
            generation = result_data.get("generation") or {}
            if not isinstance(generation, dict):
                generation = {}
            self.observe(name, operation=label,
                         returncode=result.returncode,
                         project_id=selection.get("project_id"),
                         project_version=selection.get("project_version"),
                         state=result_data.get("state"), total=result_data.get("total"),
                         generation=generation.get("version"),
                         physical_branch=generation.get("physical_branch"),
                         inherited_target=generation.get("inherited_target"),
                         error_code=last.get("code"))

        until = time.monotonic() + 60
        while True:
            result = operation()
            try:
                matched = result.returncode == 0 and predicate(result.stdout)
            except (KeyError, TypeError, ValueError):
                report_failure("readiness_invalid_response", result)
                raise
            if matched:
                return result.stdout
            if self.daemon.poll() is not None:
                report_failure("readiness_daemon_exit", result)
                raise AssertionError("owned daemon exited: " + self.log.read_text())
            if time.monotonic() >= until:
                report_failure("readiness_deadline", result)
                raise AssertionError(f"{label} not ready; see finite readiness_deadline observation")
            time.sleep(0.1)

    def functions(self, project, *expected, version=None):
        args = ["ast", "query", "MATCH (n:Function) RETURN n.name ORDER BY n.name",
                "--project-dir", str(project)]
        if version:
            args += ["--project-version", version]
        def names(text):
            return [row[0]["value"] for row in json.loads(text)["rows"]]
        value = self.ready(lambda: self.command(*args, ok=False),
                           lambda text: names(text) == sorted(expected), "graph functions")
        return names(value)

    def knowledge(self, project, term, version=None, mode="fts"):
        args = ["knowledge", "search", term, "--mode", mode, "--project-dir", str(project)]
        if version:
            args += ["--project-version", version]
        def found(text):
            native = json.loads(text)["native"]
            # Raw/Graph can be ready before Knowledge's first publication.
            # NoCorpus has no total; neither absence nor an empty result is success.
            if native["state"] == "no_corpus":
                return False
            return native["state"] == "hits" and native["total"] == 1
        return json.loads(self.ready(
            lambda: self.command(*args, ok=False),
            found,
            "Knowledge document"))

    def start_proxy(self, project):
        proxy = subprocess.Popen(
            [self.executable, "--global-dir", str(self.installation), "mcp", "--stdio",
             "--non-interactive", "--project-dir", str(project)],
            cwd=self.outside, env=self.environment,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            bufsize=0,
        )
        self.proxies.append(proxy)
        self.rpc(proxy, 1, "initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "graphit-cab0", "version": "1"}})
        proxy.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proxy.stdin.flush()
        return proxy

    def rpc(self, proxy, request_id, method, params):
        proxy.stdin.write((json.dumps({"jsonrpc": "2.0", "id": request_id,
                                      "method": method, "params": params}) + "\n").encode())
        proxy.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(proxy.stdout, selectors.EVENT_READ)
            until = time.monotonic() + 30
            while time.monotonic() < until:
                if not selector.select(max(0, until - time.monotonic())):
                    break
                line = proxy.stdout.readline()
                if not line:
                    raise AssertionError("MCP proxy ended before its response")
                message = json.loads(line)
                if message.get("id") == request_id:
                    assert "error" not in message, message
                    return message["result"]
        raise AssertionError("MCP response deadline")

    def source(self, proxy, request_id, project, expected):
        result = self.rpc(proxy, request_id, "tools/call", {
            "name": "graphit_ast_source",
            "arguments": {"project_dir": str(project), "path": "code.rs"}})
        assert not result.get("isError", False), result
        assert "".join(part.get("text", "") for part in result["content"]) == expected

    def close_proxy(self, proxy):
        proxy.stdin.close()
        assert proxy.wait(timeout=15) == 0, "MCP EOF must join without stopping the daemon"
        proxy.stdout.close()
        proxy.stderr.close()
        self.proxies.remove(proxy)

    def run(self):
        self.command("setup", "--agent", "codex", "--non-interactive")
        for module in ("embedding", "memory", "dream"):
            self.command("config", "set", "--global", "modules." + module, "false")
        if self.model_server:
            self.configure_model()
            self.command("config", "set", "--global", "modules.embedding", "true")
        self.command("config", "set", "--global", "modules.ast", "true")
        self.command("config", "set", "--global", "modules.knowledge", "true")
        self.log = self.root / "daemon.stderr"
        with self.log.open("wb") as log:
            self.log.chmod(0o600)
            self.daemon = subprocess.Popen(
                [self.executable, "--global-dir", str(self.installation), "daemon", "--managed"],
                cwd=self.outside, env=self.environment, stdout=subprocess.DEVNULL, stderr=log)
        discovery = self.installation / "daemon/mcp.json"
        until = time.monotonic() + 30
        while not discovery.exists():
            assert self.daemon.poll() is None, self.log.read_text()
            assert time.monotonic() < until, "discovery absent"
            time.sleep(0.05)
        address = json.loads(discovery.read_text())["address"]
        assert json.loads(discovery.read_text())["pid"] == self.daemon.pid
        projects, identities = {}, {}
        for name in ("a", "b"):
            project = self.root / name
            project.mkdir()
            (project / "code.rs").write_text(f"pub fn {name}_initial() {{}}\n")
            (project / "docs").mkdir()
            (project / "docs/intro.md").write_text(
                f"---\ntitle: {name} document\n---\n# {name}\n{name}initialmarker\n")
            configured = json.loads(self.command(
                "setup", "--agent", "codex", "--project-dir", str(project),
                "--name", "cab0-" + name, "--non-interactive").stdout)
            projects[name] = project
            identities[name] = configured["project"]["identity"]["id"]
        assert identities["a"] != identities["b"]
        for name, project in projects.items():
            self.functions(project, name + "_initial")
            result = self.knowledge(project, name + "initialmarker")
            assert result["selection"]["project_id"] == identities[name]
        self.observe("two_project_initial_graph_and_knowledge", independent_identities=True)
        if self.model_server:
            for name, project in projects.items():
                semantic = self.knowledge(project, name + "initialmarker", mode="semantic")
                assert semantic["selection"]["project_id"] == identities[name]
            self.shared_model()
            self.observe("two_projects_semantic_Knowledge_share_real_CPU_model_child")
        a, b = projects["a"], projects["b"]
        a_changed = "pub fn a_changed() {}\n"
        (a / "code.rs").write_text(a_changed)
        (a / "docs/intro.md").write_text("# A changed\naupdatedmarker\n")
        self.functions(a, "a_changed")
        self.functions(b, "b_initial")
        changed = self.knowledge(a, "aupdatedmarker")
        assert changed["native"]["generation"]["version"] > 1
        self.knowledge(b, "binitialmarker")
        self.observe("incremental_A_preserves_B", knowledge_generation_advanced=True)
        proxy = self.start_proxy(a)
        self.source(proxy, 2, a, a_changed)
        self.source(proxy, 3, b, "pub fn b_initial() {}\n")
        self.close_proxy(proxy)
        proxy = self.start_proxy(b)
        self.source(proxy, 2, b, "pub fn b_initial() {}\n")
        self.close_proxy(proxy)
        self.observe("mcp_selection_reconnect_and_eof", daemon_alive=self.daemon.poll() is None)
        denied = urllib.request.Request(
            "http://" + address + "/api/graphs/project/query",
            data=json.dumps({"selection": {"project_dir": native(b)},
                             "query": {"query": "MATCH (n:Function) RETURN n.name"}}).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer invalid",
                     "x-graphit-auth-origin": "runtime"})
        try:
            urllib.request.urlopen(denied, timeout=10)
            raise AssertionError("invalid authentication admitted PROJECT read")
        except urllib.error.HTTPError as error:
            assert error.code in (401, 403), error.code
            assert b"b_initial" not in error.read()
        self.functions(b, "b_initial")
        self.observe("invalid_auth_rejected_without_project_content")
        direct_a = urllib.request.Request(
            "http://" + address + "/api/graphs/project/query",
            data=json.dumps({"selection": {"project_dir": native(a)},
                             "query": {"query": "MATCH (n:Function) RETURN n.name"}}).encode(),
            headers={"Content-Type": "application/json",
                     "Authorization": "Bearer " + (self.installation / "daemon/mcp.key").read_text(),
                     "x-graphit-auth-origin": "runtime"})
        with urllib.request.urlopen(direct_a, timeout=10) as response:
            assert json.load(response)["rows"][0][0]["value"] == "a_changed"
        retired = json.loads(self.command(
            "projects", "retire-checkout", "--project-id", identities["a"],
            "--project-dir", str(a)).stdout)
        assert retired["checkout_removed"] is True
        (a / "code.rs").write_text("pub fn a_retired_must_not_run() {}\n")
        (b / "code.rs").write_text("pub fn b_survivor() {}\n")
        (b / "docs/intro.md").write_text("# B survivor\nbsurvivormarker\n")
        self.functions(b, "b_survivor")
        self.knowledge(b, "bsurvivormarker")
        if self.model_server:
            self.knowledge(b, "bsurvivormarker", mode="semantic")
            self.shared_model()
        # The Root CLI deliberately maintains its PROJECT before reading. Use
        # the captured transport directly to observe retirement without resuming A.
        try:
            urllib.request.urlopen(direct_a, timeout=10)
            raise AssertionError("retired checkout still admits direct requests")
        except urllib.error.HTTPError as error:
            # GraphServer::select_project rejects absent live selections as
            # BadInput, rendered as this finite HTTP400 by ReadFailure.
            assert error.code == 400, error.code
            assert error.read() == b"invalid graph request"
        self.observe("retired_A_denies_new_reads_B_keeps_incremental_work")
        resumed = json.loads(self.command(
            "projects", "maintain-checkout", "--project-id", identities["a"],
            "--project-dir", str(a), "--select").stdout)
        assert resumed["inventory_registered"] is True
        self.functions(a, "a_retired_must_not_run")
        self.functions(b, "b_survivor")
        self.observe("resume_A_reacquires_fresh_source_and_preserves_B")
        if self.model_server:
            self.knowledge(a, "aupdatedmarker", mode="semantic")
            self.shared_model()
            self.observe("retire_and_resume_A_preserve_B_shared_model_child")
        if self.versions:
            # Commits belong only to this disposable fixture. They establish
            # actual checkout versions; no project repository is modified.
            def git(*arguments):
                subprocess.run(["git", "-C", str(a), *arguments], env=self.environment,
                               capture_output=True, check=True, timeout=15)
            git("init", "-b", "main")
            git("add", "code.rs", "docs/intro.md")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-m", "Main fixture source")
            git("checkout", "-b", "feature/cab0")
            (a / "code.rs").write_text("pub fn a_feature() {}\n")
            (a / "docs/intro.md").write_text("# Feature A\nafeaturemarker\n")
            git("add", "code.rs", "docs/intro.md")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-m", "Feature fixture source")
            self.command("projects", "maintain-checkout", "--project-id", identities["a"],
                         "--project-dir", str(a), "--select")
            self.functions(a, "a_feature", version="feature/cab0")
            self.observe("feature_graph_ready", project_id=identities["a"],
                         project_version="feature/cab0", functions=["a_feature"],
                         physical_source_matches=(a / "code.rs").read_text() == "pub fn a_feature() {}\n",
                         physical_document_matches=(a / "docs/intro.md").read_text() == "# Feature A\nafeaturemarker\n")
            feature = self.knowledge(a, "afeaturemarker", version="feature/cab0")
            assert feature["selection"]["project_version"] == "feature/cab0"
            self.functions(b, "b_survivor")
            git("checkout", "main")
            self.command("projects", "maintain-checkout", "--project-id", identities["a"],
                         "--project-dir", str(a), "--select")
            self.functions(a, "a_retired_must_not_run", version="main")
            self.knowledge(a, "aupdatedmarker", version="main")
            self.functions(b, "b_survivor")
            self.observe("real_git_feature_and_main_switch_keep_B_and_scope_Knowledge")
        self.daemon.send_signal(signal.SIGTERM)
        assert self.daemon.wait(timeout=30) == 0, self.log.read_text()
        assert not discovery.exists()
        assert not (self.installation / "daemon/mcp.key").exists()
        if self.model_server:
            assert not Path(f"/proc/{self.model_pid}").exists(), "owned model child survived daemon join"
        try:
            urllib.request.urlopen("http://" + address + "/mcp", timeout=2)
            raise AssertionError("owned listener survived shutdown")
        except urllib.error.HTTPError as error:
            raise AssertionError(f"owned listener still answered HTTP{error.code}") from error
        except urllib.error.URLError as error:
            assert isinstance(error.reason, ConnectionRefusedError), error.reason
        self.observe("joined_shutdown_removes_owned_discovery_key_and_listener")

    def cleanup(self):
        errors = []
        try:
            for proxy in list(self.proxies):
                try:
                    proxy.stdin.close()
                    proxy.wait(timeout=10)
                except Exception as first:
                    try:
                        proxy.terminate()
                        proxy.wait(timeout=10)
                    except Exception as second:
                        errors.append(ExceptionGroup("MCP proxy cleanup", [first, second]))
        finally:
            if self.daemon is not None and self.daemon.poll() is None:
                try:
                    self.daemon.send_signal(signal.SIGTERM)
                    self.daemon.wait(timeout=30)
                except Exception as error:
                    errors.append(error)
        if errors:
            raise ExceptionGroup("owned process cleanup failed", errors)



def run_memory_dream_stage(args):
    """Use the existing Rust fixture for the SDK-only global Agent preparation."""
    selector = "foreground_source::dream_tests::foreground_dream_two_projects_memory_cli_cancel_and_join"
    started = time.monotonic()
    report = {"task": "tsk-cab0", "platform": "linux", "compilation_ms": 0,
              "coverage": "native_daemon_project_memory_cli_dream_isolation_cancel_join",
              "status": "failed", "owned_processes_joined": False,
              "remaining": ["memory_semantic_and_profile_generation_borrowers",
                            "dream_captured_version_switch", "integrated_browser_and_tray",
                            "native_windows_macos"],
              "proof_limits": ["Agent setup uses the existing qualified Hub SDK fixture; this does not qualify a global Agent CLI installation.",
                               "The new focal uses the compiled Memory CLI and actual daemon; existing MCP and CPU-model producer proofs are reused separately.",
                               "This functional subset does not qualify all cab0 journeys."]}
    with tempfile.TemporaryDirectory(prefix="graphit-cab0-records-") as temporary:
        root = Path(temporary)
        runtime = root / "runtime"
        runtime.mkdir()
        native_report = root / "native-records-result.json"
        environment = {"PATH": str(Path(sys.executable).parent) + os.pathsep + os.environ.get("PATH", ""),
                       "HOME": temporary, "TMPDIR": temporary, "XDG_RUNTIME_DIR": str(runtime),
                       "XDG_CONFIG_HOME": str(runtime),
                       "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(runtime / "absent-bus"),
                       "GRAPHIT_TEST_APP": str(Path(args.graphit).resolve(strict=True)),
                       "GRAPHIT_CAB0_RECORDS_RESULT": str(native_report)}
        command = [str(args.app_tests), selector, "--ignored", "--exact", "--test-threads=1"]
        process = subprocess.Popen(command, env=environment, cwd=root,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        try:
            stdout, stderr = process.communicate(timeout=240)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate(timeout=15)
            report["failure"] = "native_records_focal_deadline"
        else:
            report["exit_code"] = process.returncode
            if process.returncode:
                report["failure"] = "native_records_focal_failed"
            elif not native_report.is_file():
                report["failure"] = "native_records_focal_receipt_absent"
            elif b"test result: ok. 1 passed; 0 failed; 0 ignored;" not in stdout:
                report["failure"] = "native_records_focal_not_executed_once"
            else:
                observed = json.loads(native_report.read_text())
                required = {"dream_fold_and_human_activity_are_project_isolated",
                            "retiring_blocked_A_joins_Dream_and_B_Memory_CLI_continues",
                            "last_project_retirement_completes",
                            "native_daemon_owned_processes_and_listener_joined"}
                names = {item["name"] for item in observed.get("observations", [])}
                if (observed.get("task") != "tsk-cab0" or observed.get("status") != "passed_stage"
                        or not observed.get("owned_processes_joined") or not required <= names):
                    report["failure"] = "native_records_focal_receipt_incomplete"
                else:
                    report["status"] = "qualified_functional_subset"
                    report["owned_processes_joined"] = True
                    report["observations"] = observed["observations"]
        # The caller supplies a private result path. Preserve diagnostics there,
        # never embed child output or credentials in a shared receipt.
        args.result.parent.mkdir(parents=True, exist_ok=True)
        log = args.result.with_suffix(".log")
        with log.open("wb") as output:
            log.chmod(0o600)
            output.write(stdout + b"\n" + stderr)
        report["test_wall_ms"] = round((time.monotonic() - started) * 1000)
        report["selector"] = selector
        args.result.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "coverage": report["coverage"],
                      "failure": report.get("failure"), "test_wall_ms": report["test_wall_ms"]}), flush=True)
    return 0 if report["status"] == "qualified_functional_subset" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graphit", required=True, help="Existing compiled Linux executable")
    parser.add_argument("--result", required=True, type=Path)
    parser.add_argument("--versions", action="store_true", help="Also switch real private Git branches")
    parser.add_argument("--model-server", type=Path, help="Existing compiled graphit-model-server")
    parser.add_argument("--ort-library", type=Path, help="Existing qualified CPU ONNX Runtime library")
    parser.add_argument("--memory-dream", action="store_true", help="Run only the integrated Memory/Dream fixture, without replaying the base stage")
    parser.add_argument("--app-tests", type=Path, help="Existing compiled graphit-app lib-test executable containing the new records focal")
    args = parser.parse_args()
    if args.memory_dream:
        if args.versions or args.model_server or args.ort_library:
            parser.error("--memory-dream does not run --versions or the CPU model stage")
        if args.app_tests is None:
            parser.error("--memory-dream requires --app-tests")
        args.app_tests = args.app_tests.resolve(strict=True)
        raise SystemExit(run_memory_dream_stage(args))
    if args.app_tests is not None:
        parser.error("--app-tests is only used with --memory-dream")
    if bool(args.model_server) != bool(args.ort_library):
        parser.error("--model-server and --ort-library must be supplied together")
    if args.model_server:
        args.model_server = args.model_server.resolve(strict=True)
        args.ort_library = args.ort_library.resolve(strict=True)
        if args.model_server.name != "graphit-model-server":
            parser.error("the real graphit-model-server executable is required")
    started = time.monotonic()
    report = {"task": "tsk-cab0", "platform": "linux", "compilation_ms": 0,
              "coverage": "cli_mcp_ast_knowledge_lifecycle",
              "remaining": ["model_embedding_memory_dream_and_generation_borrowers",
                            "captured_version_switch", "integrated_browser_and_tray",
                            "native_windows_macos"]}
    if args.versions:
        report["remaining"].remove("captured_version_switch")
        report["coverage"] += "_git_versions"
    if args.model_server:
        report["coverage"] += "_shared_cpu_model"
        report["remaining"][0] = "memory_dream_and_profile_generation_borrowers"
    with tempfile.TemporaryDirectory(prefix="graphit-cab0-") as temporary:
        journey = Journey(str(Path(args.graphit).resolve()), Path(temporary), args.versions,
                          args.model_server, args.ort_library)
        try:
            journey.run()
            report["status"] = "passed_stage"
        except Exception as error:
            report["status"] = "failed"
            report["failure"] = str(error).replace(temporary, "<fixture>")
            raise
        finally:
            try:
                journey.cleanup()
                report["owned_processes_joined"] = True
            except Exception as error:
                report["cleanup_failure"] = str(error).replace(temporary, "<fixture>")
                raise
            finally:
                report["observations"] = journey.observations
                report["test_wall_ms"] = round((time.monotonic() - started) * 1000)
                args.result.parent.mkdir(parents=True, exist_ok=True)
                args.result.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
