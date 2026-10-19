#!/usr/bin/env python3
"""Qualify the finite AST fixture with genuine browser readers and signed OAuth."""
import argparse
import hashlib
import json
import re
import time
import traceback
from importlib.metadata import version
from pathlib import Path
from urllib.parse import parse_qs, quote, urlencode, urlsplit

from playwright.sync_api import expect, sync_playwright

READS = {"/api/ast/" + name for name in (
    "contexts", "schema", "sample", "query", "neighborhood", "source", "search", "imports",
    "imported/schema", "imported/sample", "imported/query", "imported/neighborhood",
    "imported/source", "imported/search",
)}


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def run(args):
    probe_path = Path(args.probe)
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    probe_deadline = time.monotonic() + 60
    while not probe_path.exists() and time.monotonic() < probe_deadline:
        time.sleep(0.1)
    probe = json.loads(probe_path.read_text())
    origin = probe["origin"]
    endpoint = urlsplit(origin)
    assert endpoint.scheme == "http" and endpoint.hostname in {"localhost", "127.0.0.1", "::1"}
    assert not endpoint.username and not endpoint.password and not endpoint.query and not endpoint.fragment
    assert probe["task_read_allowed"] is False, "AST fixture with independent ReadAst admission"
    receipt = {"status": "running", "tool": "python-playwright", "steps": [], "http": [],
               "scope": "finite signed-OIDC AST PROJECT/import/search/Source browser",
               "producer_proof": "separate Root SDK qualification", "screenshots": []}
    bodies = []
    request_failures = []
    deadline = time.monotonic() + 720
    stage = "setup"
    started = time.monotonic()
    browser = None
    sdk = None

    def response_seen(response):
        # Auth evidence is status only. Never inspect headers, cookies or Auth bodies.
        path = urlsplit(response.url).path
        if path.startswith("/auth/") or path in READS or path == "/api/tasks/contexts":
            receipt["http"].append({"method": response.request.method, "path": path, "status": response.status})
        if path in READS and response.status == 200:
            bodies.append((path, response.request.post_data_json, response.json()))

    def step(name):
        nonlocal stage, started
        assert time.monotonic() < deadline, "finite browser deadline"
        stage = name
        started = time.monotonic()

    def passed():
        receipt["steps"].append({"name": stage, "status": "passed",
                                 "elapsed_ms": round((time.monotonic() - started) * 1000)})
        print(stage + ": passed", flush=True)

    def latest(path):
        reply_deadline = min(deadline, time.monotonic() + 20)
        while time.monotonic() < reply_deadline:
            observed = next((item for item in reversed(bodies) if item[0] == path), None)
            if observed is not None:
                return observed
            page.wait_for_timeout(50)
        raise AssertionError("expected native response was not observed")

    def screenshot(page, name):
        filename = name + ".png"
        page.screenshot(path=str(output / filename), full_page=False, timeout=10000)
        receipt["screenshots"].append(filename)

    def project_url(selection):
        return origin + "/ast/explorer/project?" + urlencode({"selection": compact(selection)})

    def mode_search(page, mode, term, imported=False):
        page.get_by_role("tab", name="Find & inspect", exact=True).click()
        page.get_by_role("combobox", name="Search mode", exact=True).click()
        page.get_by_role("option", name=mode, exact=True).click()
        page.get_by_label("Search indexed code", exact=True).fill(term)
        path = "/api/ast/imported/search" if imported else "/api/ast/search"
        with page.expect_response(lambda r: urlsplit(r.url).path == path) as response:
            page.get_by_role("button", name="Search index", exact=True).click()
        assert response.value.status == 200, "genuine admitted search response"
        _, request, result = latest(path)
        assert request["search"] == {"query": term, "mode": mode, "k": 20, "metric": "cosine"}
        if imported:
            assert "selection" not in request and set(request["source"]) == {"id", "version"}
        if mode == "fts":
            assert result["status"] == "available" and result["results"], "published FTS hits"
        elif mode == "hybrid" and result.get("vector_status"):
            assert result["results"], "genuine SDK partial lexical evidence"
            expect(page.get_by_text("Partial search results", exact=True)).to_be_visible()
        elif result["status"] != "available":
            expect(page.get_by_text("Search unavailable", exact=True)).to_be_visible()
            expect(page.get_by_text("No indexed matches", exact=True)).to_have_count(0)
        return result

    try:
        receipt["playwright"] = version("playwright")
        assert receipt["playwright"] == "1.63.0", "reuse qualified browser SDK"
        sdk = sync_playwright().start()
        browser = sdk.chromium.launch(executable_path=args.browser, headless=True, timeout=30000)
        receipt["chromium"] = browser.version
        assert browser.version, "record the actual installed Chromium version"
        context = browser.new_context(viewport={"width": 1280, "height": 800})
        context.set_default_timeout(20000)
        expect.set_options(timeout=20000)
        page = context.new_page()
        page.on("response", response_seen)
        page.on("requestfailed", lambda r: request_failures.append(
            {"path": urlsplit(r.url).path, "failure": r.failure}) if urlsplit(r.url).path in READS else None)

        step("actual_shipping_bundle")
        asset = page.request.get(origin + "/ast/explorer/project")
        assert asset.status == 200
        receipt["served_bundle_sha256"] = hashlib.sha256(asset.body()).hexdigest()
        shipping = Path(__file__).resolve().parents[1] / "crates/graphit-app/assets/ui/index.html"
        receipt["expected_bundle_sha256"] = hashlib.sha256(shipping.read_bytes()).hexdigest()
        assert receipt["served_bundle_sha256"] == receipt["expected_bundle_sha256"]
        passed()

        step("signed_oidc_and_independent_ast_admission")
        page.goto(origin + "/task/explorer")
        page.get_by_role("button", name=re.compile(r"^Entrar com ")).click()
        provider = page.get_by_role("menuitem", name="broker", exact=True)
        if provider.count():
            provider.click()
        expect(page.get_by_role("button", name="Sair da conta", exact=True)).to_be_visible()
        assert any(r["path"] == "/api/tasks/contexts" and r["status"] == 403 for r in receipt["http"])
        page.goto(project_url(probe["ast_contexts"]["current"]))
        expect(page.get_by_role("heading", name="Code investigation", exact=True)).to_be_visible()
        expect(page.get_by_text("Native AST graph", exact=True)).to_be_visible()
        assert latest("/api/ast/schema")[1]["selection"] == probe["ast_selection"]
        passed()

        step("project_sample_real_go_neighborhood_uid")
        page.get_by_role("tab", name="Relationship map", exact=True).click()
        page.get_by_role("button", name=re.compile(r"^LinkedAlpha relationships.go")).click()
        expect(page.get_by_role("heading", name="LinkedAlpha", exact=True)).to_be_visible()
        expect(page.get_by_role("button", name=re.compile(r"^LinkedBeta relationships.go"))).to_be_visible()
        page.get_by_role("button", name="Inspect source & impact", exact=True).click()
        expect(page.get_by_text("Native Source result", exact=True)).to_be_visible()
        _, _, source = latest("/api/ast/source")
        assert source["origin"] in {"indexed", "physical"} and "LinkedAlpha" in source["selection"]["source"]
        receipt["project_whole_file_origin"] = source["origin"]
        neighborhood = latest("/api/ast/neighborhood")[2]
        uid = next(prop["value"]["value"] for node in neighborhood["nodes"]
                   if node["id"] == neighborhood["anchor"] for prop in node["properties"] if prop["name"] == "uid")
        inspector = page.get_by_label("Relationship exploration", exact=True)
        expect(inspector.get_by_text(uid, exact=True)).to_be_visible()
        receipt["uid"] = {"from": "published uid property", "matched": True, "source": "relationships.go/LinkedAlpha"}
        page.get_by_role("button", name="Read entity source", exact=True).click()
        expect(page.get_by_text("Native Source result", exact=True)).to_be_visible()
        assert latest("/api/ast/source")[1]["source"] == {
            "path": "relationships.go", "entity": "LinkedAlpha", "entity_type": "Function"}
        assert latest("/api/ast/source")[2]["selection"]["source"].strip().startswith("func LinkedAlpha")
        assert latest("/api/ast/source")[2]["origin"] == "indexed", "entity source must use published Raw"
        passed()

        step("rust_published_observed_only")
        page.get_by_role("button", name="Close inspection", exact=True).click()
        page.get_by_role("button", name="All boundaries", exact=True).click()
        page.get_by_role("button", name=re.compile(r"^Alpha main.rs")).click()
        page.get_by_role("button", name="Inspect source & impact", exact=True).click()
        expect(page.get_by_text("Published file observations", exact=True)).to_be_visible()
        expect(page.get_by_text(re.compile("ObservedOnly")).first).to_be_visible()
        assert "Alpha" in latest("/api/ast/source")[2]["selection"]["source"]
        receipt["rust_resolution"] = {"published": "ObservedOnly", "absence_inferred_from_edges": False}
        screenshot(page, "project-source")
        passed()

        step("project_real_physical_path_source")
        page.get_by_role("button", name="Close inspection", exact=True).click()
        page.get_by_role("tab", name="Find & inspect", exact=True).click()
        page.get_by_label("File path", exact=True).fill(probe["ast_physical_path"])
        with page.expect_response(lambda r: urlsplit(r.url).path == "/api/ast/source") as physical_reply:
            page.get_by_role("button", name="Read source file", exact=True).click()
        assert physical_reply.value.status == 200
        expect(page.get_by_label("File source", exact=True)).to_be_visible()
        _, physical_request, physical = latest("/api/ast/source")
        assert physical_request == {"selection": probe["ast_selection"], "source": {"path": probe["ast_physical_path"]}}
        assert physical["origin"] == "physical" and physical["selection"]["source"] == probe["ast_physical_text"]
        expect(page.get_by_label("File source", exact=True).get_by_text(probe["ast_physical_text"].strip(), exact=True)).to_be_visible()
        expect(page.get_by_text(re.compile("Physical source read for the selected PROJECT"))).to_be_visible()
        page.get_by_role("button", name="Close file", exact=True).click()
        receipt["physical"] = {"same_project_version": True, "literal_bytes_matched": True, "origin": "physical", "graph_node_created": False}
        passed()

        step("project_query_typed_rows")
        page.get_by_role("tab", name="Query lab", exact=True).click()
        page.get_by_label("Cypher query", exact=True).fill("MATCH (n:Function) RETURN n.name, n.path LIMIT 10")
        with page.expect_response(lambda r: urlsplit(r.url).path == "/api/ast/query") as queried:
            page.get_by_role("button", name="Run query", exact=True).click()
        assert queried.value.status == 200
        expect(page.get_by_text("Query rows · 4", exact=True)).to_be_visible()
        assert len(latest("/api/ast/query")[2]["rows"]) == 4
        passed()

        step("project_four_search_modes")
        statuses = {}
        for mode in ("fts", "semantic", "vector", "hybrid"):
            result = mode_search(page, mode, "LinkedAlpha")
            statuses[mode] = {"status": result["status"], "vector_status": result.get("vector_status"),
                              "hits": len(result["results"])}
        receipt["project_search"] = statuses
        passed()

        step("project_selection_switch_and_literal_reload")
        current = probe["ast_contexts"]["current"]
        other = next(item for item in probe["ast_contexts"]["contexts"] if item != current)
        page.goto(project_url(other))
        page.get_by_role("tab", name="Relationship map", exact=True).click()
        expect(page.get_by_role("button", name=re.compile(r"^OtherAlpha main.rs"))).to_be_visible()
        assert latest("/api/ast/sample")[1]["selection"] == {
            "project_dir": other["project_dir"], "project_version": other["project_version"]}
        page.reload()
        expect(page.get_by_role("heading", name="Code investigation", exact=True)).to_be_visible()
        assert json.loads(parse_qs(urlsplit(page.url).query)["selection"][0]) == other
        passed()

        step("readonly_installed_catalogue_exact_uri")
        page.goto(origin + "/ast/contexts?library=imported")
        expect(page.get_by_role("heading", name="Indexed code contexts", exact=True)).to_be_visible()
        catalogue = latest("/api/ast/imports")[2]["imports"]
        installed = next(item for item in catalogue if item["id"] == "app-ast" and item["version"] == "release-1")
        page.get_by_role("button", name=installed["id"] + " · " + installed["version"], exact=True).click()
        page.get_by_role("button", name="Investigate this context", exact=True).click()
        expect(page.get_by_role("heading", name="Code investigation", exact=True)).to_be_visible()
        expect(page.get_by_text("Native AST graph", exact=True)).to_be_visible()
        values = parse_qs(urlsplit(page.url).query)
        assert "selection" not in values
        assert json.loads(values["source"][0]) == {"id": installed["id"], "version": installed["version"]}
        expect(page.get_by_role("button", name=re.compile("Unlink"))).to_have_count(0)
        passed()

        step("installed_sample_search_entity_source")
        page.get_by_role("tab", name="Relationship map", exact=True).click()
        expect(page.get_by_role("button", name=re.compile(r"^LinkedAlpha relationships.go"))).to_be_visible()
        result = mode_search(page, "fts", "LinkedAlpha", imported=True)
        hit = next(item for item in result["results"] if item["name"] == "LinkedAlpha")
        page.get_by_role("button", name=re.compile(r"^LinkedAlpha relationships.go")).click()
        expect(page.get_by_role("button", name=re.compile(r"^LinkedBeta relationships.go"))).to_be_visible()
        page.get_by_role("button", name="Inspect source & impact", exact=True).click()
        expect(page.get_by_text("Native Source result", exact=True)).to_be_visible()
        page.get_by_role("button", name="Read entity source", exact=True).click()
        expect(page.get_by_text("Native Source result", exact=True)).to_be_visible()
        _, request, reply = latest("/api/ast/imported/source")
        assert request == {"source": {"id": installed["id"], "version": installed["version"]},
                           "input": {"path": hit["path"], "entity": hit["name"], "entity_type": hit["kind"]}}
        assert reply["origin"] == "indexed" and "func LinkedAlpha" in reply["selection"]["source"]
        expect(page.get_by_label("Relationship exploration", exact=True).get_by_text(hit["uid"], exact=True)).to_be_visible()
        passed()

        step("installed_four_search_modes")
        page.get_by_role("button", name="Close inspection", exact=True).click()
        statuses = {}
        for mode in ("fts", "semantic", "vector", "hybrid"):
            result = mode_search(page, mode, "LinkedAlpha", imported=True)
            statuses[mode] = {"status": result["status"], "vector_status": result.get("vector_status"),
                              "hits": len(result["results"])}
        receipt["installed_search"] = statuses
        for path, request, _ in bodies:
            if path.startswith("/api/ast/imported/"):
                assert "selection" not in request and set(request["source"]) == {"id", "version"}
        passed()

        step("search_abort_actual_late_reply_hidden")
        page.get_by_role("tab", name="Find & inspect", exact=True).click()
        page.get_by_role("combobox", name="Search mode", exact=True).click()
        page.get_by_role("option", name="fts", exact=True).click()
        page.get_by_label("Search indexed code", exact=True).fill("LinkedAlpha")
        held = []
        page.route("**/api/ast/imported/search", lambda route: held.append((route, route.fetch())))
        page.get_by_role("button", name="Search index", exact=True).click()
        expect(page.get_by_role("button", name="Cancel search", exact=True)).to_be_visible()
        held_deadline = min(deadline, time.monotonic() + 20)
        while not held and time.monotonic() < held_deadline:
            page.wait_for_timeout(50)
        assert held and held[0][1].status == 200, "real server accepted and answered"
        page.get_by_role("button", name="Cancel search", exact=True).click()
        expect(page.get_by_text("Search request stopped.", exact=False)).to_be_visible()
        try:
            held[0][0].fulfill(response=held[0][1])
        except Exception:
            # Chromium may already have discarded the aborted route.
            pass
        expect(page.get_by_role("button", name=re.compile(r"^LinkedAlpha relationships.go"))).to_have_count(0)
        page.unroute("**/api/ast/imported/search")
        receipt["abort"] = {"real_server_reply": 200, "late_payload_hidden": True}
        passed()

        step("narrow_keyboard_literal_import_reload")
        page.set_viewport_size({"width": 390, "height": 844})
        page.reload()
        expect(page.get_by_role("heading", name="Code investigation", exact=True)).to_be_visible()
        tab = page.get_by_role("tab", name="Find & inspect", exact=True)
        tab.focus()
        expect(tab).to_be_focused()
        tab.press("Enter")
        search = page.get_by_label("Search indexed code", exact=True)
        search.focus()
        expect(search).to_be_focused()
        receipt["narrow_layout"] = page.evaluate("""() => ({viewport:innerWidth, scroll:document.documentElement.scrollWidth,
            overflowing:[...document.querySelectorAll('body *')].map(node=>{const r=node.getBoundingClientRect();return {tag:node.tagName,class:typeof node.className==='string'?node.className:'',right:Math.round(r.right)}}).filter(r=>r.right>innerWidth+1).slice(0,15)})""")
        assert receipt["narrow_layout"]["scroll"] <= receipt["narrow_layout"]["viewport"] + 1
        screenshot(page, "narrow-import")
        assert "selection" not in parse_qs(urlsplit(page.url).query)
        passed()

        step("logout_real_denial_no_stale_source")
        imported_url = page.url
        page.get_by_role("button", name="Sair da conta", exact=True).click()
        expect(page.get_by_role("button", name=re.compile(r"^Entrar com "))).to_be_visible()
        with page.expect_response(lambda r: urlsplit(r.url).path == "/api/ast/imports" and r.status in {401, 403}):
            page.goto(imported_url)
        expect(page.get_by_label("Symbol investigation", exact=True)).to_have_count(0)
        expect(page.get_by_role("button", name=re.compile(r"^LinkedAlpha relationships.go"))).to_have_count(0)
        passed()
        assert receipt["project_whole_file_origin"] == "indexed", "PROJECT whole-file source must prefer published Raw"
        receipt["status"] = "passed"
        context.close()
        browser.close()
        browser = None
    except Exception as error:
        receipt["status"] = "failed"
        receipt["steps"].append({"name": stage, "status": "failed", "error_type": type(error).__name__,
                                 "runner_line": next(frame.lineno for frame in reversed(traceback.extract_tb(error.__traceback__)) if frame.filename == __file__)})
        print(stage + ": failed (" + type(error).__name__ + ")", flush=True)
        if browser:
            for context in browser.contexts:
                for page in context.pages:
                    if not stage.startswith("signed_oidc"):
                        try:
                            screenshot(page, "failure")
                        except Exception:
                            pass
            try:
                browser.close()
            except Exception:
                # The sync SDK has already closed its event loop on exceptional exit.
                pass
    finally:
        if sdk:
            sdk.stop()
        receipt["elapsed_ms"] = round((time.monotonic() - deadline + 720) * 1000)
        receipt["request_failures"] = request_failures
        (output / "qualification.json").write_text(json.dumps(receipt, indent=2) + "\n")
        probe_path.with_suffix(".done").write_text("browser finished\n")
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--probe", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--browser")
    parser.add_argument("--scenario", choices=("ast", "records"), default="ast")
    parser.add_argument("--records-focus", choices=("all", "memory", "knowledge", "knowledge-imported", "knowledge-controls", "knowledge-denial"), default="all")
    parser.add_argument("--keep-host", action="store_true")
    args = parser.parse_args()
    if args.scenario == "records":
        from native_record_browser_scenario import run_records
        raise SystemExit(run_records(args))
    raise SystemExit(run(args))
