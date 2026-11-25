import { afterEach, expect, it, vi } from 'vitest'
import { readNow, NowReadError, type NowUpdate } from './now'
import { stringifyNativeJson } from './nativeJson'
const selection = { project_id: 'project', project_version: 'branch/ação', project_dir: "/ÿ" }
const value: NowUpdate = { selection, snapshot: { type: 'snapshot', project_id: 'project', instances: [] } }
afterEach(() => vi.unstubAllGlobals())
function response() {
 let controller!: ReadableStreamDefaultController<Uint8Array>
 const cancel = vi.fn()
 const body = new ReadableStream<Uint8Array>({ start: value => { controller = value }, cancel })
 vi.stubGlobal('fetch', vi.fn(async () => new Response(body, { headers: { 'Content-Type': 'application/x-ndjson' } })))
 return { controller, cancel }
}
it('decodes split native UTF-8 snapshots incrementally and cancels an idle reader on abort', async () => {
 const { controller, cancel } = response(), abort = new AbortController(), updates: NowUpdate[] = []
 const result = readNow(selection, abort.signal, update => { updates.push(update) })
 const wire = new TextEncoder().encode(stringifyNativeJson(value) + '\n')
 const split = wire.indexOf(195) + 1
 controller.enqueue(wire.slice(0, split)); controller.enqueue(wire.slice(split))
 await vi.waitFor(() => expect(updates).toEqual([value]))
 expect(vi.mocked(fetch).mock.calls[0][1]).toMatchObject({ method: 'POST', credentials: 'same-origin', signal: abort.signal })
 expect(JSON.parse(vi.mocked(fetch).mock.calls[0][1]!.body as string)).toEqual({ selection })
 abort.abort(); await result; expect(cancel).toHaveBeenCalledTimes(1)
})
it.each([401,403])('does not read denied streams (%s)', async status => {
 vi.stubGlobal('fetch', vi.fn(async () => new Response('', { status })))
 const update = vi.fn()
 await expect(readNow(selection, new AbortController().signal, update)).rejects.toMatchObject({ status, retryable: false })
 expect(update).not.toHaveBeenCalled()
})
it('rejects another project before delivering any data and releases its reader', async () => {
 const { controller, cancel } = response(), update = vi.fn()
 const result = readNow(selection, new AbortController().signal, update)
 controller.enqueue(new TextEncoder().encode(stringifyNativeJson({ ...value, selection: { ...selection, project_id: 'other' } }) + '\n'))
 await expect(result).rejects.toBeInstanceOf(NowReadError)
 expect(update).not.toHaveBeenCalled(); expect(cancel).toHaveBeenCalledTimes(1)
})
it('preserves integer counters beyond JavaScript precision', async () => {
 const { controller } = response(), abort = new AbortController(), update = vi.fn((_: NowUpdate) => abort.abort())
 const snapshot = { ...value, snapshot: { ...value.snapshot, instances: [{ instance_id: 'runtime', version: selection.project_version, now: { project_id: 'project', generated_at: '0', sections: { sessions: { availability: 'available', items: [{ id: 'ses', title: 'Large session', completed_tasks: 9007199254740993n, total_tasks: 9007199254740995n }], total: 1, has_more: false, error: null } } } }] } }
 const result = readNow(selection, abort.signal, update)
 controller.enqueue(new TextEncoder().encode(stringifyNativeJson(snapshot) + '\n'))
 await result
 expect(update.mock.calls[0][0].snapshot.instances[0].now.sections.sessions.items[0].completed_tasks).toBe(9007199254740993n)
})

it('uses Now authorization for native working contexts without requesting Task discovery', async () => {
 const { nativeRecordContexts } = await import('./nativeRecordRead')
 vi.stubGlobal('fetch', vi.fn(async () => new Response(stringifyNativeJson({ contexts: [selection], current: selection }), { headers: { 'Content-Type': 'application/json' } })))
 expect(await nativeRecordContexts('now')).toEqual({ contexts: [selection], current: selection })
 expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1)
 expect(vi.mocked(fetch).mock.calls[0][0]).toMatch(/\/now\/web\/contexts$/)
 expect(JSON.parse(vi.mocked(fetch).mock.calls[0][1]!.body as string)).toEqual({})
})

it('rejects a different retained working copy with the same project and version', async () => {
 const { controller } = response(), update = vi.fn()
 const result = readNow(selection, new AbortController().signal, update)
 controller.enqueue(new TextEncoder().encode(stringifyNativeJson({ ...value, selection: { ...selection, project_dir: "/þ" } }) + '\n'))
 await expect(result).rejects.toMatchObject({ status: 502 })
 expect(update).not.toHaveBeenCalled()
})

it('preserves the original observation timestamp when a snapshot is retransmitted', async () => {
 const { controller, cancel } = response(), abort = new AbortController(), updates: NowUpdate[] = []
 const observed: NowUpdate = { selection, snapshot: { type: 'snapshot', project_id: selection.project_id, instances: [{ instance_id: 'runtime', version: selection.project_version, now: { project_id: selection.project_id, generated_at: '1792152000000000123', sections: {} } }] } }
 const result = readNow(selection, abort.signal, data => { updates.push(data); if (updates.length === 2) abort.abort() })
 const wire = stringifyNativeJson(observed) + '\n'
 controller.enqueue(new TextEncoder().encode(wire)); controller.enqueue(new TextEncoder().encode(wire))
 await result
 expect(updates).toEqual([observed, observed])
 expect(updates.map(data => data.snapshot.instances[0].now.generated_at)).toEqual(['1792152000000000123', '1792152000000000123'])
 expect(cancel).toHaveBeenCalledTimes(1)
})
