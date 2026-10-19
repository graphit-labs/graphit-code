import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, mutationCompleted, type FileOutcome, type LocalInstallSelection, type MutationSelection } from './hubNative'

const input = (): LocalInstallSelection => ({ source: { project_id: '01D39ZY06FGSCTVN4T2V9PKHFZ', project_version: 'source/data', artifact_type: 'skill', id: 'vendor/reviewer', version: 'artifact/local' }, scope: { kind: 'project', project_id: '01KSH1CRFFG8Z74B5ZS78WW808', project_version: 'consumer/data' }, binding: 'codex' })
const success = (request = input()): FileOutcome => ({ status: 'ok', operation: 'install_local', scope: request.scope, cache: 'not_attempted', publication: 'Physical(Created)', source: 'local', producer: request.source.project_id, artifact_type: request.source.artifact_type, id: request.source.id, version: request.source.version, project: request.scope.project_id, project_version: request.scope.project_version, binding: request.binding, possible_parents: [] })
const respond = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })
afterEach(() => vi.unstubAllGlobals())

it.each(['agent', 'skill', 'command', 'workflow'] as const)('local install API captures exact %s source and independent PROJECT destination without remote authority', async artifact_type => {
  const request = input(); request.source.artifact_type = artifact_type
  const captured = structuredClone(request)
  let finish!: (response: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve })); vi.stubGlobal('fetch', fetcher)
  const signal = new AbortController().signal
  const work = hubNative.mutate('install_local', request, signal)
  request.source.project_id = request.scope.project_id; request.source.version = 'changed'; request.binding = 'opencode'
  finish(respond(success(captured)))
  await expect(work).resolves.toMatchObject({ source: 'local', producer: captured.source.project_id, project: captured.scope.project_id })
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/hub\/authored\/install$/)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual(captured)
  expect(mutationCompleted(success(captured))).toBe(true)
})

it.each([undefined, 'copy', 'link'] as const)('local install API preserves mode %s and optional naming intent without echoing observed values', async mode => {
  const request = { ...input(), ...(mode ? { mode, alias: 'kept-alias', entry_name: 'entry.md' } : {}) }
  const fetcher = vi.fn(async () => respond(success(request))); vi.stubGlobal('fetch', fetcher)
  const result = await hubNative.mutate('install_local', request)
  const body = JSON.parse((fetcher.mock.calls[0] as unknown as [string, RequestInit])[1].body as string)
  expect(body).toEqual(request)
  expect(result).not.toHaveProperty('mode'); expect(result).not.toHaveProperty('alias')
  expect(body).not.toHaveProperty('provider'); expect(body).not.toHaveProperty('selection'); expect(body).not.toHaveProperty('path')
})

it.each(['mode', 'alias', 'entry_name'] as const)('local install API does not silently treat explicit %s:null as omission', async field => {
  const request = { ...input(), [field]: null } as unknown as MutationSelection
  const fetcher = vi.fn(async () => respond({ error: { code: 'invalid_arguments' } }, 400)); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.mutate('install_local', request)).rejects.toMatchObject({ status: 400, request })
  expect(JSON.parse((fetcher.mock.calls[0] as unknown as [string, RequestInit])[1].body as string)).toHaveProperty(field, null)
})

it.each([
  { source: 'remote' }, { producer: 'other-source' }, { project: 'other-consumer' }, { project_version: 'other-data' },
  { version: 'other-artifact' }, { binding: 'cursor' }, { cache: 'Created' }, { id: 'other/id' },
])('local install API retains a crossed response as uncertain effects instead of success: %j', async change => {
  const outcome = { ...success(), ...change }
  vi.stubGlobal('fetch', vi.fn(async () => respond(outcome)))
  await expect(hubNative.mutate('install_local', input())).rejects.toMatchObject({ status: 200, code: 'invalid_response', inspectionRequired: true, outcome })
})

it('local install API preserves observed post-effect cleanup and committed/pending publication errors', async () => {
  const outcome = { ...success(), status: 'error', cleanup_pending: true, publication: { committed: true, hub_pending: false, core_pending: true, observation_failed: false }, possible_parents: ["/a"] }
  vi.stubGlobal('fetch', vi.fn(async () => respond({ error: { code: 'native_operation_failed' }, outcome }, 500)))
  await expect(hubNative.mutate('install_local', input())).rejects.toMatchObject({ status: 500, outcome, request: input() })
  expect(mutationCompleted({ ...success(), cache: undefined })).toBe(false)
  expect(mutationCompleted({ ...success(), cleanup_pending: true })).toBe(false)
})

it('local install API retains exact source/target inspection for bounded responses and never uses the FILE remote route', async () => {
  const fetcher = vi.fn(async () => new Response(null, { status: 413, headers: { 'X-Graphit-Inspection-Required': 'request-selection' } })); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.mutate('install_local', input())).rejects.toMatchObject({ status: 413, inspectionRequired: true, request: input() })
  expect((fetcher.mock.calls[0] as unknown as [string])[0]).toMatch(/\/authored\/install$/)
})
