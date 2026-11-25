import '@/test/contextControls'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import userEvent from '@testing-library/user-event'
import { nativeKnowledgeApi, type KnowledgeEnvelope, type NativeKnowledgeSearch as SearchReply } from '@/api/nativeKnowledge'
import { stringifyNativeJson } from '@/api/nativeJson'
import { RefreshContext, type Registration } from '@/components/layout/WorkspaceRefresh'
import { NativeKnowledgeSearch } from './NativeKnowledgeSearch'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { search: vi.fn(), importedSearch: vi.fn() } }))
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
const focus = { slug: 'literal /slug.md', title: 'Literal result' }
const hits: Extract<SearchReply, { hits: unknown[] }> = { state: 'hits', total: 1, hits: [{ ...focus, preview: { text: 'Literal preview', truncated: true } }], next_cursor: null }
const envelope = (native: SearchReply): KnowledgeEnvelope<SearchReply> => ({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native })
const scope = { kind: 'project' as const, selection }
function submit() { fireEvent.change(screen.getByRole('textbox', { name: 'Search Knowledge pages' }), { target: { value: 'literal query' } }); fireEvent.click(screen.getByRole('button', { name: 'Search pages' })) }
function openOptions() { fireEvent.click(screen.getByText('Search options')) }
beforeEach(() => { vi.clearAllMocks(); vi.mocked(nativeKnowledgeApi.search).mockResolvedValue(envelope(hits)) })

