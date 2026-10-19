import { afterEach, expect, it, vi } from 'vitest'
import { artifactKinds, hubNative, type ArtifactKind, type PackageUploadMetadata } from './hubNative'
// These opaque client bytes qualify capture/transport, never ZIP or Native store validity.
const input = (kind: ArtifactKind = 'skill'): PackageUploadMetadata => ({ project_id: '01KSH1CRFFG8Z74B5ZS78WW808', project_version: 'feature/upload', publication: kind === 'ast' || kind === 'knowledge'
  ? { format: 'native', provider: 'corporate', artifact_type: kind, id: 'literal/artifact', version: 'release/one', name: 'Package' }
  : { format: 'file', provider: 'corporate', artifact_type: kind, id: 'literal/artifact', version: 'release/one', name: 'Package', set_latest: false } })
const receipt = (metadata: PackageUploadMetadata) => ({ producer: metadata.project_id, artifact_type: metadata.publication.artifact_type, id: metadata.publication.id, version: metadata.publication.version,
  ...(metadata.publication.format === 'native' ? { root_sha256: 'b'.repeat(64) } : { archive_sha256: 'a'.repeat(64), payload: 'published' }), catalog: 'published' })
const response = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })
afterEach(() => vi.unstubAllGlobals())
it.each(artifactKinds)('sends exact ordered multipart for %s and correlates its current receipt', async kind => {
  const selected = input(kind), original = structuredClone(selected), file = new File(['opaque'], 'package.' + (selected.publication.format === 'native' ? kind : 'zip'))
  let finish!: (value: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve })); vi.stubGlobal('fetch', fetcher)
  const signal = new AbortController().signal, pending = hubNative.upload(selected, file, signal)
  selected.project_version = 'changed'; selected.publication.id = 'changed'
  finish(response({ status: 'published', publication: receipt(original), project_metadata: 'unchanged' }))
  await expect(pending).resolves.toMatchObject({ status: 'published', publication: receipt(original) })
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/upload$/); expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(new Headers(init.headers).get('Content-Type')).toBeNull(); expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
  const parts = [...(init.body as FormData).entries()]
  expect(parts.map(([name]) => name)).toEqual(['metadata', 'file']); expect(typeof parts[0][1]).toBe('string')
  expect(JSON.parse(parts[0][1] as string)).toEqual(original); expect(parts[1][1]).toBe(file)
  expect(original).not.toHaveProperty('project_dir'); expect(original).not.toHaveProperty('agent')
  if (original.publication.format === 'native') { expect(original.publication).not.toHaveProperty('expected_sha256'); expect(original.publication).not.toHaveProperty('set_latest') }
})
it.each([undefined, [], [{ artifact_type: 'ast' as const, id: 'literal/dependency', selector: 'refs/pinned one' }]])('retains literal omitted/empty/declared dependencies: %j', async dependencies => {
  const metadata = input('knowledge'); if (dependencies !== undefined) metadata.publication.dependencies = dependencies
  const fetcher = vi.fn(async () => response({ status: 'published', publication: receipt(metadata), project_metadata: 'published' })); vi.stubGlobal('fetch', fetcher)
  await hubNative.upload(metadata, new File(['opaque'], 'input.knowledge'))
  const body = (fetcher.mock.calls[0] as unknown as [string, RequestInit])[1].body as FormData
  const captured = JSON.parse(body.get('metadata') as string)
  if (dependencies === undefined) expect(captured.publication).not.toHaveProperty('dependencies')
  else expect(captured.publication.dependencies).toEqual(dependencies)
})
it.each([
  { producer: 'other' }, { artifact_type: 'knowledge' }, { id: 'other' }, { version: 'other' }, { root_sha256: 'bad' }, { catalog: 'uncertain' }, { payload: 'published' }, { archive_sha256: 'a'.repeat(64) }, { invented: true },
])('rejects invalid or crossed Native receipts while retaining observed effects: %j', async delta => {
  const metadata = input('ast'), outcome = { status: 'published', publication: { ...receipt(metadata), ...delta }, project_metadata: 'published' }
  vi.stubGlobal('fetch', vi.fn(async () => response(outcome)))
  await expect(hubNative.upload(metadata, new File(['opaque'], 'input.ast'))).rejects.toMatchObject({ status: 200, code: 'invalid_response', effects: outcome, inspectionRequired: true, request: metadata })
})
it('rejects an invented metadata success value instead of coercing it', async () => {
  const metadata = input(); vi.stubGlobal('fetch', vi.fn(async () => response({ status: 'published', publication: receipt(metadata), project_metadata: ['published'] })))
  await expect(hubNative.upload(metadata, new File(['opaque'], 'input.zip'))).rejects.toMatchObject({ code: 'invalid_response', inspectionRequired: true })
})
it.each([400, 401, 409, 504])('retains real HTTP%s and Native effects, including a late published outcome', async status => {
  const metadata = input('knowledge'), outcome = { status: 'published', publication: receipt(metadata), project_metadata: 'published', stage: 'delivery', cause: 'request_ended' }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'publication_failed' }, outcome }, status)))
  await expect(hubNative.upload(metadata, new File(['opaque'], 'input.knowledge'))).rejects.toMatchObject({ status, code: 'publication_failed', effects: outcome, request: metadata })
})
it.each([true, false])('requires original inspection after413, JSON=%s', async json => {
  const metadata = input('ast'); vi.stubGlobal('fetch', vi.fn(async () => json ? response({ error: { code: 'request_too_large' }, outcome: { publication: 'unknown', project_metadata: 'unknown', observation_failed: true } }, 413, { 'X-Graphit-Inspection-Required': 'request-selection' }) : new Response(null, { status: 413, headers: { 'X-Graphit-Inspection-Required': 'request-selection' } })))
  await expect(hubNative.upload(metadata, new File(['opaque'], 'input.ast'))).rejects.toMatchObject({ status: 413, inspectionRequired: true, request: metadata })
})
it('propagates cancellation without another upload', async () => {
  const aborted = new DOMException('Cancelled', 'AbortError'), fetcher = vi.fn(async () => { throw aborted }); vi.stubGlobal('fetch', fetcher)
  await expect(hubNative.upload(input(), new File(['opaque'], 'input.zip'), new AbortController().signal)).rejects.toBe(aborted)
  expect(fetcher).toHaveBeenCalledOnce()
})
