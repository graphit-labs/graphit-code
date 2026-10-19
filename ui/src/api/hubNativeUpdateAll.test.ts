import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, updateAllCompleted, updateAllNeedsInspection, type UpdateAllOutcome } from './hubNative'
import { partialUpdate, updateInput, updateOutcome, updatedItem } from './hubNativeUpdateAll.fixtures'
const response = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })
afterEach(() => vi.unstubAllGlobals())
it.each([false, true])('captures only the exact PROJECT or GlobalCache scope, cache=%s', async cache => {
  const selected = updateInput(cache), original = structuredClone(selected), expected = updateOutcome(original)
  let finish!: (value: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve })); vi.stubGlobal('fetch', fetcher)
  const signal = new AbortController().signal, pending = hubNative.updateAll(selected, signal)
  if (selected.scope.kind === 'project') selected.scope.project_version = 'changed'
  finish(response(expected)); await expect(pending).resolves.toEqual(expected)
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/files\/update$/); expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual(original); expect(Object.keys(JSON.parse(init.body as string)).sort()).toEqual(cache ? ['scope'] : ['binding', 'scope'])
  expect(expected.items[0].outcome).not.toHaveProperty('operation'); expect(expected.items[0].outcome).not.toHaveProperty('scope')
})
it('preserves ordered successes, a sparse failed item and later not_attempted entries from the nested HTTP error', async () => {
  const input = updateInput(), outcome = partialUpdate(input)
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'installation_conflict' }, outcome }, 409)))
  await expect(hubNative.updateAll(input)).rejects.toMatchObject({ status: 409, code: 'installation_conflict', outcome, receiptCorrelated: true, request: input })
  expect(outcome.items[2]).not.toHaveProperty('outcome'); expect(outcome.items[2]).not.toHaveProperty('code'); expect(updateAllNeedsInspection(outcome)).toBe(true)
})
it.each([401, 504])('retains all confirmed item effects after late HTTP%s without claiming global success', async status => {
  const outcome = updateOutcome(); vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: status === 401 ? 'unauthorized' : 'deadline' }, outcome }, status)))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ status, outcome, receiptCorrelated: true })
})
it.each([true, false])('requires original scope inspection when413 omits the item list, JSON=%s', async json => {
  vi.stubGlobal('fetch', vi.fn(async () => json ? response({ error: { code: 'remote_file_too_large' }, effects: 'inspect_selected_target', inspection_required: true }, 413) : new Response(null, { status: 413, headers: { 'X-Graphit-Inspection-Required': 'request-selection' } })))
  await expect(hubNative.updateAll(updateInput(true))).rejects.toMatchObject({ status: 413, outcome: null, inspectionRequired: true, request: updateInput(true) })
})
it('preserves the existing unknown worker summary as observed effects', async () => {
  const effects = { status: 'error', cache: 'not_confirmed', publication: { change: 'Unknown', committed: null }, observation_failed: true }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'file_current_job_failed' }, outcome: effects }, 500)))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ status: 500, outcome: null, effects, request: updateInput() })
})
const invalid: Array<[string, (body: UpdateAllOutcome) => unknown]> = [
  ['crossed scope', body => ({ ...body, scope: { kind: 'global_cache' } })],
  ['crossed global flag', body => ({ ...body, global: true })],
  ['coerced status', body => ({ ...body, status: ['ok'] })],
  ['PROJECT cache target', body => ({ ...body, items: [{ ...body.items[0], target: { kind: 'global_cache' } }] })],
  ['PROJECT other native binding', body => ({ ...body, items: [{ ...body.items[0], target: { kind: 'native_binding', binding: 'cursor' } }] })],
  ['Native Global target', body => ({ ...body, items: [{ ...body.items[0], target: { kind: 'global_native' } }] })],
  ['invented operation/scope per item', body => ({ ...body, items: [{ ...body.items[0], outcome: { ...body.items[0].outcome, operation: 'update', scope: body.scope } }] })],
  ['crossed item identity', body => ({ ...body, items: [{ ...body.items[0], outcome: { ...body.items[0].outcome, id: 'different' } }] })],
  ['ok after not_attempted', body => ({ ...body, status: 'error', items: [{ ...body.items[0], status: 'not_attempted', outcome: undefined }, body.items[0]] })],
  ['not_attempted with fabricated effects', body => ({ ...body, status: 'error', items: [{ ...body.items[0], status: 'not_attempted' }] })],
  ['success with an unattempted item', body => ({ ...body, items: [{ artifact_type: 'skill', id: 'later', producer: 'publisher', previous_version: 'one', target: body.items[0].target, status: 'not_attempted' }] })],
  ['unknown item error code', body => ({ ...partialUpdate(), items: [{ ...body.items[0], status: 'error', outcome: { ...body.items[0].outcome, status: 'error' }, code: 'DebugRustError' }] })],
  ['non-Unicode path rejected', body => ({ ...body, items: [{ ...body.items[0], outcome: { ...body.items[0].outcome, possible_parents: ['\ud800'] } }] })],
  ['object path rejected', body => ({ ...body, items: [{ ...body.items[0], outcome: { ...body.items[0].outcome, possible_parents: [{ os: 'Unix', units: [256] }] } }] })],
]
it.each(invalid)('rejects %s without normalizing a success or inventing an empty result', async (_name, corrupt) => {
  vi.stubGlobal('fetch', vi.fn(async () => response(corrupt(updateOutcome()))))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ code: 'invalid_response', outcome: null, inspectionRequired: true, receiptCorrelated: false, request: updateInput() })
})
it.each([200, 500])('rejects Native flat error compatibility at HTTP%s', async status => {
  vi.stubGlobal('fetch', vi.fn(async () => response(partialUpdate(), status)))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ status, outcome: null, inspectionRequired: true, receiptCorrelated: false })
})
it('keeps a crossed error receipt uncorrelated while preserving actual HTTP/code and original request', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'installation_conflict' }, outcome: partialUpdate(updateInput(true)) }, 409)))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ status: 409, code: 'installation_conflict', outcome: null, inspectionRequired: true, receiptCorrelated: false, request: updateInput() })
})
it('accepts matching native_binding and shared_adapter PROJECT targets, and a neutral empty result', async () => {
  const project = updateInput(), native = { ...updatedItem(project), target: { kind: 'native_binding' as const, binding: 'kiro-v3' as const } }
  vi.stubGlobal('fetch', vi.fn(async () => response(updateOutcome(project, [native])))); await expect(hubNative.updateAll(project)).resolves.toMatchObject({ items: [native] })
  expect(updateAllCompleted(updateOutcome(project, []))).toBe(true); expect(updateAllCompleted(partialUpdate(project))).toBe(false)
  const uncertain = updateOutcome(project); uncertain.items[0].outcome!.publication = { change: 'Unknown', committed: null }; expect(updateAllCompleted(uncertain)).toBe(false)
})
it('preserves causal cancellation without retry', async () => {
  const aborted = new DOMException('Cancelled', 'AbortError'), fetcher = vi.fn(async () => { throw aborted }); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.updateAll(updateInput())).rejects.toBe(aborted); expect(fetcher).toHaveBeenCalledOnce()
})

