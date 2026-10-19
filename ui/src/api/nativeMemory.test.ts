import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeMemoryApi } from './nativeMemory'
import { stringifyNativeJson } from './nativeJson'

const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
const owner = { installation_id: 'fixture-installation', identity: { issuer: 'https://issuer.invalid', subject: 'fixture subject' } }
const scopeId = '["fixture-installation","https://issuer.invalid","fixture subject"]'
afterEach(() => vi.unstubAllGlobals())

describe('native Memory browser readers', () => {
  it('preserves the canonical PROJECT publication and sends exact selection and boolean filters', async () => {
    const text = '{"selection":' + JSON.stringify(selection) + ',"native":{"state":"published","generation":{"sdk_version":18446744073709551615},"memories":[]}}'
    const fetcher = vi.fn().mockImplementation(async () => new Response(text)); vi.stubGlobal('fetch', fetcher)
    const signal = new AbortController().signal
    const result = await nativeMemoryApi.list({ kind: 'project', selection }, { mandatory: false, important: true, tag: 'literal tag' }, signal)
    expect(result.native).toMatchObject({ state: 'published', generation: { sdk_version: 18446744073709551615n }, memories: [] })
    expect(fetcher).toHaveBeenCalledWith('/api/memory/list', expect.objectContaining({ method: 'POST', signal, credentials: 'same-origin' }))
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { mandatory: false, important: true, tag: 'literal tag' } })
  })

  it('keeps history operations, before/after, deleted and missing observations without flattening', async () => {
    const history = { creation: { actor: 'fixture actor' }, current: { id: 'memory1', revision: 18446744073709551615n }, deleted: true, revisions: [{ actor: 'fixture actor', operation: { delete: { expected_revision: 9007199254740993n, reason: 'literal reason' } }, before: { revision: 9007199254740993n }, after: { revision: 18446744073709551615n } }], generation: { sdk_version: 18446744073709551615n } }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(stringifyNativeJson({ selection, native: history })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ selection, native: null })))
    vi.stubGlobal('fetch', fetcher)
    const result = await nativeMemoryApi.history({ kind: 'project', selection }, 'memory1')
    expect(result.native).toEqual(history)
    expect((await nativeMemoryApi.get({ kind: 'project', selection }, 'memory1')).native).toBeNull()
    expect(fetcher.mock.calls.map(call => call[0])).toEqual(['/api/memory/history', '/api/memory/get'])
    expect(JSON.parse(fetcher.mock.calls[0][1].body).input).toEqual({ id: 'memory1' })
  })

  it('reads only the admitted USER corpus, sends no PROJECT data, and rejects a different observed scope', async () => {
    const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ scope_id: scopeId, currentOwner: owner, native: { state: 'no_corpus' } })))
    vi.stubGlobal('fetch', fetcher)
    const result = await nativeMemoryApi.list({ kind: 'user', requested_scope_id: scopeId }, {})
    expect(result.native).toEqual({ state: 'no_corpus' })
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ input: {} })
    expect(fetcher.mock.calls[0][0]).toBe('/api/user-memory/list')
    await expect(nativeMemoryApi.list({ kind: 'user', requested_scope_id: 'different literal namespace' }, {})).rejects.toThrow('namespace is unavailable')
    expect(fetcher.mock.calls.every(call => call[0] === '/api/user-memory/list')).toBe(true)
  })

  it('rejects different physical selection and preserves HTTP failure and transport cancellation', async () => {
    const mirror = { ...selection, project_dir: "/m" }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ selection: mirror, native: null })))
      .mockImplementationOnce(async () => new Response('', { status: 403 }))
      .mockRejectedValueOnce(new DOMException('Aborted', 'AbortError'))
    vi.stubGlobal('fetch', fetcher)
    await expect(nativeMemoryApi.get({ kind: 'project', selection }, 'id')).rejects.toThrow('selection changed')
    await expect(nativeMemoryApi.get({ kind: 'project', selection }, 'id')).rejects.toMatchObject({ status: 403 })
    await expect(nativeMemoryApi.history({ kind: 'user' }, 'id', new AbortController().signal)).rejects.toMatchObject({ name: 'AbortError' })
  })
})
