import { afterEach, expect, it, vi } from 'vitest'
import { hubNative, type FilePublicationInput } from './hubNative'
const input = (): FilePublicationInput => ({ project_id: '01KSH1CRFFG8Z74B5ZS78WW808', project_version: 'feature/a', publication: { provider: 'corporate', artifact_type: 'skill', id: 'vendor/reviewer', version: 'release/one', name: 'Reviewer', set_latest: false } })
const receipt = () => ({ producer: input().project_id, artifact_type: 'skill', id: 'vendor/reviewer', version: 'release/one', archive_sha256: 'a'.repeat(64), payload: 'published', catalog: 'published' })
const response = (body: unknown, status = 200, headers?: HeadersInit) => new Response(JSON.stringify(body), { status, headers })
afterEach(() => vi.unstubAllGlobals())
it('publishes the exact captured SourcePROJECT DTO with its own published receipt', async () => {
  let finish!: (value: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve })); vi.stubGlobal('fetch', fetcher)
  const selected = input(), signal = new AbortController().signal, pending = hubNative.publish(selected, signal)
  selected.project_version = 'other'; selected.publication.version = 'other'
  finish(response({ status: 'published', publication: receipt(), project_metadata: 'unchanged' }))
  await expect(pending).resolves.toMatchObject({ status: 'published', publication: receipt() })
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/files\/publish$/)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(JSON.parse(init.body as string)).toEqual(input())
  expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
})
it.each([{ producer: 'other' }, { artifact_type: 'command' }, { id: 'other' }, { version: 'other' }, { catalog: 'uncertain' }, { archive_sha256: 'invalid' }])('refuses crossed or uncertain published receipts without erasing effects: %j', async delta => {
  const outcome = { status: 'published', publication: { ...receipt(), ...delta }, project_metadata: 'published' }
  vi.stubGlobal('fetch', vi.fn(async () => response(outcome)))
  await expect(hubNative.publish(input())).rejects.toMatchObject({ status: 200, code: 'invalid_response', inspectionRequired: true, request: input(), effects: outcome })
})
it.each([401,409,504])('retains actual publication effects after HTTP%s', async status => {
  const outcome = { publication: { ...receipt(), catalog: 'uncertain' }, project_metadata: 'published', stage: 'catalog_write', cause: 'conditional_conflict', owned_temporary_residue: 'owned/staging' }
  vi.stubGlobal('fetch', vi.fn(async () => response({ error: { code: 'publication_failed' }, outcome }, status)))
  await expect(hubNative.publish(input())).rejects.toMatchObject({ status, code: 'publication_failed', effects: outcome, request: input() })
})
it.each([true,false])('keeps original target and mandatory inspection after413, body present=%s', async body => {
  vi.stubGlobal('fetch', vi.fn(async () => body ? response({ error: { code: 'file_publication_too_large' }, inspection_required: true, effects: { publication: receipt(), project_metadata: 'published' } },413)
    : new Response(null,{ status:413,headers:{ 'X-Graphit-Inspection-Required':'request-selection' } })))
  await expect(hubNative.publish(input())).rejects.toMatchObject({ status:413,inspectionRequired:true,request:input(),effects:body?{project_metadata:'published'}:null })
})
