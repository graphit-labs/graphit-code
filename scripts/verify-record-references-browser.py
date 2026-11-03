#!/usr/bin/env python3
"""Qualify the finite Rust record fixture through its real browser UI."""

import argparse
import json
import hashlib
import re
import sys
import time
import traceback
from importlib.metadata import version
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit, urlunsplit

from playwright.sync_api import expect, sync_playwright


READS = {
    "/api/tasks/get", "/api/tasks/session", "/api/tasks/context", "/api/tasks/catalogue", "/api/tasks/session", "/api/tasks/contexts",
    "/api/memory/contexts", "/api/knowledge/contexts",
    "/api/memory/history", "/api/user-memory/history",
    "/api/knowledge/page", "/api/knowledge/imported/page",
    "/api/records/references", "/api/knowledge/imported/contexts",
}


def project_locator(selection):
    return {key: selection[key] for key in ("project_id", "project_version")}


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def widget(page):
    return page.get_by_role("heading", name="Record links", exact=True).locator(
        "xpath=ancestor::section[1]"
    )


def project_url(origin, probe, kind, identifier):
    paths = {"task": "/task/explorer/", "memory": "/memory/explorer/project/"}
    from urllib.parse import quote
    return origin + paths[kind] + quote(identifier, safe="") + "?" + urlencode(
        {"selection": compact(project_locator(probe["selection"]))}
    )


def require_selection(url, expected):
    values = parse_qs(urlsplit(url).query).get("selection", [])
    assert len(values) == 1 and json.loads(values[0]) == project_locator(expected), "portable selection retained"


