import { afterEach, expect, it, vi } from 'vitest'
import { sessionApi } from './taskSession'
afterEach(() => vi.unstubAllGlobals())
it('uses the same native selector for heads/exact Session with per-request cancellation', async () => {
 const fetcher = vi.fn().mockImplementation(async () => new Response('[]')); vi.stubGlobal('fetch', fetcher)
 const selection = { project_dir: "/p", project_id: 'project1', project_version: 'main' }; const controller = new AbortController()
 await sessionApi.heads(selection, controller.signal); await sessionApi.get(selection, 'session1', { signal: controller.signal })
 expect(fetcher.mock.calls.map(call => call[0])).toEqual(['/api/tasks/session-heads', '/api/tasks/session'])
 expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: { id: 'session1' } })
 expect(fetcher.mock.calls[1][1].signal).toBe(controller.signal)
})

it('sends an explicit opaque history page without changing the native selection', async () => {
 const fetcher = vi.fn().mockImplementation(async () => new Response('null')); vi.stubGlobal('fetch', fetcher)
 const selection = { project_dir: '/p/ ÿ', project_version: ' literal/version ' }; const controller = new AbortController()
 await sessionApi.get(selection, 'session1', { signal: controller.signal, history: { page_size: 20, cursor: 'opaque-native-session-pin' } })
 expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { id: 'session1', history: { page_size: 20, cursor: 'opaque-native-session-pin' } } })
 expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
})
