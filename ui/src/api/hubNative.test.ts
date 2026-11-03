import { afterEach, expect, it, vi } from 'vitest'
import { effectsNeedInspection, hubNative, HubRequestError, mutationCompleted, type FileOutcome, type InstallSelection } from './hubNative'
import { useAppStore } from '@/store/appStore'

afterEach(() => vi.unstubAllGlobals())
const input = (): InstallSelection => ({
  scope: { kind: 'project', project_id: '01KSH1CRFFG8Z74B5ZS78WW808', project_version: 'feature/a' },
  binding: 'kiro-v3', alias: 'reviewer',
  selection: { transport: 'broker', provider: 'corporate', producer: '01D39ZY06FGSCTVN4T2V9PKHFZ', artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', exact: true },
})
const respond = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })

it('uses the Rust POST DTO and same-origin client without adding ambient PROJECT, Native profile or agent aliases', async () => {
  useAppStore.setState({ activeProjectDir: '/unrelated', activeProjectVersion: 'wrong', activeAgent: 'kiro-v2' })
  const fetcher = vi.fn(async () => respond({ status: 'ok', producer: 'publisher', artifacts: [], next_cursor: null }))
  vi.stubGlobal('fetch', fetcher)
  const signal = new AbortController().signal
  const query = { transport: 'broker' as const, provider: 'corporate', producer: 'publisher', version: 'release-1', limit: 20 }
  await hubNative.search(query, signal)
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/remote\/search$/)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual(query)
  expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
})

it('refuses a producer-crossed catalog and preserves a real cursor restart instead of empty success', async () => {
  const fetcher = vi.fn(async () => respond({ status: 'ok', producer: 'another', artifacts: [], next_cursor: null }))
  vi.stubGlobal('fetch', fetcher)
  const query = { transport: 'broker' as const, provider: 'corporate', producer: 'publisher', limit: 20 }
  await expect(hubNative.search(query)).rejects.toThrow('Invalid Hub artifact page')
  fetcher.mockImplementation(async () => respond({ error: { code: 'catalog_restart_required' } }, 409))
  await expect(hubNative.search({ ...query, cursor: 'old-cursor' })).rejects.toMatchObject({ status: 409, code: 'catalog_restart_required', outcome: null })
})

it('retains the original immutable target and bounded effects when a mutation receipt exceeds the output limit', async () => {
  let finish!: (response: Response) => void
  vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(resolve => { finish = resolve })))
  const selection = input()
  const pending = hubNative.mutate('install', selection)
  selection.scope = { kind: 'global_cache' }
  selection.selection.version = 'release-2'
  finish(respond({ error: { code: 'remote_file_too_large' }, inspection_required: true, selection: 'request', effects: { cache: 'Created', publication: { committed: true, hub_pending: true } } }, 413))
  await expect(pending).rejects.toMatchObject({
    status: 413, inspectionRequired: true, effects: { cache: 'Created' },
    request: { scope: { kind: 'project', project_version: 'feature/a' }, selection: { version: 'release-1' } },
  })
})

it('keeps inspection mandatory for an empty 413 body carrying only the fixed header', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 413, headers: { 'X-Graphit-Inspection-Required': 'request-selection' } })))
  await expect(hubNative.mutate('install', input())).rejects.toMatchObject({ status: 413, code: null, inspectionRequired: true, outcome: null, request: input() })
})

it('retains post-effect outcomes on authentication failure and distinguishes uncertainty from verified completion', async () => {
  const outcome = { status: 'error', operation: 'install', scope: input().scope, cache: 'Created', publication: { committed: true, hub_pending: true }, stage_cleanup_pending: true }
  vi.stubGlobal('fetch', vi.fn(async () => respond({ error: { code: 'unauthorized' }, outcome }, 401)))
  try { await hubNative.mutate('install', input()); throw new Error('Expected failed mutation') } catch (error) {
    expect(error).toBeInstanceOf(HubRequestError)
    expect((error as HubRequestError).outcome).toEqual(outcome)
    expect(effectsNeedInspection((error as HubRequestError).outcome!)).toBe(true)
  }
  expect(effectsNeedInspection({ cache: 'Unchanged', publication: 'VerifiedUnchanged' })).toBe(false)
  expect(effectsNeedInspection({ publication: { change: 'Unknown', committed: null } })).toBe(true)
})

it('accepts semantically identical installed scopes regardless of field order and rejects nonadvancing pages', async () => {
  const scope = { kind: 'project' as const, project_id: 'project', project_version: 'branch' }
  const fetcher = vi.fn(async () => respond({ status: 'ok', scope: { project_version: 'branch', project_id: 'project', kind: 'project' }, entries: [], next_offset: null }))
  vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.installed({ scope, binding: 'kiro-v3', offset: 0, limit: 20 })).resolves.toMatchObject({ entries: [] })
  fetcher.mockImplementation(async () => respond({ status: 'ok', scope, entries: [], next_offset: 20 }))
  await expect(hubNative.installed({ scope, offset: 20, limit: 20 })).rejects.toThrow('Invalid installed FILE page')
})

function receipt(selection = input()): FileOutcome {
  return { status: 'ok', operation: 'install', scope: selection.scope, artifact_type: selection.selection.artifact_type, id: selection.selection.id, version: selection.selection.version, producer: selection.selection.producer, binding: selection.binding, cache: 'Created', publication: 'Physical(Created)' }
}

