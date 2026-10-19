import '@/test/contextControls'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, useNavigate } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi, type ImportedKnowledgeEnvelope, type ImportedKnowledgeSource } from '@/api/nativeKnowledge'
import { importedKnowledgeHref } from '@/api/nativeRecordNavigation'
import { parseNativeJson } from '@/api/nativeJson'
import { NativeRecordReadError } from '@/api/nativeRecordRead'
import WikiExplorerPage from './WikiExplorerPage'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { heads: vi.fn(), page: vi.fn(), importedContexts: vi.fn(), importedHeads: vi.fn(), importedPage: vi.fn() } }))
const source: ImportedKnowledgeSource = { kind: 'imported_knowledge', artifact: { id: 'literal@artifact', version: ' pinned / version ', context_alias: 'literal alias' } }
const mirror: ImportedKnowledgeSource = { ...source, artifact: { ...source.artifact, context_alias: 'other alias' } }
const generation = { corpus_uri: 'fixture-import', version: 18446744073709551615n, physical_branch: null, inherited_target: null }
const document = { slug: 'literal /slug.md', title: 'Imported document', body: '# Complete imported body\n\n## Content\n\n**Imported guidance**', kind: 'guide', summary: 'Recorded summary', description: null, summary_truncated: false, source_path: 'docs/import.md', physical_digest: null, compilation_digest: Array(32).fill(7), recipe: 'literal recipe', recipe_parameters: { fallback_title: '', kind_on_absent: 'guide', max_summary_bytes: 9007199254740993n }, updated_at: '-9223372036854775808', stale_since: null, stale_after: { seconds: 18446744073709551615n, nanoseconds: 999999999 }, resolved_wiki_targets: ['literal target.md', 'literal target.md'] }
const found: ImportedKnowledgeEnvelope = { source, focused_source: null, native: { state: 'published', generation, page: { state: 'found', document } } }
function SwitchArtifact() { const navigate = useNavigate(); return <button onClick={() => navigate(importedKnowledgeHref(mirror, document.slug))}>Switch imported tuple</button> }
function mount(path = importedKnowledgeHref(source, document.slug)) { return render(<MemoryRouter initialEntries={[path]}><SwitchArtifact /><WikiExplorerPage /></MemoryRouter>) }
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
  vi.mocked(nativeKnowledgeApi.importedContexts).mockResolvedValue({ contexts: [source, mirror] })
  vi.mocked(nativeKnowledgeApi.importedHeads).mockResolvedValue({ source, native: { state: 'published', generation, heads: [{ slug: document.slug, title: document.title, updated_at: '-9223372036854775808' }] } })
  vi.mocked(nativeKnowledgeApi.importedPage).mockResolvedValue(found)
})

