import { afterEach, describe, expect, it, vi } from 'vitest'
import { taskApi } from './task'
import { nativeRecordContexts } from './nativeRecordRead'
import { sessionApi } from './taskSession'
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'feature' }
afterEach(() => vi.unstubAllGlobals())
describe('native Task browser transport', () => {
 it.each(['task', 'ast', 'knowledge', 'memory', 'now', 'hub'] as const)('retains %s inventory names separately from the exact context', async module => {
  const inventory = { contexts: [selection], current: selection, projects: [{ project_id: selection.project_id, name: 'Graphit Code' }] }
  const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify(inventory))); vi.stubGlobal('fetch', fetcher)
  const result = module === 'task' ? await taskApi.contexts() : await nativeRecordContexts(module)
  expect(result).toEqual(inventory)
  expect(Object.keys(result.contexts[0])).toEqual(['project_dir', 'project_id', 'project_version'])
  const path = module === 'task' ? '/api/tasks/contexts' : module === 'now' ? '/api/now/web/contexts' : '/api/' + module + '/contexts'
  expect(fetcher.mock.calls[0][0]).toBe(path)
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual(module === 'task' ? { selection: {} } : module === 'memory' || module === 'knowledge' ? { input: {} } : {})
 })
 it('uses JSON selection and echoes Unicode paths/cursors unchanged', async () => {
  const cursor = { root: selection.project_dir, project_version: 'feature', version: 7, filter: { status: 'open' as const }, offset: 20 }
  const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ results: [], next_cursor: cursor }))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  await taskApi.catalogue({ selection, filter: { status: 'open' }, cursor, page_size: 20, signal: controller.signal })
  expect(fetcher).toHaveBeenCalledWith('/api/tasks/catalogue', expect.objectContaining({ credentials: 'same-origin', signal: controller.signal, method: 'POST' }))
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { filter: { status: 'open' }, page_size: 20, cursor } })
  expect(fetcher.mock.calls[0][1].headers['X-Graphit-Request']).toBe('ui')
 })
 it('uses the authenticated Task search route and preserves the native mode and continuation', async () => {
  const cursor = { binding: { mode: 'semantic', semantic: { identity: { dimensions: 2 }, query_vector: [0.25, 0.75] } }, offset: 1 }
  const task = { id: 'task1', title: 'Publication' }
  const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ Published: { generation: { version: 4 }, results: [{ task, score: 0.9, location: { Specification: { revision: 1 } } }], next_cursor: cursor, total_hits: 2 } })))
  vi.stubGlobal('fetch', fetcher)
  const page = await taskApi.search({ selection, text: 'publication', mode: 'semantic', filter: { flagged: false } })
  expect(page.results).toEqual([task]); expect(page.next_cursor).toEqual(cursor)
  expect(page.matches.task1).toEqual({ score: 0.9, location: { Specification: { revision: 1 } } }); expect(page.total_hits).toBe(2)
  await taskApi.search({ selection, text: 'publication', mode: 'semantic', filter: { flagged: false }, cursor: page.next_cursor })
  expect(fetcher).toHaveBeenLastCalledWith('/api/tasks/search', expect.objectContaining({ credentials: 'same-origin' }))
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: { mode: 'semantic', text: 'publication', filter: { flagged: false }, page_size: 20, top_k: 100, cursor } })
 })
 it('reads captured context, exact Task and Session through actual native routes', async () => {
  const fetcher = vi.fn().mockImplementation(async () => new Response('null')); vi.stubGlobal('fetch', fetcher)
  await taskApi.context({ project_id: 'project1', project_version: 'feature' })
  await taskApi.get(selection, 'task1'); await sessionApi.heads(selection); await sessionApi.get(selection, 'session1')
  expect(fetcher.mock.calls.map(call => call[0])).toEqual(['/api/tasks/context', '/api/tasks/get', '/api/tasks/session-heads', '/api/tasks/session'])
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection: { project_id: 'project1', project_version: 'feature' } })
 })
 it('explains a disabled embedding service without running a textual fallback', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response('{"error":{"code":"model_disabled"}}', { status: 503 }))
  vi.stubGlobal('fetch', fetcher)
  await expect(taskApi.search({ selection, text: 'publication', mode: 'semantic' })).rejects.toThrow('Embedding is disabled')
  expect(fetcher).toHaveBeenCalledTimes(1)
 })
 it('preserves HTTP denial as an error instead of returning an empty catalogue', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403 }))
  await expect(taskApi.context()).rejects.toThrow('HTTP 403')
 })
 it('omits audit reads by default and echoes pinned section cursors without decoding them', async () => {
  const fetcher = vi.fn().mockImplementation(async () => new Response('{"generation":{"version":18446744073709551615,"physical_branch":null,"inherited_target":null},"history":null,"comments":null}'))
  vi.stubGlobal('fetch', fetcher)
  const current = await taskApi.get(selection, 'task1')
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { id: 'task1' } })
  expect(current?.history).toBeNull()
  expect(current?.comments).toBeNull()
  expect(current?.generation).toEqual({ version: 18446744073709551615n, physical_branch: null, inherited_target: null })
  const controller = new AbortController()
  const history = { page_size: 20, cursor: 'opaque/history+Ω==/snapshot' }
  const comments = { page_size: 20, cursor: 'opaque/comments+Ω==/snapshot' }
  await taskApi.get(selection, 'task1', { history, comments, signal: controller.signal })
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: { id: 'task1', history, comments } })
  expect(fetcher.mock.calls[1][1].signal).toBe(controller.signal)
 })
  it('preserves explicit boolean false and binds the same filters to the cursor payload', async () => {
  const filter = { flagged: false, blocked: true, status: 'open' as const }
  const cursor = { root: selection.project_dir, project_version: 'feature', version: 9, filter, offset: 1 }
  const fetcher = vi.fn().mockImplementation(async () => new Response('{"results":[],"next_cursor":null}')); vi.stubGlobal('fetch', fetcher)
  await taskApi.catalogue({ selection, filter, cursor, page_size: 1 })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { filter, page_size: 1, cursor } })
  })

  it('echoes the actual numeric u64 cursor without quotes or rounding', async () => {
   const text = '{"results":[],"next_cursor":{"root":"/projetos espaço/café Ω","project_version":"feature","version":18446744073709551615,"filter":{"flagged":false},"offset":9007199254740993}}'
   const fetcher = vi.fn().mockImplementation(async () => new Response(text)); vi.stubGlobal('fetch', fetcher)
   const page = await taskApi.catalogue({ selection, filter: { flagged: false } })
   expect(page.next_cursor?.version).toBe(18446744073709551615n)
   expect(page.next_cursor?.offset).toBe(9007199254740993n)
   await taskApi.catalogue({ selection, filter: { flagged: false }, cursor: page.next_cursor })
   const body: string = fetcher.mock.calls[1][1].body
   expect(body).toContain('"version":18446744073709551615')
   expect(body).toContain('"offset":9007199254740993')
   expect(body).not.toContain('18446744073709552000')
   expect(body).not.toContain('"version":"')
  })

  it('shares lossless parsing with Session and propagates malformed responses and aborted transport', async () => {
   const fetcher = vi.fn().mockImplementationOnce(async () => new Response('{"session":{"revision":18446744073709551615}}'))
     .mockImplementationOnce(async () => new Response('{'))
     .mockRejectedValueOnce(new DOMException('Request aborted', 'AbortError'))
   vi.stubGlobal('fetch', fetcher)
   expect((await sessionApi.get(selection, 'session1'))?.session.revision).toBe(18446744073709551615n)
   await expect(taskApi.get(selection, 'task1')).rejects.toThrow()
   const controller = new AbortController(); controller.abort()
   await expect(taskApi.get(selection, 'task1', { signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' })
   expect(fetcher.mock.calls[2][1].signal).toBe(controller.signal)
  })

})
