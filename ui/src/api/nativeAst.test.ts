import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeAstApi, astSelection } from './nativeAst'
import { NativeRecordReadError } from './nativeRecordRead'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'
import { selection, nativeQuery, searchReply, neighborhoodPage } from '@/components/ast/nativeAst.test-fixtures'
afterEach(() => vi.unstubAllGlobals())
describe('native AST Web client', () => {
 it('reads only the installation catalogue and exact imported native readers', async () => {
  const fetcher = vi.fn().mockImplementation(() => Promise.resolve(new Response('{}'))); vi.stubGlobal('fetch', fetcher)
  const source = { id: 'app/@literal', version: 'release/1' }, installed = { ...source, context: 'observed', backend: 'native' }, abort = new AbortController()
  const cursor = { fingerprint: [7,2,9], position: neighborhoodPage().edges[0].position }
  const neighborhood = { anchor_label: 'Function', anchor_key: { type: 'int64' as const, value: '9007199254740993' }, relationship: 'CALLS', other_label: 'Function', direction: 'incoming' as const, limit: 10, cursor }
  const input = { path: ' literal/main.rs ', entity: 'Alpha', entity_type: 'Function', start_line: 9007199254740993n, line_numbers: true }
  await nativeAstApi.imports(abort.signal)
  await nativeAstApi.importedSchema(installed, abort.signal)
  await nativeAstApi.importedSample(source, abort.signal)
  await nativeAstApi.importedQuery(source, 'RETURN $n', { n: 9007199254740993n }, abort.signal)
  await nativeAstApi.importedNeighborhood(source, neighborhood, abort.signal)
  await nativeAstApi.importedSource(source, input, abort.signal)
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual(['/api/ast/imports', '/api/ast/imported/schema', '/api/ast/imported/sample', '/api/ast/imported/query', '/api/ast/imported/neighborhood', '/api/ast/imported/source'])
  expect(fetcher.mock.calls.map(([, options]) => parseNativeJson(options.body))).toEqual([{}, { source }, { source }, { source, query: { query: 'RETURN $n', parameters: { n: 9007199254740993n } } }, { source, neighborhood }, { source, input }])
  for (const [, options] of fetcher.mock.calls) {
   expect(options).toMatchObject({ method: 'POST', credentials: 'same-origin', signal: abort.signal })
   expect(options.headers).not.toHaveProperty('Authorization')
   expect(parseNativeJson<Record<string, unknown>>(options.body)).not.toHaveProperty('selection')
  }
 })
 it.each(['fts', 'semantic', 'vector', 'hybrid'] as const)('preserves %s search input, real status and results without a second request', async mode => {
  const source = { id: 'app-ast', version: 'release-1' }, physical = astSelection(selection), abort = new AbortController()
  const reply = { status: 'available', mode, results: [], ...(mode === 'hybrid' ? { vector_status: 'nn_absent' } : {}) }
  const fetcher = vi.fn().mockImplementation(() => Promise.resolve(new Response(stringifyNativeJson(reply)))); vi.stubGlobal('fetch', fetcher)
  const search = { query: ' literal query ', mode, k: 10, metric: 'cosine' as const }
  expect(await nativeAstApi.search(physical, search, abort.signal)).toEqual(reply)
  expect(await nativeAstApi.importedSearch(source, search, abort.signal)).toEqual(reply)
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual(['/api/ast/search', '/api/ast/imported/search'])
  expect(fetcher.mock.calls.map(([, options]) => parseNativeJson(options.body))).toEqual([{ selection: physical, search }, { source, search }])
 })
 it.each([undefined, false, true])('preserves rerank=%s for PROJECT and imported search without replacing observed scores', async rerank => {
  const physical = astSelection(selection), source = { id: 'app-ast', version: 'pinned' }, controller = new AbortController()
  const reply = { ...searchReply, results: [{ ...searchReply.results[0], score: 0.5, lexical_score: 2, distance: 0.25, rerank_score: 0 }] }
  const fetcher = vi.fn().mockImplementation(async () => new Response(stringifyNativeJson(reply))); vi.stubGlobal('fetch', fetcher)
  const search = { query: 'literal', ...(rerank === undefined ? {} : { rerank }) }
  expect(await nativeAstApi.search(physical, search, controller.signal)).toEqual(reply)
  expect(await nativeAstApi.importedSearch(source, search, controller.signal)).toEqual(reply)
  expect(fetcher.mock.calls.map(([, options]) => parseNativeJson(options.body))).toEqual([{ selection: physical, search }, { source, search }])
  for (const [, options] of fetcher.mock.calls) {
   expect(Object.prototype.hasOwnProperty.call(parseNativeJson<{ search: object }>(options.body).search, 'rerank')).toBe(rerank !== undefined)
   expect(options.signal).toBe(controller.signal)
  }
 })
 it('retains an unavailable SDK status returned with HTTP 200 and preserves imported denial', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response('{"status":"index_absent","results":[]}')).mockResolvedValueOnce(new Response('{"error":{"code":"denied"}}', { status: 403 })); vi.stubGlobal('fetch', fetcher)
  const source = { id: 'app-ast', version: 'release-1' }
  expect(await nativeAstApi.importedSearch(source, { query: 'Alpha' })).toEqual({ status: 'index_absent', results: [] })
  await expect(nativeAstApi.importedSource(source, { path: 'main.rs' })).rejects.toMatchObject({ status: 403, code: 'denied' })
  expect(fetcher).toHaveBeenCalledTimes(2)
 })

 it('reads Source through the native POST codec without a physical selection echo', async () => {
  // This precision fixture exercises transport, not the existence of a file with this line count.
  const reply = { origin: 'physical', selection: { source: 'literal source\n', total_lines: 9007199254740995n, start_line: 9007199254740993n, end_line: 9007199254740993n } }
  const fetcher = vi.fn().mockResolvedValue(new Response(stringifyNativeJson(reply))); vi.stubGlobal('fetch', fetcher)
  const abort = new AbortController(), physical = astSelection(selection)
  const input = { path: ' literal/main.rs ', start_line: 9007199254740993n, end_line: 9007199254740993n, line_numbers: false }
  expect(await nativeAstApi.source(physical, input, abort.signal)).toEqual(reply)
  expect(fetcher.mock.calls[0][0]).toBe('/api/ast/source')
  const options = fetcher.mock.calls[0][1]
  expect(parseNativeJson(options.body)).toEqual({ selection: physical, source: input })
  expect(options).toMatchObject({ method: 'POST', credentials: 'same-origin', signal: abort.signal })
  expect(options.body).not.toContain('project_id')
  expect(options.headers).not.toHaveProperty('Authorization')
  expect(Object.prototype.hasOwnProperty.call(reply.selection, 'matches')).toBe(false)
 })
 it.each([403,408,413,500])('preserves Source HTTP %i without a GET fallback', async status => {
  const fetcher = vi.fn().mockResolvedValue(new Response('actual source failure', { status })); vi.stubGlobal('fetch', fetcher)
  await expect(nativeAstApi.source(astSelection(selection), { path: 'main.rs' })).rejects.toMatchObject({ status })
  expect(fetcher).toHaveBeenCalledTimes(1)
  expect(fetcher.mock.calls[0][0]).toBe('/api/ast/source')
 })
 it('uses its own cookie-authenticated discovery and closed bodies with exact physical selection', async () => {
  const fetcher = vi.fn().mockImplementation(() => Promise.resolve(new Response('{}'))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController(), physical = astSelection(selection)
  await nativeAstApi.contexts(controller.signal)
  await nativeAstApi.schema(physical, controller.signal)
  await nativeAstApi.sample(physical, controller.signal)
  await nativeAstApi.query(physical, 'RETURN $n', { n: 9007199254740993n }, controller.signal)
  const cursor = { fingerprint: [9,2,7], position: neighborhoodPage().edges[0].position }
  const input = { anchor_label: 'Function', anchor_key: { type: 'string' as const, value: 'real-key' }, relationship: 'CALLS', other_label: 'Function', direction: 'incoming' as const, limit: 100, cursor }
  await nativeAstApi.neighborhood(physical, input, controller.signal)
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual(['/api/ast/contexts','/api/ast/schema','/api/ast/sample','/api/ast/query','/api/ast/neighborhood'])
  const bodies = fetcher.mock.calls.map(([,options]) => parseNativeJson(options.body))
  expect(bodies).toEqual([{}, { selection: physical }, { selection: physical }, { selection: physical, query: { query: 'RETURN $n', parameters: { n: 9007199254740993n } } }, { selection: physical, neighborhood: input }])
  for (const [,options] of fetcher.mock.calls) { expect(options).toMatchObject({ method: 'POST', credentials: 'same-origin', signal: controller.signal }); expect(options.headers).not.toHaveProperty('Authorization'); expect(options.headers).not.toHaveProperty('Origin'); expect(options.body).not.toContain('project_id') }
 })
 it('returns the native typed query without flattening or rounding SDK cells', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(nativeQuery))))
  const query = await nativeAstApi.query(astSelection(selection), 'RETURN n')
  expect(query).toEqual(nativeQuery); expect(query.rows[0][1]).toEqual({ type: 'INT64', value: '9007199254740993' })
 })
 it.each([400,401,403,404,409,503,504])('preserves HTTP %i failures without a sample fallback', async status => {
  const fetcher = vi.fn().mockImplementation(() => Promise.resolve(new Response('actual graph failure', { status }))); vi.stubGlobal('fetch', fetcher)
  await expect(nativeAstApi.sample(astSelection(selection))).rejects.toMatchObject({ status })
  try { await nativeAstApi.schema(astSelection(selection)) } catch (cause) { expect(cause).toBeInstanceOf(NativeRecordReadError) }
  expect(fetcher).toHaveBeenCalledTimes(2)
 })
})

