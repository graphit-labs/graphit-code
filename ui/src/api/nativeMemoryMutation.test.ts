import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeMemoryMutationApi } from './nativeMemoryMutation'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'

const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: ' literal version ' }
const input = { idempotency_key: 'fixture-create', title: ' Literal title ', body: '# Body\n', attributes: { classification: 'decision' as const, important: true, mandatory: false }, tags: ['literal,tag'], references: [] }
const receipt = { state: 'completed', id: '01ARZ3NDEKTSV4RRFFQ69G5FAV', revision: 1, idempotency_key: input.idempotency_key, sdk_version: null, generation: null, recovery_key: null, completion_error: null, delivery_error: null }
afterEach(() => vi.unstubAllGlobals())
describe('native Memory mutation wire and real effects', () => {
  it('uses closed PROJECT create and CAS update/delete inputs with exact integer tokens', async () => {
    const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ selection, native: receipt })))
    vi.stubGlobal('fetch', fetcher)
    const scope = { kind: 'project' as const, selection }, controller = new AbortController()
    await nativeMemoryMutationApi.create(scope, input, controller.signal)
    const update = { id: receipt.id, expected_revision: 9007199254740993n, idempotency_key: input.idempotency_key, reason: 'Revised guidance', body: '# Literal\n', tags: ['literal,tag'] }
    await nativeMemoryMutationApi.update(scope, update, controller.signal)
    await nativeMemoryMutationApi.remove(scope, { id: receipt.id, expected_revision: 18446744073709551615n, idempotency_key: input.idempotency_key, reason: 'Retired guidance' }, controller.signal)
    expect(fetcher.mock.calls.map(call => call[0])).toEqual(['/api/memory/create', '/api/memory/update', '/api/memory/delete'])
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ selection, input })
    expect(parseNativeJson(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: update })
    expect(fetcher.mock.calls[2][1].body).toContain('"expected_revision":18446744073709551615')
    expect(fetcher.mock.calls[2][1].signal).toBe(controller.signal)
  })
  it('binds USER to the observed namespace while sending only input and never requesting PROJECT', async () => {
    const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ scope_id: 'fixture-user', currentOwner: { installation_id: 'fixture', identity: { issuer: 'fixture-issuer', subject: 'fixture-subject' } }, native: receipt })))
    vi.stubGlobal('fetch', fetcher)
    const result = await nativeMemoryMutationApi.create({ kind: 'user', requested_scope_id: 'fixture-user' }, input)
    expect(result).toMatchObject({ delivery: 'receipt', envelope: { scope_id: 'fixture-user', native: receipt } })
    expect(fetcher.mock.calls[0][0]).toBe('/api/user-memory/create')
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ input })
    expect(fetcher).toHaveBeenCalledOnce()
  })
  it('recognizes confirmed HTTP500 headers before error classification and never retries a known effect', async () => {
    const response = new Response('', { status: 500, headers: { 'x-graphit-memory-outcome': 'confirmed', 'x-graphit-memory-id': receipt.id, 'x-graphit-memory-revision': '18446744073709551615', 'x-graphit-memory-sdk-version': '9007199254740993', 'x-graphit-memory-response-error': 'response_limit', 'x-graphit-memory-delivery-error': 'cancelled' } })
    const body = vi.spyOn(response, 'text'), fetcher = vi.fn().mockResolvedValue(response); vi.stubGlobal('fetch', fetcher)
    expect(await nativeMemoryMutationApi.create({ kind: 'user' }, input)).toEqual({ delivery: 'headers', http_status: 500, receipt: { state: 'confirmed', id: receipt.id, revision: 18446744073709551615n, sdk_version: 9007199254740993n, response_error: 'response_limit', completion_error: null, delivery_error: 'cancelled' } })
    expect(body).not.toHaveBeenCalled(); expect(fetcher).toHaveBeenCalledOnce()
  })
  it('retains uncertain recovery and completion errors without manufacturing success or Memory data', async () => {
    const native = { ...receipt, state: 'uncertain', id: null, revision: null, recovery_key: { corpus_uri: 'fixture-corpus', physical_branch: 'fixture-branch', base_version: 7, transaction_id: 'fixture-transaction' }, completion_error: 'unknown_outcome', delivery_error: 'deadline_expired' }
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => new Response(JSON.stringify({ selection, native }))))
    const result = await nativeMemoryMutationApi.create({ kind: 'project', selection }, input)
    expect(result).toEqual({ delivery: 'receipt', envelope: { selection, native } })
    expect('memory' in result).toBe(false)
  })
  it('preserves real CAS conflict values and other causal HTTP errors', async () => {
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response('{"error":{"code":"revision_conflict","expected":9007199254740993,"actual":9007199254740994}}', { status: 409 }))
      .mockImplementationOnce(async () => new Response('{"error":{"code":"cancelled"}}', { status: 408 }))
    vi.stubGlobal('fetch', fetcher)
    await expect(nativeMemoryMutationApi.create({ kind: 'project', selection }, input)).rejects.toMatchObject({ status: 409, code: 'revision_conflict', expected: 9007199254740993n, actual: 9007199254740994n })
    await expect(nativeMemoryMutationApi.create({ kind: 'project', selection }, input)).rejects.toMatchObject({ status: 408, code: 'cancelled' })
    expect(fetcher).toHaveBeenCalledTimes(2)
  })
  it('rejects changed namespace, idempotency and selection receipts without repeating the mutation', async () => {
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ scope_id: 'different', native: receipt })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ selection, native: { ...receipt, idempotency_key: 'different' } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ selection: { ...selection, project_version: 'different' }, native: receipt })))
    vi.stubGlobal('fetch', fetcher)
    await expect(nativeMemoryMutationApi.create({ kind: 'user', requested_scope_id: 'fixture-user' }, input)).rejects.toThrow('namespace changed')
    await expect(nativeMemoryMutationApi.create({ kind: 'project', selection }, input)).rejects.toThrow('Invalid Memory mutation receipt')
    await expect(nativeMemoryMutationApi.create({ kind: 'project', selection }, input)).rejects.toThrow('selection changed')
    expect(fetcher).toHaveBeenCalledTimes(3)
  })
  it.each(['project', 'user'] as const)('keeps %s index publication separate when the Memory write was not applied', async kind => {
    const index = { sdk_version: 18446744073709551615n, generation: kind === 'user' ? { owner: { installation_id: 'fixture', identity: { issuer: 'fixture-issuer', subject: 'fixture-subject' } }, sdk_version: 18446744073709551615n, physical_branch: null, inherited_target: null } : null, recovery_key: { corpus_uri: 'index-corpus', physical_branch: null, base_version: 9007199254740993n, transaction_id: 'index-transaction' } }
    const native = { ...receipt, state: 'not_applied', id: null, revision: null, index_publication: index, completion_error: 'revision_conflict' }
    const envelope = kind === 'project' ? { selection, native } : { scope_id: 'fixture-user', native }
    const fetcher = vi.fn().mockResolvedValue(new Response(stringifyNativeJson(envelope))); vi.stubGlobal('fetch', fetcher)
    const result = await nativeMemoryMutationApi.create(kind === 'project' ? { kind, selection } : { kind, requested_scope_id: 'fixture-user' }, input)
    expect(result).toEqual({ delivery: 'receipt', envelope })
    expect(fetcher).toHaveBeenCalledOnce()
  })
  it.each(['confirmed', 'uncertain'] as const)('retains %s CRUD recovery separately from its index publication', async state => {
    const index = { sdk_version: 9007199254740993n, generation: null, recovery_key: { corpus_uri: 'index-corpus', physical_branch: null, base_version: 7, transaction_id: 'index-transaction' } }
    const native = { ...receipt, state, sdk_version: state === 'confirmed' ? 18446744073709551615n : null, recovery_key: { corpus_uri: 'crud-corpus', physical_branch: 'crud-branch', base_version: 9007199254740994n, transaction_id: 'crud-transaction' }, index_publication: index, completion_error: 'unknown_outcome' }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(stringifyNativeJson({ selection, native }))))
    expect(await nativeMemoryMutationApi.create({ kind: 'project', selection }, input)).toEqual({ delivery: 'receipt', envelope: { selection, native } })
  })
  it('recognizes not-applied HTTP500 headers with only the observed exact index version', async () => {
    const response = new Response('', { status: 500, headers: { 'x-graphit-memory-outcome': 'not_applied', 'x-graphit-memory-index-sdk-version': '18446744073709551615', 'x-graphit-memory-response-error': 'response_limit', 'x-graphit-memory-completion-error': 'revision_conflict' } })
    const body = vi.spyOn(response, 'text'), fetcher = vi.fn().mockResolvedValue(response); vi.stubGlobal('fetch', fetcher)
    expect(await nativeMemoryMutationApi.create({ kind: 'user' }, input)).toEqual({ delivery: 'headers', http_status: 500, receipt: { state: 'not_applied', id: null, revision: null, sdk_version: null, index_sdk_version: 18446744073709551615n, response_error: 'response_limit', completion_error: 'revision_conflict', delivery_error: null } })
    expect(body).not.toHaveBeenCalled(); expect(fetcher).toHaveBeenCalledOnce()
  })

})
