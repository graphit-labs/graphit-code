import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeMemoryApi } from './nativeMemory'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: ' literal ' }
const memory = { id: 'memory1', revision: 9007199254740993n, title: 'Current title', body: '# Current\n', attributes: { classification: 'decision', important: true, mandatory: false }, tags: ['literal,tag'], references: [], reference_generation: 7, created_by: 'fixture', updated_by: 'fixture', created_at: 0, updated_at: 1 }
const generation = { sdk_version: 18446744073709551615n, project_id: 'project1', project_version: selection.project_version, physical_branch: null, inherited_target: null }
afterEach(() => vi.unstubAllGlobals())
describe('native Memory search wire', () => {
  it('retains USER lexical matched revision, score, ordering and closed filters without PROJECT or query alias', async () => {
    const native = { mode: 'fts', model: null, state: 'published', generation, total_hits: 9007199254740993n, hits: [{ current: memory, matched: { ...memory, title: 'Historical title', body: '# Historical\n', revision: 1 }, location: { kind: 'archived', revision: 1 }, deleted: false, score: 0.125 }] }
    const fetcher = vi.fn().mockImplementation(async () => new Response(stringifyNativeJson({ scope_id: 'fixture-user', currentOwner: { installation_id: 'fixture', identity: { issuer: 'fixture', subject: 'fixture' } }, native })))
    vi.stubGlobal('fetch', fetcher)
    const input = { text: ' literal query ', top_k: 20, mode: 'fts' as const, filter: { important: false, mandatory: true, tag: 'literal,tag' }, include_deleted_history: true }
    const controller = new AbortController()
    expect((await nativeMemoryApi.search({ kind: 'user', requested_scope_id: 'fixture-user' }, input, controller.signal)).native).toEqual(native)
    expect(fetcher.mock.calls[0][0]).toBe('/api/user-memory/search')
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ input })
    expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
  })
  it('retains semantic/hybrid identity, distances, lexical evidence and unavailable states without fabricated hits', async () => {
    const model = { vector_id: Array(32).fill(9), dimensions: 384, projection_recipe: 'fixture-recipe' }
    for (const native of [
      { mode: 'semantic', model, state: 'published', generation, total_hits: 1, hits: [{ current: memory, matched_revision: memory.revision, location: { kind: 'current' }, distance: 0.2 }] },
      { mode: 'hybrid', model, state: 'published', generation, total_hits: 1, hits: [{ current: memory, lexical: { matched_revision: 1, location: { kind: 'archived', revision: 1 }, score: 0.1 }, semantic: { matched_revision: memory.revision, location: { kind: 'current' }, distance: 0.2 }, score: 0.67 }] },
      ...['model_unavailable', 'missing_coverage', 'incompatible_identity'].map(reason => ({ mode: 'semantic', model: reason === 'model_unavailable' ? null : model, state: 'unavailable', reason })),
    ]) {
      vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => new Response(stringifyNativeJson({ selection, native }))))
      expect((await nativeMemoryApi.search({ kind: 'project', selection }, { text: 'fixture', top_k: 20, mode: native.mode as 'semantic' | 'hybrid' })).native).toEqual(native)
    }
  })
  it('preserves model access, failure and deadline HTTP causes instead of an empty healthy search', async () => {
    for (const [status, code] of [[403, 'model_access_denied'], [502, 'model_failure'], [408, 'deadline_expired']] as const) {
      vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => new Response(JSON.stringify({ error: { code } }), { status })))
      await expect(nativeMemoryApi.search({ kind: 'project', selection }, { text: 'fixture', top_k: 20, mode: 'semantic' })).rejects.toMatchObject({ status, code })
    }
  })
})