describe('native Knowledge Query controls', () => {
  it('renders the preview, opens the literal slug and preserves a u64 result limit', async () => {
    const openPage = vi.fn(); render(<NativeKnowledgeSearch scope={scope} openPage={openPage} />)
    expect(screen.getByText('Search options').closest('details')?.open).toBe(false)
    openOptions()
    fireEvent.change(screen.getByRole('textbox', { name: 'Knowledge result limit' }), { target: { value: '18446744073709551615' } })
    submit()
    await screen.findByText('Literal preview')
    expect(nativeKnowledgeApi.search).toHaveBeenCalledWith(selection, { query: 'literal query', mode: 'fts', metric: 'cosine', top_k: 18446744073709551615n, page_size: 20, preview: true }, expect.any(AbortSignal))
    expect(screen.getByText('Preview truncated')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Literal result' }))
    expect(openPage).toHaveBeenCalledWith(focus.slug)
  })

  it('echoes the complete observed cursor and retries a pin error with that captured input', async () => {
    const cursor = { binding: Array(32).fill(7), semantic_generation: null, uri: 'fixture-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null, offset: 20 }
    vi.mocked(nativeKnowledgeApi.search).mockResolvedValueOnce(envelope({ ...hits, next_cursor: cursor }))
      .mockRejectedValueOnce(new Error('HTTP409 search_pin_changed'))
      .mockResolvedValueOnce(envelope(hits))
    const user = userEvent.setup()
    render(<NativeKnowledgeSearch scope={scope} openPage={vi.fn()} />)
    openOptions()
    await user.click(screen.getByRole('combobox', { name: 'Knowledge reranking' })); await user.click(screen.getByRole('option', { name: 'On' }))
    submit()
    fireEvent.click(await screen.findByRole('button', { name: 'More search results' }))
    await screen.findByText('HTTP409 search_pin_changed')
    expect(screen.getByRole('button', { name: 'Literal result' })).toBeTruthy()
    const capturedInput = vi.mocked(nativeKnowledgeApi.search).mock.calls[1][1]
    expect(capturedInput.cursor).toEqual(cursor)
    expect(capturedInput.rerank).toBe(true)
    expect(stringifyNativeJson(capturedInput)).toContain('"version":18446744073709551615')
    fireEvent.click(screen.getByRole('button', { name: 'Retry search' }))
    await screen.findByText('Literal result')
    expect(vi.mocked(nativeKnowledgeApi.search).mock.calls[2][1]).toEqual(capturedInput)
  })

  it('retains partial/unavailable causes and distinguishes no hits, missing corpus and empty publication', async () => {
    const cases: Array<[SearchReply, string]> = [
      [{ ...hits, state: 'partial', reason: 'model_unavailable' }, 'Partial Knowledge search results'],
      [{ ...hits, state: 'unavailable', hits: [], reason: 'missing_semantic' }, 'Knowledge search unavailable'],
      [{ ...hits, state: 'no_hits', hits: [] }, 'No Knowledge matches'],
      [{ ...hits, state: 'published_empty', hits: [] }, 'The Knowledge publication is empty'],
      [{ state: 'no_corpus' }, 'No Knowledge corpus has been published'],
    ]
    for (const [native, title] of cases) {
      vi.mocked(nativeKnowledgeApi.search).mockResolvedValueOnce(envelope(native))
      const view = render(<NativeKnowledgeSearch scope={scope} openPage={vi.fn()} />); submit()
      await screen.findByText(title)
      if (native.state === 'partial' || native.state === 'unavailable') expect(screen.queryByText('No Knowledge matches')).toBeNull()
      view.unmount()
    }
  })

  it('aborts a pending Query on cancel and on a source change and ignores late success or denial', async () => {
    let finish!: (value: KnowledgeEnvelope<SearchReply>) => void
    vi.mocked(nativeKnowledgeApi.search).mockImplementationOnce(() => new Promise(done => { finish = done }))
    const view = render(<NativeKnowledgeSearch scope={scope} openPage={vi.fn()} />); submit()
    const oldSignal = vi.mocked(nativeKnowledgeApi.search).mock.calls[0][2]!
    fireEvent.click(screen.getByRole('button', { name: 'Cancel search' }))
    expect(oldSignal.aborted).toBe(true)
    await act(async () => finish(envelope(hits)))
    expect(screen.queryByText('Literal result')).toBeNull()
    expect(screen.getByText(/Search request stopped/)).toBeTruthy()
    let deny!: (reason: Error) => void
    vi.mocked(nativeKnowledgeApi.search).mockImplementationOnce(() => new Promise((_done, reject) => { deny = reject }))
    fireEvent.click(screen.getByRole('button', { name: 'Retry search' }))
    await waitFor(() => expect(nativeKnowledgeApi.search).toHaveBeenCalledTimes(2))
    const nextSignal = vi.mocked(nativeKnowledgeApi.search).mock.calls[1][2]!
    view.rerender(<NativeKnowledgeSearch scope={{ kind: 'project', selection: { ...selection, project_dir: "/m" } }} openPage={vi.fn()} />)
    expect(nextSignal.aborted).toBe(true)
    await act(async () => deny(new Error('late HTTP403')))
    expect(screen.queryByText('late HTTP403')).toBeNull()
    expect(screen.queryByText('Literal result')).toBeNull()
  })
  it.each([['Off', false], ['On', true]] as const)('uses explicit %s for an imported source and retains backend order', async (option, rerank) => {
    const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal-id', version: 'pinned' } }
    const native: SearchReply = { ...hits, total: 2, hits: [hits.hits[0], { ...hits.hits[0], slug: 'alpha', title: 'Alpha result' }] }
    vi.mocked(nativeKnowledgeApi.importedSearch).mockResolvedValue({ source, focused_source: null, native })
    const user = userEvent.setup(); render(<NativeKnowledgeSearch scope={{ kind: 'imported', source }} openPage={vi.fn()} />)
    openOptions()
    await user.click(screen.getByRole('combobox', { name: 'Knowledge reranking' })); await user.click(screen.getByRole('option', { name: option }))
    submit(); await screen.findByText('Alpha result')
    expect(nativeKnowledgeApi.search).not.toHaveBeenCalled()
    expect(nativeKnowledgeApi.importedSearch).toHaveBeenCalledWith(source, expect.objectContaining({ rerank, query: 'literal query' }), expect.any(AbortSignal))
    const results = screen.getByRole('navigation', { name: 'Knowledge search results' })
    expect(Array.from(results.querySelectorAll('button strong')).map(node => node.textContent)).toEqual(['Literal result', 'Alpha result'])
  })

  it('aborts and invalidates a pending search when reranking changes, then clears results on a query edit', async () => {
    let finish!: (value: KnowledgeEnvelope<SearchReply>) => void
    vi.mocked(nativeKnowledgeApi.search).mockImplementationOnce(() => new Promise(done => { finish = done }))
    const user = userEvent.setup(); render(<NativeKnowledgeSearch scope={scope} openPage={vi.fn()} />); submit()
    openOptions()
    const signal = vi.mocked(nativeKnowledgeApi.search).mock.calls[0][2]!
    await user.click(screen.getByRole('combobox', { name: 'Knowledge reranking' })); await user.click(screen.getByRole('option', { name: 'On' }))
    expect(signal.aborted).toBe(true)
    await act(async () => finish(envelope(hits)))
    expect(screen.queryByText('Literal result')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Search pages' })); await screen.findByText('Literal result')
    expect(vi.mocked(nativeKnowledgeApi.search).mock.calls[1][1].rerank).toBe(true)
    fireEvent.change(screen.getByRole('textbox', { name: 'Search Knowledge pages' }), { target: { value: 'changed query' } })
    expect(screen.queryByText('Literal result')).toBeNull()
    expect(screen.queryByRole('button', { name: 'More search results' })).toBeNull()
  })

  it('appends matches and refreshes the loaded pages using a fresh cursor', async () => {
    const firstCursor = { offset: 20, version: 1 }
    const refreshedCursor = { offset: 20, version: 2 }
    const second = { slug: 'other', title: 'Second result' }
    vi.mocked(nativeKnowledgeApi.search)
      .mockResolvedValueOnce(envelope({ ...hits, total: 2, next_cursor: firstCursor }))
      .mockResolvedValueOnce(envelope({ ...hits, total: 2, hits: [second, hits.hits[0]] }))
      .mockResolvedValueOnce(envelope({ ...hits, total: 2, next_cursor: refreshedCursor }))
      .mockResolvedValueOnce(envelope({ ...hits, total: 2, hits: [{ ...second, title: 'Refreshed result' }] }))
    let registration!: Registration
    render(<RefreshContext.Provider value={{ register: entry => { registration = entry; return () => {} }, scope: 'knowledge', refresh: async () => {}, refreshing: false, error: '' }}>
      <NativeKnowledgeSearch scope={scope} openPage={vi.fn()} />
    </RefreshContext.Provider>)
    submit()
    fireEvent.click(await screen.findByRole('button', { name: 'More search results' }))
    await screen.findByRole('button', { name: 'Second result' })
    expect(screen.getAllByRole('button', { name: 'Literal result' })).toHaveLength(1)
    await act(async () => { await registration.run() })
    expect(screen.getByRole('button', { name: 'Refreshed result' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Literal result' })).toBeTruthy()
    const requests = vi.mocked(nativeKnowledgeApi.search).mock.calls
    expect(requests.map(call => call[1].cursor)).toEqual([undefined, firstCursor, undefined, refreshedCursor])
  })

})
