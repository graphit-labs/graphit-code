import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, type HubUnpublishInput } from './hubNative'
const input = (): HubUnpublishInput => ({ provider: 'corporate', producer: '01KSH1CRFFG8Z74B5ZS78WW808', artifact_type: 'ast', id: 'vendor/code', target: { kind: 'version', version: 'release/one', expected_sha256: 'a'.repeat(64) } })
const receipt = () => ({ producer: input().producer, artifact_type: 'ast', id: 'vendor/code', targeted_versions: ['release/one'], desired_latest: null, catalog: 'updated' })
const response = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })
afterEach(() => vi.unstubAllGlobals())
it('captures and posts only the remote DTO with same-origin authorization and its own unpublished receipt', async () => {
  let finish!: (value: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve })); vi.stubGlobal('fetch', fetcher)
  const selected = input(), signal = new AbortController().signal, pending = hubNative.unpublish(selected, signal)
  selected.id = 'other'; selected.target = { kind: 'artifact' }
  finish(response({ status: 'unpublished', retraction: receipt() }))
  await expect(pending).resolves.toMatchObject({ status: 'unpublished', retraction: receipt() })
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/unpublish$/)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual(input())
  expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
})
it('accepts the actual whole-artifact versions including concurrent releases absent from the old UI catalogue', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => response({ status: 'unpublished', retraction: { ...receipt(), targeted_versions: ['release/one', 'concurrent/two'], catalog: 'removed' } })))
  await expect(hubNative.unpublish({ ...input(), target: { kind: 'artifact' } })).resolves.toMatchObject({ retraction: { catalog: 'removed', desired_latest: null, targeted_versions: ['release/one', 'concurrent/two'] } })
})
it.each([{ producer: 'other' }, { artifact_type: 'skill' }, { id: 'other' }, { targeted_versions: ['other'] }, { targeted_versions: [] }, { targeted_versions: ['release/one', 'release/one'] }, { targeted_versions: ['latest'] }, { desired_latest: 'release/one' }, { desired_latest: 42 }, { catalog: 'uncertain' }, { catalog: 'removed', desired_latest: 'other' }])('rejects crossed or unconfirmed receipts: %j', async delta => {
  vi.stubGlobal('fetch', vi.fn(async () => response({ status: 'unpublished', retraction: { ...receipt(), ...delta } })))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({ status: 200, code: 'invalid_response', inspectionRequired: true, request: input() })
})
it.each([401,503,504])('preserves an actual unpublished receipt in a failed HTTP%s delivery', async status => {
  const outcome = { status: 'unpublished', retraction: receipt() }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: status === 401 ? 'unauthorized' : 'unavailable' }, outcome }, status)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({ status, effects: outcome, inspectionRequired: false, request: input() })
})
it.each([400,403,404,409,499,501])('keeps finite HTTP%s failure and not-attempted effects distinct from success', async status => {
  const outcome = { status: 'error', retraction: { catalog: 'not_attempted' }, stage: 'catalog_read', cause: 'permission_denied' }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'hub_unpublish_failed' }, outcome },status)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({ status, effects: outcome, inspectionRequired: false })
})
it.each(['uncertain','unknown'])('requires inspection for %s and preserves original request/stage/cause', async catalog => {
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'hub_unpublish_failed' }, outcome: { status: 'error', stage: 'catalog_delete', retraction: { ...receipt(), catalog }, cause: 'unavailable', observation_failed: true } },500)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({ status: 500, inspectionRequired: true, request: input(), effects: { stage: 'catalog_delete', cause: 'unavailable', retraction: { catalog }, observation_failed: true } })
})
it.each(['summary','minimal','empty','extractor'] as const)('requires original inspection after413 %s response', async kind => {
  vi.stubGlobal('fetch', vi.fn(async () => kind === 'empty' || kind === 'extractor'
    ? new Response(null,{status:413,headers:kind === 'empty' ? {'X-Graphit-Inspection-Required':'request-selection'} : {}})
    : response({ error: {code:'hub_unpublish_too_large'},inspection_required:true,selection:'request',effects:kind === 'summary' ? { retraction: receipt(),stage:'delivery' } : 'inspect_selected_target' },413)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({ status:413,inspectionRequired:true,request:input() })
})
it('retains unknown delivery after network failure without retrying', async () => {
  const fetcher=vi.fn(async()=>{throw new TypeError('network')});vi.stubGlobal('fetch',fetcher)
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({status:0,inspectionRequired:true,request:input(),effects:null})
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it.each([{producer:'other'},{artifact_type:'skill'},{id:'other'},{targeted_versions:['other']},{targeted_versions:[]},{targeted_versions:['release/one','release/one']},{desired_latest:'release/one'},{catalog:'removed',desired_latest:'other'}])('retains a crossed failed-delivery receipt as uncorrelated and requires inspection: %j',async delta=>{
  const outcome={status:'unpublished',retraction:{...receipt(),...delta}}
  vi.stubGlobal('fetch',vi.fn(async()=>response({error:{code:'unavailable'},outcome},503)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({status:503,code:'unavailable',effects:outcome,receiptCorrelated:false,inspectionRequired:true,request:input()})
})
it.each([{stage:{private:'invalid'}},{retraction:{catalog:{private:'invalid'}}},{retraction:{catalog:'uncertain',targeted_versions:[{}]}}])('rejects malformed finite effects instead of rendering native objects: %j',async outcome=>{
  vi.stubGlobal('fetch',vi.fn(async()=>response({error:{code:'hub_unpublish_failed'},outcome},500)))
  await expect(hubNative.unpublish(input())).rejects.toMatchObject({status:500,effects:null,receiptCorrelated:false,inspectionRequired:true,request:input()})
})
it('requires whole-artifact success to confirm removal of its catalogue',async()=>{
  vi.stubGlobal('fetch',vi.fn(async()=>response({status:'unpublished',retraction:receipt()})))
  await expect(hubNative.unpublish({...input(),target:{kind:'artifact'}})).rejects.toMatchObject({code:'invalid_response',inspectionRequired:true,receiptCorrelated:false})
})