it.each([false, true])('preserves a correlated early HTTP summary before any items, cache=%s', async cache => {
  const input = updateInput(cache), effects = { status: 'error', operation: 'update', scope: input.scope, cache: 'not_attempted', publication: 'not_attempted' }
  for (const [status, code] of [[499, 'cancelled'], [503, 'unavailable'], [504, 'deadline']] as const) {
    vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code }, outcome: effects }, status)))
    await expect(hubNative.updateAll(input)).rejects.toMatchObject({ status, code, outcome: null, effects, receiptCorrelated: true, inspectionRequired: false, request: input })
  }
})
it.each([
  ['crossed scope', { scope: updateInput(true).scope }],
  ['crossed project version', { scope: { ...updateInput().scope, project_version: 'other' } }],
  ['invented items', { items: [] }],
  ['unknown effects', { cache: 'not_confirmed' }],
  ['invented operation', { operation: 'update_all' }],
  ['extra effect claim', { observation_failed: true }],
])('rejects an early summary with %s', async (_name, change) => {
  const input = updateInput(), effects = { status: 'error', operation: 'update', scope: input.scope, cache: 'not_attempted', publication: 'not_attempted', ...change }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'deadline' }, outcome: effects }, 504)))
  await expect(hubNative.updateAll(input)).rejects.toMatchObject({ status: 504, code: 'deadline', outcome: null, effects: null, receiptCorrelated: false, inspectionRequired: true })
})

it.each([false, true])('does not rescue an uncorrelated summary through generic effects, parallel=%s', async parallel => {
  const effects = { cache: 'not_attempted', publication: 'not_attempted' }
  const outcome = { status: 'error', ...(parallel ? { operation: 'update' } : {}), scope: updateInput(true).scope, ...effects }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'unavailable' }, outcome, ...(parallel ? { effects } : {}) }, 503)))
  await expect(hubNative.updateAll(updateInput())).rejects.toMatchObject({ outcome: null, effects: null, receiptCorrelated: false, inspectionRequired: true, notAttempted: false })
})