it('omits neighborhood filters for a combined page and round-trips the server cursor without interpretation', async () => {
 const cursor={fingerprint:Array(32).fill(7),position: neighborhoodPage().edges[0].position};
 const reply={anchor:{table:1,row:'9'},nodes:[],edges:[],next_cursor:cursor};
 const fetcher=vi.fn().mockImplementation(async()=>new Response(stringifyNativeJson(reply)));vi.stubGlobal('fetch',fetcher);
 const physical=astSelection(selection),source={id:'observed',version:'literal/version'},controller=new AbortController();
 const first={anchor_label:'Function',anchor_key:{type:'string' as const,value:'opaque-key'},limit:100};
 const project=await nativeAstApi.neighborhood(physical,first,controller.signal);
 await nativeAstApi.importedNeighborhood(source,{...first,cursor:project.next_cursor},controller.signal);
 const bodies=fetcher.mock.calls.map(([,options])=>parseNativeJson(options.body));
 expect(bodies).toEqual([{selection:physical,neighborhood:first},{source,neighborhood:{...first,cursor}}]);
 expect(project).toEqual(reply);expect(project).not.toHaveProperty('generation');
 expect(fetcher.mock.calls.map(([url])=>url)).toEqual(['/api/ast/neighborhood','/api/ast/imported/neighborhood']);
});

it.each([
 { type: 'string' as const, value: 'literal/"entity-key"' },
 { type: 'int64' as const, value: '-9223372036854775808' },
])('preserves typed Source PK $type for PROJECT and imported readers', async entity_key => {
 const reply = { origin: 'indexed' as const, source_sha256: 'backend-only-fingerprint', selection: { source: 'selected literal code', total_lines: 12, start_line: 8, end_line: 9 } }
 const fetcher = vi.fn().mockImplementation(async () => new Response(stringifyNativeJson(reply))); vi.stubGlobal('fetch', fetcher)
 const physical = astSelection(selection), imported = { id: 'literal/context', version: 'exact' }, controller = new AbortController()
 const input = { path: ' literal/ß.rs ', entity_key, entity_type: 'Function' }
 expect(await nativeAstApi.source(physical, input, controller.signal)).toEqual(reply)
 expect(await nativeAstApi.importedSource(imported, input, controller.signal)).toEqual(reply)
 expect(fetcher.mock.calls.map(([, options]) => parseNativeJson(options.body))).toEqual([{ selection: physical, source: input }, { source: imported, input }])
 for (const [, options] of fetcher.mock.calls) expect(options.signal).toBe(controller.signal)
})