def run(args):
    probe_path = Path(args.probe)
    output = Path(args.output)
    deadline = time.monotonic() + 720
    receipt = {"status": "running", "tool": "python-playwright",
               "steps": [], "http": [], "screenshots": [],
               "coverage": "real record navigation, paging, opt-ins, reload, focus, keyboard, narrow view, anonymous denial",
               "ut2_complete": False,
               "required_remaining": ["real expired credential", "missing projection source",
                                      "same ID/version with distinct NativePaths", "in-flight abort"]}
    bodies = []
    stage = "setup"
    stage_started = time.monotonic()
    failure = None
    browser = None
    context = None

    def response_seen(response):
        # Never access headers, cookies, auth bodies or URL query parameters.
        path = urlsplit(response.url).path
        if path.startswith("/auth/") or path in READS:
            receipt["http"].append({"method": response.request.method, "path": path, "status": response.status})
        if path in READS and response.status == 200:
            try:
                bodies.append((path, response.request.post_data_json, response.json()))
            except Exception as cause:
                receipt["http"].append({"path": path, "decode_error": type(cause).__name__})

    def step(name):
        nonlocal stage, stage_started
        assert time.monotonic() < deadline, "finite browser deadline"
        stage = name
        stage_started = time.monotonic()

    def passed():
        receipt["steps"].append({"name": stage, "status": "passed",
                                 "elapsed_ms": round((time.monotonic() - stage_started) * 1000)})
        if stage == "same_identity_distinct_native_paths":
            receipt["required_remaining"].remove("same ID/version with distinct NativePaths")
            if receipt.get("abort", {}).get("request_failed") == "net::ERR_ABORTED":
                receipt["required_remaining"].remove("in-flight abort")
        if stage == "missing_projection_no_invented_navigation":
            receipt["required_remaining"].remove("missing projection source")

    def screenshot(page, name):
        filename = name + ".png"
        page.screenshot(path=str(output / filename), full_page=False, timeout=10000)
        receipt["screenshots"].append(filename)

    def narrow_keyboard(page):
        step("narrow_keyboard_reload")
        page.set_viewport_size({"width": 390, "height": 844})
        page.reload(timeout=30000)
        expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()
        expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_focused()
        expect(widget(page).get_by_role("heading", name="References", exact=True)).to_be_visible()
        button = widget(page).get_by_role("button", name="Reload links", exact=True)
        button.focus()
        expect(button).to_be_focused()
        button.press("Enter")
        expect(widget(page).get_by_role("heading", name="References", exact=True)).to_be_visible()
        receipt["narrow_layout"] = page.evaluate("""() => ({ viewport: window.innerWidth, scroll: document.documentElement.scrollWidth,
            overflowing: [...document.querySelectorAll('body *')].map(node => { const rect = node.getBoundingClientRect(); return { tag: node.tagName, class: typeof node.className === 'string' ? node.className : '', left: Math.round(rect.left), right: Math.round(rect.right), width: Math.round(rect.width) }; }).filter(rect => rect.right > window.innerWidth + 1).slice(0, 20) })""")
        assert receipt["narrow_layout"]["scroll"] <= receipt["narrow_layout"]["viewport"] + 1, "no global narrow overflow"
        screenshot(page, "narrow-keyboard")
        passed()


    def logout_denial(page, memory_href):
        step("logout_permission_no_old_record")
        page.get_by_role("button", name="Sair da conta", exact=True).click()
        expect(page.get_by_role("button", name=re.compile(r"^Entrar com "))).to_be_visible()
        with page.expect_response(lambda response:
                urlsplit(response.url).path in {"/api/memory/contexts", "/api/tasks/contexts", "/api/memory/history"}
                and response.status in {401, 403}, timeout=15000) as denied:
            page.goto(origin + memory_href, timeout=30000)
        assert denied.value.status in {401, 403}, "actual anonymous admission denial"
        expect(page.get_by_role("article", name="Memory record", exact=True)).to_have_count(0)
        expect(page.get_by_role("button", name=re.compile(r"^Entrar com "))).to_be_visible()
        passed()

    try:
        output.mkdir(parents=True, exist_ok=True)
        probe = json.loads(probe_path.read_text())
        budget = 240 if probe.get("scenario") == "session_expiration" else 720
        deadline = time.monotonic() + budget
        receipt["browser_budget_seconds"] = budget
        origin = probe["origin"]
        parsed = urlsplit(origin)
        assert parsed.scheme == "http" and parsed.hostname in {"localhost", "127.0.0.1", "::1"}
        assert not parsed.username and not parsed.password and not parsed.query and not parsed.fragment
        assert probe["task_read_allowed"] is True, "refs fixture, not the separate AST fixture"
        receipt["playwright"] = version("playwright")
        assert receipt["playwright"] == "1.63.0", "qualified Playwright pin"
        step("launch")
        with sync_playwright() as sdk:
            try:
                browser = sdk.chromium.launch(executable_path=args.browser, headless=True, timeout=30000)
                assert browser.version == "155.0.8059.39", "qualified Chromium pin"
                receipt["chromium"] = browser.version
                context = browser.new_context(viewport={"width": 1280, "height": 800})
                context.set_default_timeout(30000)
                expect.set_options(timeout=30000)
                page = context.new_page()
                page.on("response", response_seen)
                task_url = project_url(origin, probe, "task", probe["task_id"])

                step("served_shipping_bundle")
                asset = page.request.get(origin + "/task/explorer", timeout=30000)
                assert asset.status == 200, "actual public compiled UI asset"
                receipt["served_bundle_sha256"] = hashlib.sha256(asset.body()).hexdigest()
                expected_asset = Path(__file__).resolve().parents[1] / "crates/graphit-app/assets/ui/index.html"
                receipt["expected_bundle_sha256"] = hashlib.sha256(expected_asset.read_bytes()).hexdigest()
                assert receipt["served_bundle_sha256"] == receipt["expected_bundle_sha256"], "actual shipping bundle matches compiled public asset"
                passed()

                step("entrar_oidc_real")
                page.goto(task_url, timeout=30000)
                page.get_by_role("button", name=re.compile(r"^Entrar com ")).click()
                provider = page.get_by_role("menuitem", name="broker", exact=True)
                if provider.count():
                    provider.click()
                expect(page.get_by_role("button", name="Sair da conta", exact=True)).to_be_visible(timeout=30000)
                assert any(r["path"] == "/auth/login" and r["status"] == 200 for r in receipt["http"])
                assert any(r["path"].startswith("/auth/callback") and r["status"] in {200, 302, 303} for r in receipt["http"])
                passed()

                if args.only_narrow:
                    page.goto(origin + "/knowledge/explorer?" + urlencode({"selection": compact(project_locator(probe["selection"])), "page": probe["wiki_slug"]}), timeout=30000)
                    expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()
                    narrow_keyboard(page)
                    logout_denial(page, project_url(origin, probe, "memory", probe["memory_id"])[len(origin):])
                    receipt["coverage"] = "focused real Page reload at narrow viewport, settled reader focus, keyboard references and actual logout denial"
                    receipt["required_remaining"] = ["main journey and session expiration in separate receipts"]
                elif probe.get("scenario") == "session_expiration":
                    step("session_expiration_initial_reader")
                    page.goto(task_url, timeout=30000)
                    expect(page.get_by_role("heading", name="Specification", exact=True)).to_be_visible()
                    assert any(entry["path"] == "/auth/session" and entry["status"] == 200 for entry in receipt["http"])
                    assert any(entry["path"] == "/api/tasks/get" and entry["status"] == 200 for entry in receipt["http"])
                    require_selection(page.url, probe["selection"])
                    screenshot(page, "before-session-expiration")
                    passed()
                    step("session_expiration_actual_denial")
                    lifetime = probe["session_lifetime_ms"]
                    assert 1000 <= lifetime <= 60000, "bounded configured session lifetime"
                    page.wait_for_timeout(lifetime + 500)
                    with (page.expect_response(lambda response: urlsplit(response.url).path == "/auth/session"
                            and response.status == 401, timeout=30000) as expired,
                          page.expect_response(lambda response: urlsplit(response.url).path in {"/api/tasks/contexts", "/api/tasks/get"}
                            and response.status in {401, 403}, timeout=30000) as protected):
                        page.reload(timeout=30000)
                    assert expired.value.status == 401 and protected.value.status in {401, 403}
                    expect(page.get_by_role("button", name=re.compile(r"^Entrar com "))).to_be_visible()
                    expect(page.get_by_role("heading", name="Specification", exact=True)).to_have_count(0)
                    assert any(entry["path"] in {"/api/tasks/contexts", "/api/tasks/get"}
                               and entry["status"] in {401, 403} for entry in receipt["http"])
                    screenshot(page, "expired-session-denial")
                    receipt["session_expiration"] = {"lifetime_ms": lifetime, "auth_session_status": 401,
                                                       "initial_reader_status": 200, "credential_injected": False}
                    passed()
                    receipt["coverage"] = "real login, initial protected reader, actual session timer expiration and denial"
                    receipt["required_remaining"] = ["main journey and its other UT2 scenarios in separate receipt"]
                else:
                    step("task_focal_default")
                    page.goto(task_url, timeout=30000)
                    expect(page.get_by_role("heading", name="Specification", exact=True)).to_be_visible()
                    links = widget(page)
                    expect(links.get_by_role("heading", name="References", exact=True)).to_be_visible()
                    expect(links.get_by_role("heading", name="Referenced by", exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    initial = [request for path, request, _ in bodies if path == "/api/records/references"][-1]
                    assert initial["selection"] == probe["selection"]
                    assert initial["input"]["include_user"] is False and initial["input"]["contexts"] == []
                    assert all(p["source"]["kind"] == "project_task" for p in initial["input"]["pages"])
                    screenshot(page, "task-focal")
                    passed()

                    step("memory_current_and_history")
                    memory_link = links.get_by_role("link").filter(has_text=probe["memory_id"])
                    expect(memory_link).to_have_count(1)
                    memory_href = memory_link.get_attribute("href")
                    memory_link.click()
                    expect(page.get_by_role("article", name="Memory record", exact=True)).to_be_visible()
                    expect(page.get_by_role("heading", name="Revised foreground memory", exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    revisions = page.get_by_role("group", name="Memory revisions", exact=True).get_by_role("button")
                    assert revisions.count() >= 2, "SDK creation and genuine revision"
                    revisions.first.click()
                    expect(page.get_by_role("heading", name="Created foreground memory", exact=True)).to_be_visible()
                    expect(page.get_by_text("Historical revision", exact=True)).to_be_visible()
                    revisions.last.click()
                    expect(page.get_by_role("heading", name="Revised foreground memory", exact=True)).to_be_visible()
                    memory_url = page.url
                    screenshot(page, "memory-history")
                    passed()

                    step("namespace_mismatch_and_ambiguous_selection")
                    parts = urlsplit(memory_url)
                    parameters = parse_qs(parts.query)
                    focus = json.loads(parameters["reference_focus"][0])
                    step("namespace_descriptor_identity")
                    assert focus["id"] == probe["memory_id"]
                    step("namespace_descriptor_scope")
                    assert focus["scope"] == {"project": probe["selection"]["project_id"]}
                    other = next(item for item in probe["contexts"]["contexts"]
                                 if item["project_id"] != probe["selection"]["project_id"])
                    focus["scope"] = {"project": other["project_id"]}
                    parameters["reference_focus"] = [compact(focus)]
                    wrong = urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(parameters, doseq=True), ""))
                    step("namespace_mismatch_denial")
                    page.goto(wrong, timeout=30000)
                    expect(page.get_by_text("Reference destination unavailable", exact=True)).to_be_visible()
                    expect(page.get_by_role("article", name="Memory record", exact=True)).to_have_count(0)
                    passed()
                    duplicate = memory_url + "&" + urlencode({"selection": compact(project_locator(probe["selection"]))})
                    step("duplicate_selection_denial")
                    page.goto(duplicate, timeout=30000)
                    notice = page.get_by_role("alert").filter(has_text="The record selection is ambiguous")
                    expect(notice).to_be_visible()
                    expect(notice).to_contain_text("The record selection is ambiguous")
                    expect(page.get_by_role("article", name="Memory record", exact=True)).to_have_count(0)
                    passed()
                    step("namespace_valid_restore")
                    page.goto(memory_url, timeout=30000)
                    expect(page.get_by_role("heading", name="Revised foreground memory", exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    passed()

                    step("knowledge_reader_and_backlinks")
                    widget(page).get_by_role("link").filter(has_text=probe["wiki_slug"]).click()
                    expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    wiki_url = page.url
                    page.reload(timeout=30000)
                    expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()
                    assert page.url == wiki_url
                    widget(page).get_by_role("checkbox", name="Include other project sources", exact=True).check()
                    expect(widget(page).get_by_role("link").filter(has_text=probe["task_id"]).first).to_be_visible()
                    expect(widget(page).get_by_role("link").filter(has_text=probe["session_id"]).first).to_be_visible()
                    pages = [body for path, _, body in bodies if path == "/api/knowledge/page"]
                    assert pages[-1]["focused_source"]["namespace"]["scope"] == {"project": probe["selection"]["project_id"]}
                    screenshot(page, "knowledge-backlinks")
                    passed()

                    step("session_reader_to_memory")
                    widget(page).get_by_role("link").filter(has_text=probe["session_id"]).first.click()
                    expect(page.get_by_role("heading", name="Request", exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    assert any(entry["path"] == "/api/tasks/session" and entry["status"] == 200 for entry in receipt["http"])
                    widget(page).get_by_role("link").filter(has_text=probe["memory_id"]).click()
                    expect(page.get_by_role("heading", name="Revised foreground memory", exact=True)).to_be_visible()
                    require_selection(page.url, probe["selection"])
                    passed()

                    step("zero_edge_observed_namespace")
                    empty_url = origin + "/knowledge/explorer?" + urlencode({
                        "selection": compact(project_locator(probe["selection"])), "page": probe["empty_slug"]})
                    page.goto(empty_url, timeout=30000)
                    expect(page.get_by_role("heading", name=probe["empty_slug"], exact=True)).to_be_visible()
                    expect(widget(page).get_by_role("button", name="Reload links", exact=True)).to_be_enabled()
                    empty_pages = [body for path, request, body in bodies if path == "/api/knowledge/page"
                                   and request["input"]["slug"] == probe["empty_slug"]]
                    assert empty_pages[-1]["focused_source"]["namespace"]["scope"] == {"project": probe["selection"]["project_id"]}
                    assert empty_pages[-1]["focused_source"]["witness"] is not None
                    empty_refs = [body for path, request, body in bodies if path == "/api/records/references"
                                  and request["input"]["focus"]["id"] == probe["empty_slug"]]
                    assert empty_refs[-1]["outgoing_complete"] is True and empty_refs[-1]["incoming_complete"] is True
                    expect(widget(page).get_by_role("link")).to_have_count(0)
                    screenshot(page, "zero-edge-namespace")
                    passed()
                    page.goto(wiki_url, timeout=30000)
                    expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()

                    step("per_source_pagination_user_import_optins")
                    widget(page).get_by_role("link").filter(has_text=probe["task_id"]).first.click()
                    expect(page.get_by_role("heading", name="Specification", exact=True)).to_be_visible()
                    links = widget(page)
                    links.get_by_role("checkbox", name="Include other project sources", exact=True).check()
                    step("project_memory_first_page")
                    memory_source = links.locator('section[aria-label="Project Memory incoming"]')
                    expect(memory_source.get_by_role("button", name="Next page Project Memory incoming", exact=True)).to_be_visible()
                    expect(links.get_by_text("Partial reference view", exact=True)).to_be_visible()
                    first_count = memory_source.get_by_role("link").count()
                    assert first_count == 20, "actual default page_size20"
                    passed()
                    step("project_memory_next_page")
                    memory_source.get_by_role("button", name="Next page Project Memory incoming", exact=True).click()
                    expect(memory_source.get_by_role("link")).to_have_count(22)
                    expect(memory_source.get_by_role("button", name="Next page Project Memory incoming", exact=True)).to_have_count(0)
                    passed()
                    step("user_reference_optin")
                    links.get_by_role("checkbox", name="Include personal Memory references", exact=True).check()
                    personal = links.locator('section[aria-label="Personal Memory incoming"]')
                    expect(personal.get_by_role("link").filter(has_text=probe["user_memory_id"])).to_be_visible()
                    passed()
                    step("imported_reference_optin")
                    links.get_by_role("button", name="Choose imported sources", exact=True).click()
                    imported = links.get_by_role("group", name="Imported reference sources", exact=True)
                    expect(imported.get_by_role("checkbox")).to_have_count(1)
                    imported.get_by_role("checkbox").check()
                    imported_label = "Imported Knowledge: " + probe["import_source"]["artifact"]["id"] + " @ " + probe["import_source"]["artifact"]["version"]
                    imported_incoming = links.locator('section').filter(has=page.get_by_role("heading", name=imported_label, exact=True)).last
                    expect(imported_incoming.get_by_role("link").filter(has_text=probe["import_slug"])).to_be_visible()
                    screenshot(page, "sources-optins")
                    passed()

                    step("user_own_history")
                    personal.get_by_role("link").filter(has_text=probe["user_memory_id"]).click()
                    expect(page.get_by_role("article", name="Memory record", exact=True)).to_be_visible()
                    expect(page.get_by_role("heading", name="Own USER reference", exact=True)).to_be_visible()
                    params = parse_qs(urlsplit(page.url).query)
                    assert params["scope_id"] == [probe["user_scope_id"]]
                    assert "selection" not in params
                    passed()

                    step("imported_namespace_reader")
                    page.goto(task_url, timeout=30000)
                    links = widget(page)
                    links.get_by_role("button", name="Choose imported sources", exact=True).click()
                    links.get_by_role("group", name="Imported reference sources", exact=True).get_by_role("checkbox").check()
                    imported_links = links.locator('section').filter(has=page.get_by_role("heading", name=imported_label, exact=True)).last
                    imported_links.get_by_role("link").filter(has_text=probe["import_slug"]).click()
                    expect(page.get_by_role("heading", name=probe["import_slug"], exact=True)).to_be_visible()
                    params = parse_qs(urlsplit(page.url).query)
                    assert "selection" not in params and json.loads(params["source"][0]) == probe["import_source"]
                    imported_pages = [body for path, _, body in bodies if path == "/api/knowledge/imported/page"]
                    latest = imported_pages[-1]
                    assert "selection" not in latest and latest["source"] == probe["import_source"]
                    assert latest["focused_source"]["namespace"]["scope"] != {"project": probe["selection"]["project_id"]}
                    expect(widget(page).get_by_role("heading", name="References", exact=True)).to_be_visible()
                    imported_composition = [(request, body) for path, request, body in bodies
                                            if path == "/api/records/references" and "selection" not in request
                                            and request["input"].get("outgoing_source") == probe["import_source"]]
                    assert imported_composition, "actual imported focal composition succeeds"
                    request, composition = imported_composition[-1]
                    assert request["input"]["contexts"] == [probe["import_source"]["artifact"]]
                    assert request["input"]["focus"]["scope"] == latest["focused_source"]["namespace"]["scope"]
                    assert composition["selection"] is None
                    assert [source["source"] for source in composition["sources"]] == [probe["import_source"]]
                    screenshot(page, "imported-namespace")
                    passed()

                    step("abort_real_project_request")
                    page.goto(task_url, timeout=30000)
                    expect(widget(page).get_by_role("link").filter(has_text=probe["memory_id"])).to_be_visible()
                    assert probe["control"] == "issuer_hold_entered_release"
                    assert probe["fixture_contract"] == "mirror_issuer_wait_registered_v1"

                    def wait_marker(suffix):
                        until = min(deadline, time.monotonic() + 30)
                        while not probe_path.with_suffix(suffix).exists():
                            assert time.monotonic() < until, "finite fixture control deadline"
                            page.wait_for_timeout(50)

                    probe_path.with_suffix(".hold").write_text("arm issuer hold\n")
                    try:
                        wait_marker(".armed")
                        with page.expect_request(lambda request: urlsplit(request.url).path == "/api/records/references"):
                            widget(page).get_by_role("button", name="Reload links", exact=True).click()
                        wait_marker(".entered")
                        with page.expect_event("requestfailed", predicate=lambda request:
                                urlsplit(request.url).path == "/api/records/references", timeout=30000) as aborted:
                            widget(page).get_by_role("button", name="Cancel links", exact=True).click()
                        assert aborted.value.failure == "net::ERR_ABORTED", "real browser transport abort"
                        receipt["abort"] = {"request_failed": "net::ERR_ABORTED", "issuer_entered": True,
                                            "handler_joined": False, "control_marker_is_runtime_proof": False}
                    finally:
                        probe_path.with_suffix(".release").write_text("release issuer hold\n")
                    wait_marker(".released")
                    passed()

                    step("same_identity_distinct_native_paths")
                    mirror = probe["mirror_selection"]
                    assert mirror["project_id"] == probe["selection"]["project_id"]
                    assert mirror["project_version"] == probe["selection"]["project_version"]
                    assert mirror["project_dir"] != probe["selection"]["project_dir"]
                    mirror_label = mirror["project_dir"]
                    assert isinstance(mirror_label, str)
                    mirror_label.encode("utf-8", errors="strict")
                    page.get_by_role("group", name="Working context", exact=True).get_by_role("combobox", name="Project", exact=True).click()
                    option = page.get_by_role("option").filter(has_text=mirror_label)
                    expect(option).to_have_count(1)
                    with page.expect_response(lambda response:
                            urlsplit(response.url).path == "/api/tasks/catalogue"
                            and response.request.post_data_json.get("selection") == mirror, timeout=30000) as mirror_read:
                        option.click()
                    assert mirror_read.value.status == 200, "actual mirror catalogue reader"
                    assert mirror_read.value.json()["results"] == [], "genuine mirror has no Tasks"
                    require_selection(page.url, mirror)
                    expect(page.get_by_text("No tasks match this view.", exact=True)).to_be_visible()
                    expect(page.get_by_role("heading", name="Select a task", exact=True)).to_be_visible()
                    require_selection(page.url, mirror)
                    expect(page.get_by_text("Task to native Memory and Knowledge", exact=True)).to_have_count(0)
                    expect(page.get_by_role("heading", name="Record links", exact=True)).to_have_count(0)
                    page.goto(origin + "/memory/explorer/project/" + probe["mirror_memory_id"], timeout=30000)
                    page.get_by_role("group", name="Working context", exact=True).get_by_role("combobox", name="Project", exact=True).click()
                    page.get_by_role("option").filter(has_text=mirror_label).click()
                    expect(page.get_by_role("heading", name="Mirror memory", exact=True)).to_be_visible()
                    require_selection(page.url, mirror)
                    expect(page.get_by_role("heading", name="Revised foreground memory", exact=True)).to_have_count(0)
                    memory_reads = [request for path, request, _ in bodies if path == "/api/memory/history"]
                    assert memory_reads[-1]["selection"] == mirror
                    screenshot(page, "mirror-memory")
                    passed()

                    step("missing_projection_no_invented_navigation")
                    missing_url = origin + "/knowledge/explorer?" + urlencode({"page": probe["missing_slug"]})
                    page.goto(missing_url, timeout=30000)
                    page.get_by_role("group", name="Working context", exact=True).get_by_role("combobox", name="Project", exact=True).click()
                    page.get_by_role("option").filter(has_text=mirror_label).click()
                    expect(page.get_by_role("heading", name=probe["missing_slug"], exact=True)).to_be_visible()
                    require_selection(page.url, mirror)
                    expect(widget(page).get_by_text("References unavailable", exact=True)).to_be_visible()
                    expect(widget(page).get_by_role("link")).to_have_count(0)
                    missing_pages = [body for path, request, body in bodies if path == "/api/knowledge/page"
                                     and request["selection"] == mirror and request["input"]["slug"] == probe["missing_slug"]]
                    focused = missing_pages[-1].get("focused_source")
                    assert focused is None or focused["namespace"] is None and focused["witness"] is None
                    assert not any(path == "/api/records/references" and request.get("selection") == mirror
                                   and request["input"]["focus"]["id"] == probe["missing_slug"] for path, request, _ in bodies)
                    screenshot(page, "missing-projection")
                    passed()
                    page.goto(wiki_url, timeout=30000)
                    expect(page.get_by_role("heading", name=probe["wiki_slug"], exact=True)).to_be_visible()

                    narrow_keyboard(page)
                    logout_denial(page, memory_href)
                receipt["status"] = "passed"
            except Exception as cause:
                if stage == "same_identity_distinct_native_paths":
                    mirror = probe["mirror_selection"]
                    parameters = parse_qs(urlsplit(page.url).query)
                    selection_values = parameters.get("selection", [])
                    receipt["mirror_diagnostic"] = {
                        "uri_selection_matches": len(selection_values) == 1 and json.loads(selection_values[0]) == mirror,
                        "task_reader_request_seen": any(path == "/api/tasks/get" and request.get("selection") == mirror
                                                         for path, request, _ in bodies),
                        "catalogue_request_seen": any(path == "/api/tasks/catalogue" and request.get("selection") == mirror
                                                             for path, request, _ in bodies),
                        "context_request_seen": any(path in {"/api/tasks/contexts", "/api/tasks/context"} and request.get("selection") == mirror
                                                    for path, request, _ in bodies),
                    }
                try:
                    screenshot(page, "failure-" + stage)
                except Exception as capture_error:
                    receipt["screenshot_error"] = type(capture_error).__name__
                failure = cause
                receipt["status"] = "failed"
                receipt["failure"] = {"stage": stage, "type": type(cause).__name__, "oracle_line": next((frame.lineno for frame in reversed(traceback.extract_tb(cause.__traceback__)) if Path(frame.filename).name == Path(__file__).name), None)}
            finally:
                for resource in (context, browser):
                    if resource is not None:
                        try:
                            resource.close()
                        except Exception as cause:
                            receipt.setdefault("cleanup_errors", []).append(type(cause).__name__)
                            if failure is None:
                                failure = cause
                                receipt["status"] = "failed"
                                receipt["failure"] = {"stage": "cleanup", "type": type(cause).__name__}
    except Exception as cause:
        if failure is None:
            failure = cause
            receipt["status"] = "failed"
            receipt["failure"] = {"stage": stage, "type": type(cause).__name__, "oracle_line": next((frame.lineno for frame in reversed(traceback.extract_tb(cause.__traceback__)) if Path(frame.filename).name == Path(__file__).name), None)}
        else:
            receipt.setdefault("cleanup_errors", []).append(type(cause).__name__)
    finally:
        # Completion signals drain, not successful browser acceptance.
        probe_path.with_suffix(".done").write_text("browser runner finished\n")
        (output / "qualification.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"status": receipt["status"], "steps": len(receipt["steps"]), "failure": receipt.get("failure")}))
    return 1 if failure is not None else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only-narrow", action="store_true")
    parser.add_argument("--probe", required=True)
    parser.add_argument("--browser", required=True)
    parser.add_argument("--output", default=".build/validation-rust/tsk-51f2/browser-records")
    sys.exit(run(parser.parse_args()))
