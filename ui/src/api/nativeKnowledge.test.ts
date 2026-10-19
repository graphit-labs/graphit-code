import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi } from './nativeKnowledge'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'

const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
afterEach(() => vi.unstubAllGlobals())

describe('native Knowledge browser readers', () => {
  it('reads exact imported heads without PROJECT, preserving empty/absent/u64 and rejecting changed SourceKeys', async () => {
    const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal@id', version: ' pinned ', context_alias: 'literal alias' } }
    const native = { state: 'published', generation: { corpus_uri: 'import-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null }, heads: [{ slug: 'literal /slug.md', title: '', updated_at: '-9223372036854775808' }] }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(stringifyNativeJson({ source, native })))
      .mockImplementationOnce(async () => new Response(stringifyNativeJson({ source, native: { ...native, heads: [] } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ source, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ source: { ...source, artifact: { ...source.artifact, context_alias: 'other' } }, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response('{"error":{"code":"knowledge_capture_failure"}}', { status: 500 }))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController()
    expect((await nativeKnowledgeApi.importedHeads(source, controller.signal)).native).toEqual(native)
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/imported/heads')
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ input: { source } })
    expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
    expect((await nativeKnowledgeApi.importedHeads(source)).native).toMatchObject({ state: 'published', heads: [] })
    expect((await nativeKnowledgeApi.importedHeads(source)).native).toEqual({ state: 'registry_absent' })
    await expect(nativeKnowledgeApi.importedHeads(source)).rejects.toThrow('selection changed')
    await expect(nativeKnowledgeApi.importedHeads(source)).rejects.toMatchObject({ status: 500, code: 'knowledge_capture_failure' })
  })
  it('uses the own-module Ask stream with literal model, explicit empty selection and exclusive retrieval', async () => {
    const fetcher = vi.fn().mockImplementation(async () => new Response('event: progress\ndata: {"provider":"kiro","phase":"update","count":18446744073709551615}\n\nevent: final\ndata: {"status":"empty_selection","provided_slugs":[]}\n\n', { headers: { 'Content-Type': 'text/event-stream' } }))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController(), onProgress = vi.fn()
    expect(await nativeKnowledgeApi.ask(selection, { query: ' literal question ', model: ' literal model ', selected_slugs: [] }, { signal: controller.signal, onProgress })).toEqual({ status: 'empty_selection', provided_slugs: [] })
    expect(onProgress).toHaveBeenCalledWith({ provider: 'kiro', phase: 'update', count: 18446744073709551615n })
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/ask')
    expect(fetcher.mock.calls[0][1]).toMatchObject({ credentials: 'same-origin', signal: controller.signal, headers: { 'Content-Type': 'application/json', 'X-Graphit-Request': 'ui' } })
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { query: ' literal question ', model: ' literal model ', selected_slugs: [] } })
    await nativeKnowledgeApi.ask(selection, { query: 'question', model: 'model', retrieval: { mode: 'fts', top_k: 9007199254740993n } })
    expect(parseNativeJson(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: { query: 'question', model: 'model', retrieval: { mode: 'fts', top_k: 9007199254740993n } } })
    expect(fetcher).toHaveBeenCalledTimes(2)
  })
  it.each([undefined, false, true])('preserves Query rerank=%s, separate scores and lossless generation/cursor through continuation', async rerank => {
    const cursor = { binding: Array(32).fill(7), semantic_generation: [9007199254740993n, null, null], uri: 'fixture-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null, offset: 20 }
    const focus = { slug: 'literal /slug', title: '', source_path: 'docs/literal.md', compilation_digest: Array(32).fill(3), namespace: null, witness: null }
    const native = { state: 'partial', generation: { corpus_uri: 'fixture-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null }, semantic_generation: null, reason: 'model_unavailable', total: 9007199254740993n, hits: [{ focused_source: focus, lexical_score: 1.25, semantic_distance: 0.25, rrf_score: 0.5, rerank_score: 0, preview: { text: 'Literal preview', source: 'body', range: { start: 0, end: 15 }, truncated: false } }], next_cursor: cursor }
    const fetcher = vi.fn().mockImplementation(async () => new Response(stringifyNativeJson({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native })))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController()
    const input = { query: '  literal query  ', mode: 'hybrid' as const, metric: 'dot' as const, top_k: 0, page_size: 20, preview: true, ...(rerank === undefined ? {} : { rerank }) }
    const result = await nativeKnowledgeApi.search(selection, input, controller.signal)
    expect(result.native).toEqual(native)
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/search')
    expect(parseNativeJson(fetcher.mock.calls[0][1].body)).toEqual({ selection, input })
    expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
    await nativeKnowledgeApi.search(selection, { ...input, cursor }, controller.signal)
    expect(fetcher.mock.calls[1][1].body).toContain('"version":18446744073709551615')
    expect(parseNativeJson(fetcher.mock.calls[1][1].body)).toEqual({ selection, input: { ...input, cursor } })
  })

  it.each([undefined, false, true])('posts imported Query rerank=%s without PROJECT and rejects changed source or HTTP failure', async rerank => {
    const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal@id', version: 'literal version', context_alias: 'literal alias' } }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ source, focused_source: null, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ source: { ...source, artifact: { ...source.artifact, context_alias: 'changed' } }, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response('{"error":{"code":"search_pin_changed"}}', { status: 409 }))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController()
    const search = { query: 'literal', mode: 'semantic' as const, metric: 'l2' as const, preview: false, ...(rerank === undefined ? {} : { rerank }) }
    expect((await nativeKnowledgeApi.importedSearch(source, search, controller.signal)).native).toEqual({ state: 'registry_absent' })
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/imported/search')
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ input: { source, search } })
    expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
    await expect(nativeKnowledgeApi.importedSearch(source, search)).rejects.toThrow('selection changed')
    await expect(nativeKnowledgeApi.importedSearch(source, search)).rejects.toMatchObject({ status: 409, code: 'search_pin_changed' })
    expect(fetcher).toHaveBeenCalledTimes(3)
  })

  it('posts imported registry discovery and literal pages without PROJECT and rejects changed artifact tuples', async () => {
    const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal@id', version: ' pinned ', context_alias: 'literal alias' } }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ contexts: [source] })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ source, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ source: { ...source, artifact: { ...source.artifact, version: 'changed' } }, native: { state: 'registry_absent' } })))
      .mockImplementationOnce(async () => new Response('', { status: 503 }))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController()
    expect(await nativeKnowledgeApi.importedContexts(controller.signal)).toEqual({ contexts: [source] })
    expect((await nativeKnowledgeApi.importedPage(source, 'literal /slug', controller.signal)).native).toEqual({ state: 'registry_absent' })
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/imported/contexts')
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ input: {} })
    expect(fetcher.mock.calls[1][0]).toBe('/api/knowledge/imported/page')
    expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ input: { source, slug: 'literal /slug' } })
    expect(fetcher.mock.calls[1][1].signal).toBe(controller.signal)
    await expect(nativeKnowledgeApi.importedPage(source, 'literal')).rejects.toThrow('selection changed')
    await expect(nativeKnowledgeApi.importedPage(source, 'literal')).rejects.toMatchObject({ status: 503 })
  })
  it('preserves heads, empty titles and exact u64 generation instead of deriving modules', async () => {
    const text = '{"selection":' + JSON.stringify(selection) + ',"native":{"state":"published","generation":{"corpus_uri":"fixture-corpus","version":18446744073709551615,"physical_branch":null,"inherited_target":null},"heads":[{"slug":"literal/slug","title":"","updated_at":null}]}}'
    const fetcher = vi.fn().mockImplementation(async () => new Response(text)); vi.stubGlobal('fetch', fetcher)
    const result = await nativeKnowledgeApi.heads(selection)
    expect(result.native).toMatchObject({ state: 'published', generation: { version: 18446744073709551615n }, heads: [{ slug: 'literal/slug', title: '', updated_at: null }] })
    expect(fetcher.mock.calls[0][0]).toBe('/api/knowledge/heads')
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: {} })
  })

  it('keeps no corpus, published empty and absent pages distinct without inventing a document', async () => {
    for (const native of [{ state: 'no_corpus' }, { state: 'published', generation: { version: 1 }, page: { state: 'published_empty' } }, { state: 'published', generation: { version: 1 }, page: { state: 'page_absent' } }]) {
      const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify({ selection, native }))); vi.stubGlobal('fetch', fetcher)
      expect((await nativeKnowledgeApi.page(selection, 'literal /slug')).native).toEqual(native)
      expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ selection, input: { slug: 'literal /slug' } })
    }
  })

  it('retains all document fields, decimal nanosecond timestamps and repeated resolved targets', async () => {
    const document = { slug: 'literal/slug', title: '', body: '# Literal body', kind: 'guide', summary: '', description: null, summary_truncated: false, source_path: 'docs/literal.md', physical_digest: null, compilation_digest: Array(32).fill(0), recipe: 'literal', recipe_parameters: { fallback_title: '', kind_on_absent: '', max_summary_bytes: 1024 }, updated_at: '-9223372036854775808', stale_since: null, stale_after: { seconds: 1, nanoseconds: 2 }, resolved_wiki_targets: ['target', 'target'] }
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => new Response(JSON.stringify({ selection, native: { state: 'published', generation: { version: 1 }, page: { state: 'found', document } } }))))
    expect((await nativeKnowledgeApi.page(selection, document.slug)).native).toMatchObject({ page: { state: 'found', document } })
  })

  it('accepts field order differences but rejects a changed selection and keeps HTTP errors causal', async () => {
    const same = { project_version: selection.project_version, project_id: selection.project_id, project_dir: selection.project_dir }
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ selection: same, native: { state: 'no_corpus' } })))
      .mockImplementationOnce(async () => new Response(JSON.stringify({ selection: { ...selection, project_version: 'different' }, native: { state: 'no_corpus' } })))
      .mockImplementationOnce(async () => new Response('', { status: 500 }))
    vi.stubGlobal('fetch', fetcher)
    expect((await nativeKnowledgeApi.heads(selection)).native).toEqual({ state: 'no_corpus' })
    await expect(nativeKnowledgeApi.heads(selection)).rejects.toThrow('selection changed')
    await expect(nativeKnowledgeApi.heads(selection)).rejects.toMatchObject({ status: 500 })
  })
})
