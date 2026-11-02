import '@/test/contextControls'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, useNavigate } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi, type KnowledgeEnvelope, type NativeKnowledgeDocument, type NativeKnowledgePage } from '@/api/nativeKnowledge'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { parseNativeJson } from '@/api/nativeJson'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import WikiExplorerPage from './WikiExplorerPage'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { heads: vi.fn(), page: vi.fn() } }))
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
const generation = { corpus_uri: 'fixture-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null }
const document: NativeKnowledgeDocument = { slug: 'literal /slug.md', title: 'Literal document', body: '---\nrecorded: true\n---\n\n# Original heading\n\n## Content\n\nNative **guidance**.', kind: 'guide', summary: 'Original summary', description: null, summary_truncated: false, source_path: 'docs/literal.md', physical_digest: null, compilation_digest: Array(32).fill(7), recipe: 'literal recipe', recipe_parameters: { fallback_title: 'fallback', kind_on_absent: 'guide', max_summary_bytes: 9007199254740993n }, updated_at: '-9223372036854775808', stale_since: null, stale_after: { seconds: 18446744073709551615n, nanoseconds: 999999999 }, resolved_wiki_targets: ['literal target.md', 'literal target.md'] }
const found: NativeKnowledgePage = { state: 'published', generation, page: { state: 'found', document } }
const mirror = { ...selection, project_dir: "/m" }
function SwitchSelection() { const navigate = useNavigate(); return <button onClick={() => navigate(projectRecordHref('knowledge', document.slug, mirror))}>Switch physical selection</button> }
function page(path = projectRecordHref('knowledge', document.slug, selection)) { return render(<MemoryRouter initialEntries={[path]}><SwitchSelection /><WikiExplorerPage /></MemoryRouter>) }
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
  useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'ready', error: '' })
  vi.mocked(nativeKnowledgeApi.heads).mockResolvedValue({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native: { state: 'published', generation, heads: [{ slug: document.slug, title: document.title, updated_at: document.updated_at }] } })
  vi.mocked(nativeKnowledgeApi.page).mockResolvedValue({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native: found })
})

describe('canonical PROJECT Knowledge reader', () => {
  it('reads an exact reload selection and preserves body, all metadata, integers and repeated literal targets', async () => {
    const user = userEvent.setup(); page()
    const dossier = await screen.findByRole('article', { name: 'Knowledge document' })
    await within(dossier).findByRole('heading', { name: document.title })
    expect(nativeKnowledgeApi.page).toHaveBeenCalledWith(selection, document.slug, expect.any(AbortSignal))
    expect(screen.getByText('Maximum summary bytes').nextElementSibling?.textContent).toBe('9007199254740993')
    expect(screen.getByText('Updated at (nanoseconds)').nextElementSibling?.textContent).toBe(document.updated_at)
    expect(parseNativeJson(screen.getByText('Complete publication document').parentElement!.querySelector('pre')!.textContent!)).toEqual(found)
    expect(screen.getAllByRole('button', { name: 'literal target.md' })).toHaveLength(2)
    await user.click(screen.getByRole('button', { name: 'Raw body' }))
    expect(within(dossier).getByRole('heading', { name: 'Document body' }).closest('section')!.querySelector('pre')!.textContent).toBe(document.body)
    const copy = vi.spyOn(navigator.clipboard, 'writeText')
    await user.click(screen.getByRole('button', { name: 'Copy Markdown' }))
    expect(copy).toHaveBeenCalledWith(document.body)
    await user.click(screen.getAllByRole('button', { name: 'literal target.md' })[0])
    await waitFor(() => expect(nativeKnowledgeApi.page).toHaveBeenLastCalledWith(selection, 'literal target.md', expect.any(AbortSignal)))
  })

  it('distinguishes absent corpus, empty publication and absent slug and preserves causal errors with Retry', async () => {
    for (const [native, title] of [
      [{ state: 'no_corpus' }, 'No Knowledge corpus has been published'],
      [{ state: 'published', generation, page: { state: 'published_empty' } }, 'The Knowledge publication is empty'],
      [{ state: 'published', generation, page: { state: 'page_absent' } }, 'Knowledge page not found'],
    ] as const) {
      vi.mocked(nativeKnowledgeApi.page).mockResolvedValueOnce({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native })
      const rendered = page(); await within(screen.getByRole('article', { name: 'Knowledge document' })).findByText(title); rendered.unmount()
    }
    vi.mocked(nativeKnowledgeApi.page).mockRejectedValueOnce(new Error('HTTP 403'))
    page(); await screen.findByText('HTTP 403')
    expect(screen.queryByText('Knowledge page not found')).toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry document' }))
    expect(await screen.findByRole('heading', { name: document.title })).toBeTruthy()
  })

  it('aborts both old reads on a physical scope switch and ignores a late old document', async () => {
    let finish!: (value: KnowledgeEnvelope<NativeKnowledgePage>) => void
    vi.mocked(nativeKnowledgeApi.page).mockImplementationOnce(() => new Promise(done => { finish = done }))
    page(); await waitFor(() => expect(nativeKnowledgeApi.page).toHaveBeenCalledOnce())
    const oldPageSignal = vi.mocked(nativeKnowledgeApi.page).mock.calls[0][2]!
    const oldHeadsSignal = vi.mocked(nativeKnowledgeApi.heads).mock.calls[0][1]!
    vi.mocked(nativeKnowledgeApi.heads).mockResolvedValue({ selection: mirror, source: { kind: 'project_knowledge' }, focused_source: null, native: { state: 'published', generation, heads: [] } })
    vi.mocked(nativeKnowledgeApi.page).mockResolvedValue({ selection: mirror, source: { kind: 'project_knowledge' }, focused_source: null, native: { state: 'published', generation, page: { state: 'found', document: { ...document, title: 'Current mirror document' } } } })
    await userEvent.setup().click(screen.getByRole('button', { name: 'Switch physical selection' }))
    expect(await screen.findByRole('heading', { name: 'Current mirror document' })).toBeTruthy()
    expect(oldPageSignal.aborted).toBe(true); expect(oldHeadsSignal.aborted).toBe(true)
    await act(async () => finish({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native: found }))
    expect(screen.queryByRole('heading', { name: document.title })).toBeNull()
  })

  it('rejects malformed selections and ambiguous page requests before any native read', async () => {
    const malformed = page('/knowledge/explorer?selection=%7B%7D&page=literal'); await screen.findByText('Invalid Knowledge selection'); malformed.unmount()
    page(projectRecordHref('knowledge', document.slug, selection) + '&page=other')
    await screen.findByText('Invalid Knowledge page')
    expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
  })
})
