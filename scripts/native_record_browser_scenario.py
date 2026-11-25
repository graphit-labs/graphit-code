"""Records scenario for the existing finite native browser qualification runner."""
import hashlib
import json
import re
import time
import traceback
from importlib.metadata import version
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit

from playwright.sync_api import Error as PlaywrightError, expect, sync_playwright


def project_locator(selection):
    return {key: selection[key] for key in ("project_id", "project_version")}


def run_records(args):
    probe_path, output = Path(args.probe), Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    until = time.monotonic() + 60
    while not probe_path.exists() and time.monotonic() < until:
        time.sleep(0.1)
    if not probe_path.exists():
        (output / 'qualification.json').write_text(json.dumps({'status': 'failed', 'scope': 'host startup prerequisite', 'steps': [{'name': 'host_probe_available', 'status': 'failed', 'reason': 'Host did not publish its finite probe; no Browser interaction executed.'}], 'iab_qualified': False}, indent=2) + '\n')
        return 1
    probe = json.loads(probe_path.read_text())
    origin = probe['origin']
    endpoint = urlsplit(origin)
    assert endpoint.scheme == 'http' and endpoint.hostname in {'localhost', '127.0.0.1', '::1'}
    assert probe['scenario'] == 'db07'
    receipt = {'status': 'running', 'scope': 'native Records React consumer', 'focus': args.records_focus, 'steps': [], 'http': [],
               'provider': 'controlled Codex CLI fixture through public Foreground startup',
               'model_identity_claimed': False, 'iab_qualified': False,
               'knowledge_http_budget': {'fixture_deadline_ms': 120000, 'runner_response_timeout_ms': 130000, 'basis': 'Prepared.snapshot configures task.browser only; HubResourcePolicy default request.deadline_ms applies; 10s harness/render allowance'},
               'reused_evidence': [{'scope': 'Task/Session exact reload and own-module isolation', 'artifact': 'browser-04/qualification.json', 'reason': 'Readers/chrome unchanged by R11-R14; not repeated as a Memory gate.'}, {'scope': 'Knowledge PROJECT FTS/paging and Ask, imported catalogue/page/FTS', 'artifact': 'qualification.json', 'reason': 'Retained IAB03 observations; this run covers remaining consumers only.'}], 'screenshots': [], 'native_errors': [], 'writes': []}
    if args.records_focus in {'knowledge-imported', 'knowledge-controls', 'knowledge-denial'}:
        receipt['reused_evidence'].extend([{'scope': 'Memory first publication/CRUD/FTS/removal and zero PROJECT/Task I/O', 'artifact': 'browser-08/qualification.json', 'reason': 'Current bundle Memory consumers qualified independently.'}, {'scope': 'PROJECT hybrid/dot', 'artifact': 'browser-09/qualification.json', 'reason': 'HTTP200 partial/model_unavailable measured 49494ms click to complete JSON; no repeat.'}])
        receipt['not_exercised'] = ['Knowledge PROJECT no_corpus was not exercised in Browser; wire/component regressions and real backend HTTP qualification tsk-c135 cover this state separately.']
    if args.records_focus in {'knowledge-controls', 'knowledge-denial'}:
        receipt['reused_evidence'].append({'scope': 'Imported inventory/heads/page/reload/semantic/hybrid, exact SourceKey, zero PROJECT discovery, PROJECT absent and PublishedEmpty', 'artifact': 'browser-13/qualification.json', 'reason': 'Qualified independently before a message-selector failure; not repeated as a controls gate.'})
    receipt['coverage_by_layer'] = {'knowledge_no_corpus': {'browser_exercised': False, 'client': 'ui/src/api/nativeKnowledge.test.ts:103', 'component': 'ui/src/components/wiki/WikiExplorerNative.test.tsx:47', 'real_backend': 'tsk-c135 COM20 K2/KT1 HTTP08 13 Knowledge tests 7.99s; current Source assertions browser_auth/tests/knowledge.rs:251-259,355-357', 'historical_backend_logs_currently_available': False}, 'heads_abort': {'unit_context_change': True, 'http_cancel_or_late_browser_claimed': False}}
    bodies, legacy, failures, requests, pending = [], [], [], [], []
    stage, started, beginning = 'setup', time.monotonic(), time.monotonic()
    sdk = browser = page = None

    def observed(response):
        path = urlsplit(response.url).path
        if path.startswith('/api/wiki/'):
            legacy.append({'path': path, 'method': response.request.method})
        if path.startswith(('/auth/', '/api/tasks/', '/api/memory/', '/api/user-memory/', '/api/knowledge/')):
            receipt['http'].append({'path': path, 'method': response.request.method, 'status': response.status})
        if path.startswith(('/api/memory/', '/api/user-memory/', '/api/knowledge/')) and path != '/api/knowledge/ask' and response.request.method == 'POST' and response.headers.get('content-type', '').split(';', 1)[0].strip().lower() == 'application/json':
            pending.append(response)

    def collect_responses():
        # Reading bodies in the event callback yields reentrantly before append.
        # Consume completed bodies from the scenario before consulting evidence.
        completed = pending[:]
        pending.clear()
        for response in completed:
            path = urlsplit(response.url).path
            try:
                value = response.json()
                if response.status == 200:
                    bodies.append((path, response.request.post_data_json, value))
                elif response.status >= 400:
                    cause = value.get('error', value) if isinstance(value, dict) else None
                    code = cause.get('code') if isinstance(cause, dict) else None
                    if isinstance(code, str) and re.fullmatch(r'[a-z][a-z0-9_]{0,95}', code):
                        receipt['native_errors'].append({'path': path, 'status': response.status, 'code': code})
            except (PlaywrightError, ValueError, TypeError):
                receipt.setdefault('unreadable_responses', []).append({'path': path, 'status': response.status})

    def step(name):
        nonlocal stage, started
        assert time.monotonic() - beginning < 720
        stage, started = name, time.monotonic()

    def passed():
        receipt['steps'].append({'name': stage, 'status': 'passed', 'elapsed_ms': round((time.monotonic() - started) * 1000)})
        print(stage + ': passed', flush=True)
        (output / 'qualification.json').write_text(json.dumps(receipt, indent=2) + '\n')

    def latest(path):
        collect_responses()
        matches = [value for value in bodies if value[0] == path]
        assert matches, 'No completed HTTP200 JSON response for ' + path
        return matches[-1]

    def project_url(path, selection=None, **params):
        params['selection'] = json.dumps(project_locator(selection or probe['selection']), separators=(',', ':'))
        return origin + path + '?' + urlencode(params)

    def imported_url(source=None, slug=None):
        params = {'library': 'imported'}
        if source is not None:
            params['source'] = json.dumps(source, separators=(',', ':'))
        if slug is not None:
            params['page'] = slug
        return origin + '/knowledge/explorer?' + urlencode(params)

    def choose(label, value):
        page.get_by_role('combobox', name=label, exact=True).click()
        page.get_by_role('option', name=value, exact=True).click()

    def screenshot(name):
        page.screenshot(path=str(output / (name + '.png')), full_page=False)
        receipt['screenshots'].append(name + '.png')

    def query(mode, metric, imported=False):
        choose('Knowledge search mode', mode)
        choose('Knowledge distance metric', metric)
        page.get_by_label('Search Knowledge pages', exact=True).fill('Foreground' if not imported else 'Imported')
        path = '/api/knowledge/imported/search' if imported else '/api/knowledge/search'
        query_started = time.monotonic()
        with page.expect_response(lambda response: urlsplit(response.url).path == path, timeout=130000) as response:
            page.get_by_role('button', name='Search pages', exact=True).click()
        assert response.value.status == 200
        request, native = latest(path)[1:]
        search = request['input']['search'] if imported else request['input']
        assert search['mode'] == mode and search['metric'] == metric
        if imported:
            assert 'selection' not in request and request['input']['source'] == probe['import_source']
        else:
            assert request['selection'] == probe['selection']
        result = native['native']
        receipt.setdefault('query_http', []).append({'source': 'imported' if imported else 'project', 'mode': mode, 'metric': metric, 'status': response.value.status, 'elapsed_ms': round((time.monotonic() - query_started) * 1000), 'state': result['state'], 'reason': result.get('reason')})
        if mode == 'fts':
            assert result['state'] == 'hits' and result['hits']
        elif result['state'] == 'partial':
            expect(page.get_by_text('Partial Knowledge search results', exact=True)).to_be_visible()
        elif result['state'] == 'unavailable':
            expect(page.get_by_text('Knowledge search unavailable', exact=True)).to_be_visible()
        else:
            assert result['state'] == 'hits'
        return result

    def project_or_task_request(item):
        path = item['path']
        return path.startswith(('/api/tasks/', '/api/memory/', '/api/projects/')) or path in {'/api/projects', '/api/global-projects', '/api/project-catalog', '/api/knowledge/contexts', '/api/knowledge/heads', '/api/knowledge/page', '/api/knowledge/search'}

    def write_memory(user=False, selection=None, first_corpus=False):
        collect_responses()
        mark = len(bodies)
        expected_selection = selection or probe['selection']
        prefix = '/api/user-memory/' if user else '/api/memory/'
        title = 'Browser USER durable memory' if user else 'Browser first PROJECT durable memory' if first_corpus else 'Browser PROJECT durable memory'
        page.get_by_role('button', name='Create memory', exact=True).click()
        dialog = page.get_by_role('dialog', name='Create memory', exact=True)
        dialog.get_by_label('Memory title', exact=True).fill(title)
        dialog.get_by_label('Memory content', exact=True).fill('Original browser guidance')
        operation_started = time.monotonic()
        with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'create', timeout=60000) as response:
            dialog.get_by_role('button', name='Create memory', exact=True).click()
        receipt['writes'].append({'scope': 'user' if user else 'project', 'operation': 'create', 'status': response.value.status, 'elapsed_ms': round((time.monotonic() - operation_started) * 1000)})
        assert response.value.status == 200
        expect(page.get_by_role('heading', name=title, exact=True)).to_be_visible()
        addressed = page.url

        def search_current(expected_title, term=None):
            operation_started = time.monotonic()
            with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'search', timeout=60000) as response:
                page.get_by_label('Search memories', exact=True).fill(term or expected_title)
            receipt['writes'].append({'scope': 'user' if user else 'project', 'operation': 'fts_after_create' if expected_title == title else 'fts_after_update', 'first_corpus': first_corpus, 'status': response.value.status, 'elapsed_ms': round((time.monotonic() - operation_started) * 1000)})
            assert response.value.status == 200
            request, result = latest(prefix + 'search')[1:]
            assert result['native']['state'] == 'published'
            assert any(hit['current']['title'] == expected_title for hit in result['native']['hits'])
            if user:
                assert set(request) == {'input'}
            else:
                assert request['selection'] == expected_selection
            page.get_by_label('Search memories', exact=True).fill('')

        search_current(title)
        record = page.get_by_role('article', name='Memory record', exact=True)
        record.get_by_role('button', name='Edit', exact=True).click()
        dialog = page.get_by_role('dialog', name='Edit memory', exact=True)
        dialog.get_by_label('Memory title', exact=True).fill(title + ' revised')
        dialog.get_by_label('Memory content', exact=True).fill('r12tailpublication Revised browser guidance')
        dialog.get_by_label('Memory revision reason', exact=True).fill('Browser revision verification')
        operation_started = time.monotonic()
        with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'update', timeout=60000) as response:
            dialog.get_by_role('button', name='Save revision', exact=True).click()
        receipt['writes'].append({'scope': 'user' if user else 'project', 'operation': 'update', 'status': response.value.status, 'elapsed_ms': round((time.monotonic() - operation_started) * 1000)})
        assert response.value.status == 200
        expect(page.get_by_role('heading', name=title + ' revised', exact=True)).to_be_visible()
        search_current(title + ' revised', term='r12tailpublication')
        record.get_by_role('button', name='Revision 1', exact=True).click()
        expect(record.get_by_text('Original browser guidance', exact=True)).to_be_visible()
        record.get_by_role('button', name='Revision 2', exact=True).click()
        record.get_by_role('button', name='Remove', exact=True).click()
        removal = page.get_by_role('dialog', name='Remove memory', exact=True)
        removal.get_by_label('Memory removal reason', exact=True).fill('Browser removal verification')
        if not user:
            page.set_viewport_size({'width': 390, 'height': 844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            removal.get_by_label('Memory removal reason', exact=True).focus()
            page.keyboard.press('Tab')
            expect(removal.get_by_role('button', name='Cancel', exact=True)).to_be_focused()
            screenshot('native-memory-removal-narrow-first' if first_corpus else 'native-memory-removal-narrow')
            page.set_viewport_size({'width': 1280, 'height': 800})
        operation_started = time.monotonic()
        with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'delete', timeout=60000) as response:
            removal.get_by_role('button', name='Remove memory', exact=True).click()
        receipt['writes'].append({'scope': 'user' if user else 'project', 'operation': 'delete', 'status': response.value.status, 'elapsed_ms': round((time.monotonic() - operation_started) * 1000)})
        assert response.value.status == 200
        page.goto(addressed)
        expect(page.get_by_text('Memory deleted', exact=True)).to_be_visible()
        with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'search', timeout=60000) as response:
            page.get_by_label('Search memories', exact=True).fill('r12tailpublication')
        assert response.value.status == 200
        current = latest(prefix + 'search')[2]['native']
        assert current['state'] in {'published', 'published_empty'}
        assert not any(hit['current']['title'] == title + ' revised' for hit in current.get('hits', []))
        with page.expect_response(lambda response: urlsplit(response.url).path == prefix + 'search', timeout=60000) as response:
            page.get_by_label('Search retained revisions', exact=True).check()
        assert response.value.status == 200
        retained = latest(prefix + 'search')[2]['native']
        assert retained['state'] == 'published'
        assert any(hit['current']['title'] == title + ' revised' and hit['deleted'] for hit in retained['hits'])
        receipt['writes'].append({'scope': 'user' if user else 'project', 'operation': 'fts_deleted_current_excluded_retained_found', 'first_corpus': first_corpus, 'status': response.value.status})
        collect_responses()
        for path, request, _ in bodies[mark:]:
            if path in {prefix + name for name in ('create', 'update', 'delete')}:
                assert bool(request['input']['idempotency_key'])
                if user:
                    assert set(request) == {'input'}
                else:
                    assert request['selection'] == expected_selection

    try:
        receipt['playwright'] = version('playwright')
        assert receipt['playwright'] == '1.63.0'
        sdk = sync_playwright().start()
        browser = sdk.chromium.launch(executable_path=args.browser, headless=True)
        receipt['chromium'] = browser.version
        context = browser.new_context(viewport={'width': 1280, 'height': 800})
        context.set_default_timeout(20000)
        expect.set_options(timeout=20000)
        page = context.new_page()
        page.on('response', observed)
        page.on('request', lambda request: requests.append({'path': urlsplit(request.url).path, 'method': request.method}) if urlsplit(request.url).path.startswith('/api/') else None)
        page.on('requestfailed', lambda request: failures.append({'path': urlsplit(request.url).path, 'failure': request.failure}) if urlsplit(request.url).path.startswith('/api/') else None)
        step('actual_shipping_bundle')
        response = page.request.get(origin + '/knowledge/explorer')
        shipping = Path(__file__).resolve().parents[1] / 'crates/graphit-app/assets/ui/index.html'
        assert response.status == 200 and hashlib.sha256(response.body()).digest() == hashlib.sha256(shipping.read_bytes()).digest()
        receipt['bundle_sha256'] = hashlib.sha256(response.body()).hexdigest()
        passed()

        step('signed_oidc_consumer_entry')
        entry = '/knowledge/contexts' if args.records_focus == 'knowledge-imported' else '/knowledge/explorer?library=imported' if args.records_focus in {'knowledge-controls', 'knowledge-denial'} else '/knowledge/explorer' if args.records_focus == 'knowledge' else '/memory/explorer/project'
        page.goto(origin + entry)
        page.get_by_role('button', name=re.compile(r'^Entrar com ')).click()
        provider = page.get_by_role('menuitem', name='broker', exact=True)
        if provider.count():
            provider.click()
        expect(page.get_by_role('button', name='Sair da conta', exact=True)).to_be_visible()
        pending.clear()  # Anonymous responses belong to the completed sign-in page.
        passed()

        if args.records_focus not in {'knowledge', 'knowledge-imported', 'knowledge-controls', 'knowledge-denial'}:
            step('own_module_contexts_with_task_denied')
            probe_path.with_suffix('.deny-task').write_text('deny task\n')
            until = time.monotonic() + 10
            while not probe_path.with_suffix('.task-denied').exists() and time.monotonic() < until:
                page.wait_for_timeout(50)
            assert probe_path.with_suffix('.task-denied').exists()
            denied = page.request.post(origin + '/api/tasks/contexts', data={'selection': {}}, headers={'Origin': origin, 'X-Graphit-Request': 'ui'})
            assert denied.status == 403
            receipt['task_read_after_denial'] = denied.status
            page.goto(origin + '/memory/explorer/project')
            expect(page.get_by_role('heading', name='Project memory', exact=True)).to_be_visible()
            assert latest('/api/memory/contexts')[2]['contexts']
            page.goto(origin + '/knowledge/explorer')
            expect(page.get_by_role('navigation', name='Published Knowledge pages', exact=True)).to_be_visible()
            assert latest('/api/knowledge/contexts')[2]['contexts']
            passed()

            step('project_first_memory_publication_and_fts')
            first_selection = probe['additional_selections'][0]
            page.goto(project_url('/memory/explorer/project', selection=first_selection))
            expect(page.get_by_text('No Memory corpus has been published', exact=True)).to_be_visible()
            assert latest('/api/memory/list')[2]['native']['state'] == 'no_corpus'
            write_memory(selection=first_selection, first_corpus=True)
            passed()

            step('same_identity_different_native_path_exact_selection')
            mirror = probe['mirror_selection']
            page.goto(origin + '/memory/explorer/project/' + probe['mirror_memory_id'])
            def choose_mirror():
                page.get_by_role('group', name='Working context', exact=True).get_by_role('combobox', name='Project', exact=True).click()
                page.get_by_role('option').filter(has_text=mirror['project_dir']).click()
            choose_mirror()
            expect(page.get_by_role('heading', name='Mirror memory', exact=True)).to_be_visible()
            assert latest('/api/memory/history')[1]['selection'] == mirror
            selection = parse_qs(urlsplit(page.url).query)['selection'][0]
            assert json.loads(selection) == project_locator(mirror)
            page.reload()
            choose_mirror()
            expect(page.get_by_role('heading', name='Mirror memory', exact=True)).to_be_visible()
            expect(page.get_by_role('heading', name='Revised foreground memory', exact=True)).to_have_count(0)
            passed()

            step('user_memory_zero_project_io_and_web_crud')
            mark = len(requests)
            assert probe['user_memory_id'] is None
            page.goto(origin + '/memory/explorer/user?' + urlencode({'scope_id': probe['user_scope_id']}))
            expect(page.get_by_text('No Memory corpus has been published', exact=True)).to_be_visible()
            assert latest('/api/user-memory/list')[2]['native']['state'] == 'no_corpus'
            write_memory(user=True, first_corpus=True)
            relevant = requests[mark:]
            assert not any(item['path'].startswith(('/api/tasks/', '/api/memory/', '/api/knowledge/')) for item in relevant)
            collect_responses()
            assert all('selection' not in body for path, body, _ in bodies if path.startswith('/api/user-memory/'))
            passed()

        if args.records_focus != 'memory':
            if args.records_focus not in {'knowledge-imported', 'knowledge-controls', 'knowledge-denial'}:
                step('project_knowledge_hybrid_gap')
                page.goto(project_url('/knowledge/explorer'))
                expect(page.get_by_role('navigation', name='Published Knowledge pages', exact=True)).to_be_visible()
                result = query('hybrid', 'dot')
                receipt['query'] = [{'mode': 'hybrid', 'metric': 'dot', 'state': result['state'], 'reason': result.get('reason'), 'hits': len(result.get('hits', []))}]
                passed()

                step('knowledge_published_empty_and_absent_states')
                empty = probe['additional_selections'][0]
                page.goto(project_url('/knowledge/explorer', selection=empty))
                expect(page.get_by_text('The Knowledge publication is empty', exact=True)).to_be_visible()
                page.goto(project_url('/knowledge/explorer', page='absent literal slug'))
                expect(page.get_by_text('Knowledge page not found', exact=True)).to_be_visible()
                passed()

            if args.records_focus not in {'knowledge-controls', 'knowledge-denial'}:
                step('imported_inventory_heads_query_page_reload_zero_project_io')
                mark = len(requests)
                page.goto(origin + '/knowledge/contexts')
                expect(page.get_by_text('Select an installed artifact', exact=True)).to_be_visible()
                page.reload()
                expect(page.get_by_text('Select an installed artifact', exact=True)).to_be_visible()
                page.get_by_role('combobox', name='Imported Knowledge artifact', exact=True).click()
                source = probe['import_source']
                label = source['artifact']['id'] + ' @ ' + source['artifact']['version']
                if source['artifact'].get('context_alias'):
                    label += ' · ' + source['artifact']['context_alias']
                listbox = page.get_by_role('listbox', name='Imported Knowledge artifact', exact=True)
                option = listbox.get_by_role('option', name=label, exact=True)
                receipt['imported_option'] = {'expected_label': label, 'option_count': option.count()}
                expect(option).to_have_count(1)
                option.click()
                expect(page.get_by_role('navigation', name='Published imported Knowledge pages', exact=True)).to_be_visible()
                assert latest('/api/knowledge/imported/heads')[1] == {'input': {'source': probe['import_source']}}
                assert json.loads(parse_qs(urlsplit(page.url).query)['source'][0]) == probe['import_source']
                for mode, metric in [('semantic', 'l2'), ('hybrid', 'dot')]:
                    query(mode, metric, imported=True)
                page.get_by_role('navigation', name='Published imported Knowledge pages', exact=True).get_by_role('button', name=re.compile('^Imported namespace ')).click()
                expect(page.get_by_role('article', name='Imported Knowledge document', exact=True)).to_contain_text('Complete foreground reference page')
                page.reload()
                expect(page.get_by_role('article', name='Imported Knowledge document', exact=True)).to_contain_text('Complete foreground reference page')
                assert latest('/api/knowledge/imported/page')[1] == {'input': {'source': probe['import_source'], 'slug': probe['import_slug']}}
                assert not any(project_or_task_request(item) or item['path'].startswith('/api/user-memory/') for item in requests[mark:])
                receipt['imported_io'] = {'start': mark, 'end': len(requests), 'requests': requests[mark:], 'contexts_direct_reload': True}
                passed()

                step('project_knowledge_absent_literal_page')
                page.goto(project_url('/knowledge/explorer', page='absent literal slug'))
                expect(page.get_by_text('Knowledge page not found', exact=True)).to_be_visible()
                assert latest('/api/knowledge/page')[1] == {'selection': probe['selection'], 'input': {'slug': 'absent literal slug'}}
                receipt['project_absent_page'] = {'http_status': 200, 'literal_slug': 'absent literal slug', 'selection_exact': True}
                passed()

            if args.records_focus != 'knowledge-denial':
                step('accepted_http_query_cancel_terminal')
                page.goto(project_url('/knowledge/explorer'))
                expect(page.get_by_role('navigation', name='Published Knowledge pages', exact=True)).to_be_visible()
                probe_path.with_suffix('.hold').write_text('hold\n')
                until = time.monotonic() + 10
                while not probe_path.with_suffix('.armed').exists() and time.monotonic() < until:
                    page.wait_for_timeout(50)
                assert probe_path.with_suffix('.armed').exists()
                page.get_by_label('Search Knowledge pages', exact=True).fill('Foreground')
                page.get_by_role('button', name='Search pages', exact=True).click()
                until = time.monotonic() + 10
                while not probe_path.with_suffix('.entered').exists() and time.monotonic() < until:
                    page.wait_for_timeout(50)
                assert probe_path.with_suffix('.entered').exists()
                failed_mark = len(failures)
                page.get_by_role('button', name='Cancel search', exact=True).click()
                probe_path.with_suffix('.release').write_text('release\n')
                until = time.monotonic() + 10
                while not any(item['path'] == '/api/knowledge/search' for item in failures[failed_mark:]) and time.monotonic() < until:
                    page.wait_for_timeout(50)
                assert any(item['path'] == '/api/knowledge/search' for item in failures[failed_mark:]), 'accepted query must reach client abort terminal'
                receipt['accepted_cancel'] = {'issuer_entered': True, 'released': True, 'client_abort_terminal': True}
                expect(page.get_by_text('Search request stopped.', exact=False)).to_be_visible()
                expect(page.get_by_role('navigation', name='Knowledge search results', exact=True)).to_have_count(0)
                passed()

                step('real_imported_reply_held_then_cancel_delivery_attempt')
                page.goto(imported_url(probe['import_source']))
                expect(page.get_by_role('navigation', name='Published imported Knowledge pages', exact=True)).to_be_visible()
                page.get_by_label('Search Knowledge pages', exact=True).fill('Imported')
                held = []
                page.route('**/api/knowledge/imported/search', lambda route: held.append((route, route.fetch(timeout=130000))))
                page.get_by_role('button', name='Search pages', exact=True).click()
                until = time.monotonic() + 130
                while not held and time.monotonic() < until:
                    page.wait_for_timeout(50)
                assert held and held[0][1].status == 200, 'real server must answer before Cancel'
                native = held[0][1].json()['native']
                assert native['state'] == 'hits' and native['hits']
                failed_mark = len(failures)
                page.get_by_role('button', name='Cancel search', exact=True).click()
                expect(page.get_by_text('Search request stopped.', exact=False)).to_be_visible()
                until = time.monotonic() + 10
                while not any(item['path'] == '/api/knowledge/imported/search' for item in failures[failed_mark:]) and time.monotonic() < until:
                    page.wait_for_timeout(50)
                assert any(item['path'] == '/api/knowledge/imported/search' for item in failures[failed_mark:])
                delivery = 'fulfilled_after_cancel'
                try:
                    held[0][0].fulfill(response=held[0][1])
                except PlaywrightError:
                    delivery = 'browser_discarded_after_abort'
                page.wait_for_timeout(100)
                expect(page.get_by_text('Search request stopped.', exact=False)).to_be_visible()
                expect(page.get_by_role('navigation', name='Knowledge search results', exact=True)).to_have_count(0)
                page.unroute('**/api/knowledge/imported/search')
                receipt['late_reply'] = {'real_server_status': 200, 'real_state': native['state'], 'real_hits': len(native['hits']), 'client_abort_terminal': True, 'delivery_attempt': delivery, 'results_rendered': False}
                passed()

                step('user_read_zero_project_discovery')
                mark = len(requests)
                with page.expect_response(lambda response: urlsplit(response.url).path == '/api/user-memory/list') as response:
                    page.goto(origin + '/memory/explorer/user')
                assert response.value.status == 200
                expect(page.get_by_role('button', name='Create memory', exact=True)).to_be_visible()
                collect_responses()
                assert not any(project_or_task_request(item) for item in requests[mark:])
                receipt['user_read_io'] = {'start': mark, 'end': len(requests), 'requests': requests[mark:], 'status': 200}
                passed()

            step('narrow_viewport_keyboard_and_native_denial')
            page.set_viewport_size({'width': 390, 'height': 844})
            page.goto(imported_url(probe['import_source'], probe['import_slug']))
            expect(page.get_by_role('article', name='Imported Knowledge document', exact=True)).to_be_visible()
            width = page.evaluate('({width:innerWidth,scroll:document.documentElement.scrollWidth})')
            assert width['scroll'] <= width['width']
            page.get_by_label('Search Knowledge pages', exact=True).focus()
            page.keyboard.press('Tab')
            assert page.locator(':focus').count() == 1
            screenshot('native-records-narrow')
            with page.expect_response(lambda response: urlsplit(response.url).path == '/auth/logout') as logout:
                page.get_by_role('button', name='Sair da conta', exact=True).click()
            assert logout.value.status == 204
            page.wait_for_url(origin + '/task/gantt', wait_until='domcontentloaded')
            expect(page.get_by_role('button', name=re.compile(r'^Entrar com '))).to_be_visible()
            page.goto(imported_url(probe['import_source'], probe['import_slug']))
            expect(page.get_by_text('Unable to read imported document', exact=True)).to_be_visible()
            expect(page.get_by_role('article', name='Imported Knowledge document', exact=True).get_by_role('heading', name='Imported namespace', exact=True)).to_have_count(0)
            assert any(item['path'] == '/api/knowledge/imported/page' and item['status'] == 401 for item in receipt['http'])
            receipt['denial'] = {'logout_http_status': 204, 'logout_redirect_observed': '/task/gantt', 'imported_page_http_status': 401, 'old_document_heading_rendered': False}
            assert not legacy
            passed()
        receipt['status'] = 'passed'
    except Exception as error:
        receipt['status'] = 'failed'
        frames = traceback.extract_tb(error.__traceback__)
        receipt['steps'].append({'name': stage, 'status': 'failed', 'error_type': type(error).__name__, 'runner_line': next(frame.lineno for frame in reversed(frames) if frame.filename == __file__)})
        print(stage + ': failed (' + type(error).__name__ + ')', flush=True)
        (output / 'qualification.json').write_text(json.dumps(receipt, indent=2) + '\n')
        if browser and not stage.startswith('signed_oidc'):
            try:
                screenshot('failure')
            except Exception:
                pass
    finally:
        if browser:
            if page:
                page.remove_listener('response', observed)
                collect_responses()
            browser.close()
        if sdk:
            sdk.stop()
        receipt['elapsed_ms'] = round((time.monotonic() - beginning) * 1000)
        receipt['legacy_http'] = legacy
        receipt['request_failures'] = failures
        receipt['requests'] = requests
        (output / 'qualification.json').write_text(json.dumps(receipt, indent=2) + '\n')
        if not args.keep_host:
            probe_path.with_suffix('.done').write_text('browser finished\n')
    return 0 if receipt['status'] == 'passed' else 1