describe('native imported Knowledge reader', () => {
  it('requires an explicit artifact choice and never chooses the first inventory item', async () => {
    mount(importedKnowledgeHref(null))
    await screen.findByRole('combobox', { name: 'Imported Knowledge artifact' })
    expect(screen.getByText('Select an installed artifact')).toBeTruthy()
    expect(nativeKnowledgeApi.importedPage).not.toHaveBeenCalled()
    expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
  })
  it('uses the exact imported tuple and literal slug, retaining all fields and repeated targets', async () => {
    mount()
    const dossier = screen.getByRole('article', { name: 'Imported Knowledge document' })
    await within(dossier).findByRole('heading', { name: document.title })
    expect(nativeKnowledgeApi.importedPage).toHaveBeenCalledWith(source, document.slug, expect.any(AbortSignal))
    expect(screen.getByText('Maximum summary bytes').nextElementSibling?.textContent).toBe('9007199254740993')
    expect(parseNativeJson(screen.getByText('Complete publication document').parentElement!.querySelector('pre')!.textContent!)).toEqual(found.native)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Raw body' }))
    expect(within(dossier).getByRole('heading', { name: 'Document body' }).closest('section')!.querySelector('pre')!.textContent).toBe(document.body)
    await user.click(screen.getAllByRole('button', { name: 'literal target.md' })[0])
    await waitFor(() => expect(nativeKnowledgeApi.importedPage).toHaveBeenLastCalledWith(source, 'literal target.md', expect.any(AbortSignal)))
    expect((screen.getByLabelText('Imported Knowledge page slug') as HTMLInputElement).value).toBe('literal target.md')
    expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
  })
  it('distinguishes registry absent, empty publication and absent page, then retries a real causal error', async () => {
    for (const [native, title] of [
      [{ state: 'registry_absent' }, 'Knowledge artifact is not registered'],
      [{ state: 'published', generation, page: { state: 'published_empty' } }, 'The Knowledge publication is empty'],
      [{ state: 'published', generation, page: { state: 'page_absent' } }, 'Knowledge page not found'],
    ] as const) {
      vi.mocked(nativeKnowledgeApi.importedPage).mockResolvedValueOnce({ source, focused_source: null, native })
      const view = mount(); await screen.findByText(title); view.unmount()
    }
    vi.mocked(nativeKnowledgeApi.importedPage).mockRejectedValueOnce(new NativeRecordReadError(503))
    mount(); await screen.findByText('Record request failed (HTTP 503)')
    expect(screen.queryByText('Knowledge artifact is not registered')).toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry document' }))
    expect(await screen.findByRole('heading', { name: document.title })).toBeTruthy()
  })
  it('aborts reads on an imported tuple switch and ignores a late old publication', async () => {
    let finish!: (value: ImportedKnowledgeEnvelope) => void
    let finishHeads!: (value: Awaited<ReturnType<typeof nativeKnowledgeApi.importedHeads>>) => void
    vi.mocked(nativeKnowledgeApi.importedPage).mockImplementationOnce(() => new Promise(done => { finish = done }))
    vi.mocked(nativeKnowledgeApi.importedHeads).mockImplementationOnce(() => new Promise(done => { finishHeads = done }))
    mount(); await waitFor(() => expect(nativeKnowledgeApi.importedPage).toHaveBeenCalledOnce())
    await waitFor(() => expect(nativeKnowledgeApi.importedHeads).toHaveBeenCalledOnce())
    const oldPage = vi.mocked(nativeKnowledgeApi.importedPage).mock.calls[0][2]!
    const oldHeads = vi.mocked(nativeKnowledgeApi.importedHeads).mock.calls[0][1]!
    const oldRegistry = vi.mocked(nativeKnowledgeApi.importedContexts).mock.calls[0][0]!
    vi.mocked(nativeKnowledgeApi.importedPage).mockResolvedValue({ source: mirror, focused_source: null, native: { state: 'published', generation, page: { state: 'found', document: { ...document, title: 'Current imported document' } } } })
    vi.mocked(nativeKnowledgeApi.importedHeads).mockResolvedValue({ source: mirror, native: { state: 'published', generation, heads: [{ slug: 'current-head.md', title: 'Current imported head', updated_at: '-9223372036854775808' }] } })
    await userEvent.setup().click(screen.getByRole('button', { name: 'Switch imported tuple' }))
    await screen.findByRole('heading', { name: 'Current imported document' })
    const currentHeads = await screen.findByRole('navigation', { name: 'Published imported Knowledge pages' })
    await within(currentHeads).findByText('Current imported head')
    expect(nativeKnowledgeApi.importedHeads).toHaveBeenLastCalledWith(mirror, expect.any(AbortSignal))
    expect(oldPage.aborted).toBe(true); expect(oldHeads.aborted).toBe(true); expect(oldRegistry.aborted).toBe(true)
    await act(async () => {
      finish(found)
      finishHeads({ source, native: { state: 'published', generation, heads: [{ slug: 'late-old-head.md', title: 'Late old imported head', updated_at: '-9223372036854775808' }] } })
    })
    expect(screen.queryByRole('heading', { name: document.title })).toBeNull()
    expect(within(currentHeads).getByText('Current imported head')).toBeTruthy()
    expect(within(currentHeads).queryByText('Late old imported head')).toBeNull()
    expect(within(currentHeads).queryByText('late-old-head.md')).toBeNull()
  })
  it('rejects malformed, ambiguous and PROJECT-bearing imported URIs before any read', async () => {
    for (const path of ['/knowledge/explorer?library=unknown', '/knowledge/explorer?library=imported&source=%7B%7D', importedKnowledgeHref(source, document.slug) + '&source=%7B', importedKnowledgeHref(source) + '&selection=%7B%7D']) {
      const view = mount(path); await screen.findByText('Invalid imported Knowledge selection'); view.unmount()
    }
    expect(nativeKnowledgeApi.importedContexts).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.importedPage).not.toHaveBeenCalled()
    expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
  })
})