it.each([
  { operation: 'remove' },
  { scope: { kind: 'global_cache' } },
  { id: 'another/artifact' },
  { artifact_type: 'command' },
  { version: 'another-release' },
  { producer: 'another-producer' },
  { binding: 'antigravity' },
])('preserves effects and requires inspection for a crossed mutation receipt: %j', async crossed => {
  const outcome = { ...receipt(), ...crossed, cleanup_pending: true }
  vi.stubGlobal('fetch', vi.fn(async () => respond(outcome)))
  await expect(hubNative.mutate('install', input())).rejects.toMatchObject({
    code: 'invalid_response', inspectionRequired: true, outcome, effects: outcome, request: input(),
  })
})

it('correlates a success with the immutable submitted selection and permits a genuinely resolved non-exact version', async () => {
  let finish!: (response: Response) => void
  vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(resolve => { finish = resolve })))
  const selection = input(), outcome = receipt(selection)
  const pending = hubNative.mutate('install', selection)
  selection.binding = 'opencode'; selection.selection.version = 'changed-after-send'
  finish(respond(outcome))
  await expect(pending).resolves.toEqual(outcome)
  const resolved = input(); resolved.selection.exact = false
  vi.stubGlobal('fetch', vi.fn(async () => respond({ ...receipt(resolved), version: 'resolved-release' })))
  await expect(hubNative.mutate('install', resolved)).resolves.toMatchObject({ version: 'resolved-release' })
})

it('accepts PROJECT lifecycle without an invented producer and enforces the selected installed version', async () => {
  const { scope, binding, selection } = input()
  const request = { scope, binding, artifact_type: selection.artifact_type, id: selection.id, version: selection.version }
  const outcome = { status: 'ok', operation: 'recover', ...request, publication: { change: 'Recovered', cleanup_pending: false } }
  const fetcher = vi.fn(async () => respond(outcome)); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.mutate('recover', request)).resolves.toEqual(outcome)
  fetcher.mockImplementation(async () => respond({ ...outcome, version: 'another-release' }))
  await expect(hubNative.mutate('recover', request)).rejects.toMatchObject({ code: 'invalid_response', inspectionRequired: true })
})

it('does not infer successful publication from unknown changes or commitment alone', () => {
  expect(mutationCompleted({ ...receipt(), publication: 'Unsupported' })).toBe(false)
  expect(mutationCompleted({ ...receipt(), publication: 'FutureChange' })).toBe(false)
  expect(mutationCompleted({ ...receipt(), publication: { committed: true } })).toBe(false)
  expect(mutationCompleted({ ...receipt(), publication: 'Physical(Updated)' })).toBe(true)
  expect(mutationCompleted({ ...receipt(), publication: { change: 'MetadataRemoved', committed: true } })).toBe(true)
  expect(mutationCompleted({ ...receipt(), publication: { change: 'VerifiedUnchanged', committed: false } })).toBe(true)
  expect(effectsNeedInspection({ publication: { committed: false } })).toBe(false)
  expect(effectsNeedInspection({ publication: { committed: false, core_pending: true } })).toBe(true)
  expect(effectsNeedInspection({ publication: { change: 'Unknown', committed: null } })).toBe(true)
})

it('requires a confirmed installed cache for install/update while lifecycle retains its separate publication result', () => {
  for (const cache of [undefined, 'not_attempted', 'Removed', 'FutureChange']) {
    expect(mutationCompleted({ ...receipt(), cache })).toBe(false)
    expect(mutationCompleted({ ...receipt(), operation: 'update', cache })).toBe(false)
  }
  expect(effectsNeedInspection({ cache: 'FutureChange' })).toBe(true)
  expect(mutationCompleted({ ...receipt(), operation: 'remove', cache: 'not_attempted', publication: { change: 'MetadataRemoved', committed: true } })).toBe(true)
})

it.each(['install','update'] as const)('physical mode %s captures copy or actual omission without ambient defaults',async operation=>{
  for(const mode of [undefined,'copy'] as const){
    let finish!:(value:Response)=>void
    const fetcher=vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve}));vi.stubGlobal('fetch',fetcher)
    const request={...input(),...(mode?{mode}:{}),entry_name:'review.md'},captured=structuredClone(request)
    const pending=hubNative.mutate(operation,request)
    request.mode=mode==='copy'?undefined:'copy';request.scope={kind:'global_cache'};request.alias='other'
    finish(respond({...receipt(captured),operation}))
    await expect(pending).resolves.toMatchObject({operation,scope:captured.scope})
    const [,init]=fetcher.mock.calls[0] as unknown as [string,RequestInit]
    const body=JSON.parse(init.body as string)
    expect(body).toEqual(captured)
    expect(Object.prototype.hasOwnProperty.call(body,'mode')).toBe(mode!==undefined)
  }
})
it('physical mode preserves explicit Link and not-attempted Unsupported without fallback or retry',async()=>{
  const request={...input(),mode:'copy' as const},outcome={status:'error',operation:'install',scope:request.scope,cache:'not_attempted',publication:'not_attempted'}
  const fetcher=vi.fn(async()=>respond({error:{code:'unsupported'},outcome},501));vi.stubGlobal('fetch',fetcher)
  await expect(hubNative.mutate('install',request)).rejects.toMatchObject({status:501,code:'unsupported',outcome,request})
  expect(fetcher).toHaveBeenCalledTimes(1)
})
