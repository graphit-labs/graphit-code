import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, useLocation, useNavigate } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi, type KnowledgeEnvelope, type NativeKnowledgeDocument, type NativeKnowledgePage } from '@/api/nativeKnowledge'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { parseNativeJson } from '@/api/nativeJson'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import WikiExplorerPage from './WikiExplorerPage'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { heads: vi.fn(), page: vi.fn(), search:vi.fn() } }))
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
const generation = { corpus_uri: 'fixture-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null }
const document: NativeKnowledgeDocument = { slug: 'literal /slug.md', title: 'Literal document', body: '---\nrecorded: true\n---\n\n# Original heading\n\n## Content\n\nNative **guidance**.', kind: 'guide', summary: 'Original summary', description: null, summary_truncated: false, source_path: 'docs/literal.md', physical_digest: null, compilation_digest: Array(32).fill(7), recipe: 'literal recipe', recipe_parameters: { fallback_title: 'fallback', kind_on_absent: 'guide', max_summary_bytes: 9007199254740993n }, updated_at: '-9223372036854775808', stale_since: null, stale_after: { seconds: 18446744073709551615n, nanoseconds: 999999999 }, resolved_wiki_targets: ['literal target.md', 'literal target.md'] }
const found: NativeKnowledgePage = { state: 'published', generation, page: { state: 'found', document } }
const mirror = { ...selection, project_dir: "/m" }
function SwitchSelection() { return <button onClick={() => useTaskWorkspace.setState({ selection: mirror })}>Switch physical selection</button> }
function BrowserHistory() {
  const navigate = useNavigate(), location = useLocation()
  return <><button onClick={() => navigate(-1)}>Browser back</button><button onClick={() => navigate(1)}>Browser forward</button><output aria-label="Current URL">{location.search}</output></>
}
function page(path = projectRecordHref('knowledge', document.slug, selection)) { return render(<MemoryRouter initialEntries={[path]}><BrowserHistory /><SwitchSelection /><WikiExplorerPage /></MemoryRouter>) }
beforeEach(() => {
  vi.clearAllMocks()
  HTMLElement.prototype.scrollIntoView = vi.fn()
  HTMLElement.prototype.scrollTo = vi.fn()
  window.scrollTo = vi.fn()
  vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
  useTaskWorkspace.setState({ module: 'knowledge', selection, contexts: [selection, mirror], phase: 'ready', error: '' })
  vi.mocked(nativeKnowledgeApi.heads).mockResolvedValue({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native: { state: 'published', generation, heads: [{ slug: document.slug, title: document.title, updated_at: document.updated_at }] } })
  vi.mocked(nativeKnowledgeApi.page).mockResolvedValue({ selection, source: { kind: 'project_knowledge' }, focused_source: null, native: found })
})

describe('canonical PROJECT Knowledge reader', () => {
  it.each(['catalogue', 'search'])('reveals an initial %s selection immediately through loading, error and retry', async origin => {
    let failPage!: (error:Error) => void
    vi.mocked(nativeKnowledgeApi.page).mockImplementationOnce(() => new Promise((_done,reject) => { failPage=reject }))
    const focus = {slug:document.slug,title:document.title,source_path:document.source_path,compilation_digest:document.compilation_digest,namespace:null,witness:null}
    vi.mocked(nativeKnowledgeApi.search).mockResolvedValue({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'hits',generation,total:1,hits:[{focused_source:focus,lexical_score:2}],next_cursor:null}})
    const library = new URL(projectRecordHref('knowledge','',selection),'http://localhost'); library.searchParams.delete('page')
    page(library.pathname+library.search)
    const user = userEvent.setup()
    if (origin === 'search') {
      await user.type(screen.getByRole('textbox',{name:'Search Knowledge pages'}),'guidance')
      await user.click(screen.getByRole('button',{name:'Search pages'}))
    }
    const catalogue = screen.getByRole('navigation',{name:origin === 'search' ? 'Knowledge search results' : 'Published Knowledge pages'})
    await user.click(await within(catalogue).findByRole('button',{name:document.title}))
    const reader = within(catalogue).getByRole('article',{name:'Knowledge document'})
    const scroll = vi.mocked(HTMLElement.prototype.scrollTo)
    expect(within(reader).getByText('Loading Knowledge document…')).toBeTruthy()
    expect(reader).toHaveFocus()
    expect(scroll).toHaveBeenCalledTimes(1)
    expect(scroll.mock.contexts.at(-1)).toBe(reader.closest('.work-page'))
    expect(scroll).toHaveBeenLastCalledWith({top:0,behavior:'instant'})
    expect(HTMLElement.prototype.scrollIntoView).not.toHaveBeenCalled()
    expect(window.scrollTo).not.toHaveBeenCalled()
    await waitFor(() => expect(nativeKnowledgeApi.page).toHaveBeenCalledTimes(1))
    await act(async () => failPage(new Error('HTTP 503 initial document unavailable')))
    expect(within(reader).getByText('HTTP 503 initial document unavailable')).toBeTruthy()
    expect(reader).toHaveFocus()
    expect(scroll).toHaveBeenCalledTimes(1)
    await user.click(within(reader).getByRole('button',{name:'Retry document'}))
    const title = await within(reader).findByRole('heading',{name:document.title})
    await waitFor(() => expect(title).toHaveFocus())
    expect(scroll).toHaveBeenCalledTimes(1)
    expect(nativeKnowledgeApi.page).toHaveBeenLastCalledWith(selection,document.slug,expect.any(AbortSignal))
  })

  it('reveals a direct URL while loading and after a late roster moves the reader, retaining the actual error', async () => {
    let finishHeads!: (reply:Awaited<ReturnType<typeof nativeKnowledgeApi.heads>>) => void
    let failPage!: (error:Error) => void
    vi.mocked(nativeKnowledgeApi.heads).mockImplementationOnce(() => new Promise(done => { finishHeads=done }))
    vi.mocked(nativeKnowledgeApi.page).mockImplementationOnce(() => new Promise((_done,reject) => { failPage=reject }))
    page()
    const panel = screen.getByRole('article',{name:'Knowledge document'})
    const scroll = vi.mocked(HTMLElement.prototype.scrollTo)
    expect(panel).toHaveFocus()
    expect(within(panel).getByText('Loading Knowledge document…')).toBeTruthy()
    expect(scroll).toHaveBeenLastCalledWith({top:0,behavior:'instant'})
    expect(HTMLElement.prototype.scrollIntoView).not.toHaveBeenCalled()
    expect(window.scrollTo).not.toHaveBeenCalled()
    expect(scroll.mock.contexts.at(-1)).toBe(panel.closest('.work-page'))
    const beforeRoster = scroll.mock.calls.length
    await waitFor(() => expect(nativeKnowledgeApi.heads).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(nativeKnowledgeApi.page).toHaveBeenCalledTimes(1))
    const preceding = Array.from({length:30},(_,index) => ({slug:`earlier-${index}`,title:`Earlier page ${index}`,updated_at:null}))
    await act(async () => finishHeads({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'published',generation,heads:[...preceding,{slug:document.slug,title:document.title,updated_at:null}]}}))
    expect(panel.closest('.knowledge-page-item')?.previousElementSibling?.textContent).toContain('Earlier page 29')
    expect(scroll.mock.calls.length).toBeGreaterThan(beforeRoster)
    expect(scroll.mock.contexts.at(-1)).toBe(panel.closest('.work-page'))
    await act(async () => failPage(new Error('HTTP 403 exact direct page denied')))
    expect(within(panel).getByText('HTTP 403 exact direct page denied')).toBeTruthy()
    expect(panel).toHaveFocus()
    expect(nativeKnowledgeApi.page).toHaveBeenCalledTimes(1)
    expect(nativeKnowledgeApi.page).toHaveBeenCalledWith(selection,document.slug,expect.any(AbortSignal))
  })

  it('opens a search hit beside that hit with one reader and keeps sequence navigation in Search', async () => {
    const user = userEvent.setup()
    const nextSlug = 'outside /search.md'
    const focus = {slug:document.slug,title:document.title,source_path:document.source_path,compilation_digest:document.compilation_digest,namespace:null,witness:null}
    vi.mocked(nativeKnowledgeApi.search).mockResolvedValue({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'hits',generation,total:1,hits:[{focused_source:focus,lexical_score:2}],next_cursor:null}})
    let failNext!: (error:Error) => void
    let nextAttempts = 0
    vi.mocked(nativeKnowledgeApi.page).mockImplementation(async (_selection, slug) => {
      if (slug === nextSlug && ++nextAttempts === 1) return new Promise((_done,reject) => { failNext = reject })
      return {selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'published',generation,page:{state:'found',document:{...document,slug,title:slug===nextSlug?'Outside results':document.title,next:slug===nextSlug?null:{target:nextSlug,slug:nextSlug,title:'Outside results'}}}}}
    })
    const library = new URL(projectRecordHref('knowledge','',selection),'http://localhost'); library.searchParams.delete('page')
    page(library.pathname+library.search)
    await user.type(screen.getByRole('textbox',{name:'Search Knowledge pages'}),'guidance')
    await user.click(screen.getByRole('button',{name:'Search pages'}))
    const results = screen.getByRole('navigation',{name:'Knowledge search results'})
    await user.click(await within(results).findByRole('button',{name:document.title}))
    const title = await within(results).findByRole('heading',{name:document.title})
    await waitFor(() => expect(title).toHaveFocus())
    const scroll = vi.mocked(HTMLElement.prototype.scrollTo)
    expect(scroll).toHaveBeenCalledTimes(1)
    expect(scroll.mock.contexts.at(-1)).toBe(within(results).getByRole('article',{name:'Knowledge document'}).closest('.work-page'))
    expect(screen.getAllByRole('article',{name:'Knowledge document'})).toHaveLength(1)
    expect(within(screen.getByRole('navigation',{name:'Published Knowledge pages'})).queryByRole('article')).toBeNull()
    expect(nativeKnowledgeApi.page).toHaveBeenCalledTimes(1)
    await user.click(within(results).getByRole('button',{name:'Next page: Outside results'}))
    const pendingReader = within(results).getByRole('article',{name:'Knowledge document'})
    expect(within(pendingReader).getByText('Loading Knowledge document…')).toBeTruthy()
    expect(pendingReader).toHaveFocus()
    expect(scroll).toHaveBeenLastCalledWith({top:0,behavior:'instant'})
    expect(HTMLElement.prototype.scrollIntoView).not.toHaveBeenCalled()
    expect(window.scrollTo).not.toHaveBeenCalled()
    expect(scroll.mock.contexts.at(-1)).toBe(pendingReader.closest('.work-page'))
    await act(async () => failNext(new Error('HTTP 503 continuation unavailable')))
    expect(within(pendingReader).getByText('HTTP 503 continuation unavailable')).toBeTruthy()
    expect(pendingReader).toHaveFocus()
    expect(scroll).toHaveBeenCalledTimes(2)
    await user.click(within(pendingReader).getByRole('button',{name:'Retry document'}))
    await within(results).findByRole('heading',{name:'Outside results'})
    expect(scroll).toHaveBeenCalledTimes(2)
    expect(within(results).getByText('The opened page is outside these search results.')).toBeTruthy()
    expect(nativeKnowledgeApi.search).toHaveBeenCalledTimes(1)
    expect(nativeKnowledgeApi.page).toHaveBeenLastCalledWith(selection,nextSlug,expect.any(AbortSignal))
    await user.click(screen.getByRole('button',{name:'Browser back'}))
    await within(results).findByRole('heading',{name:document.title})
    expect(screen.getAllByRole('article',{name:'Knowledge document'})).toHaveLength(1)
    await user.click(screen.getByRole('button',{name:'Browser forward'}))
    await within(results).findByRole('heading',{name:'Outside results'})
    expect(scroll).toHaveBeenCalledTimes(4)
    await user.click(within(results).getByRole('button',{name:nextSlug}))
    expect(screen.queryByRole('article',{name:'Knowledge document'})).toBeNull()
    expect(results).toHaveFocus()
  })

  it('expands beside the selected row and preserves keyboard, collapse focus and browser history', async () => {
    const user = userEvent.setup()
    const next = { slug: 'following /page.md', title: 'Following document', updated_at: null }
    vi.mocked(nativeKnowledgeApi.heads).mockResolvedValue({ selection, source: {kind:'project_knowledge'}, focused_source:null, native: {state:'published', generation, heads:[{slug:document.slug,title:document.title,updated_at:null}, next]} })
    vi.mocked(nativeKnowledgeApi.page).mockImplementation(async (_selection, slug) => ({ selection, source:{kind:'project_knowledge'}, focused_source:null, native:{state:'published',generation,page:{state:'found',document:{...document,slug,title:slug === next.slug ? next.title : document.title}}} }))
    page()
    const reader = await screen.findByRole('article', {name:'Knowledge document'})
    const title = await within(reader).findByRole('heading', {name:document.title})
    await waitFor(() => expect(title).toHaveFocus())
    vi.mocked(HTMLElement.prototype.scrollTo).mockClear()
    const trigger = screen.getByRole('button', {name:document.title})
    expect(trigger).toHaveAttribute('aria-expanded','true')
    expect(reader.closest('.knowledge-page-item')?.nextElementSibling?.textContent).toContain(next.title)
    expect(reader.parentElement?.id).toBe(trigger.getAttribute('aria-controls'))
    expect(title).toHaveAttribute('tabindex','-1')
    await user.click(trigger)
    expect(screen.queryByRole('article', {name:'Knowledge document'})).toBeNull()
    expect(trigger).toHaveFocus()
    expect(trigger).toHaveAttribute('aria-expanded','false')
    expect(screen.getByLabelText('Current URL').textContent).not.toContain('page=')
    await user.keyboard('{End}')
    expect(screen.getByRole('button', {name:next.title})).toHaveFocus()
    await user.keyboard('{Enter}')
    await screen.findByRole('heading', {name:next.title})
    expect(nativeKnowledgeApi.page).toHaveBeenLastCalledWith(selection,next.slug,expect.any(AbortSignal))
    expect(HTMLElement.prototype.scrollTo).toHaveBeenCalledTimes(1)
    expect(vi.mocked(HTMLElement.prototype.scrollTo).mock.contexts.at(-1)).toBe(screen.getByRole('article',{name:'Knowledge document'}).closest('.work-page'))
    await user.click(screen.getByRole('button', {name:'Browser back'}))
    await waitFor(() => expect(screen.queryByRole('article', {name:'Knowledge document'})).toBeNull())
    await user.click(screen.getByRole('button', {name:'Browser back'}))
    await screen.findByRole('heading', {name:document.title})
    await user.click(screen.getByRole('button', {name:'Browser forward'}))
    await waitFor(() => expect(screen.queryByRole('article', {name:'Knowledge document'})).toBeNull())
    expect(HTMLElement.prototype.scrollTo).toHaveBeenCalledTimes(2)
  })

  it('uses resolved sequence slugs in the same scope and keeps an unresolved authored target local', async () => {
    const next = { target:'Authored next',slug:'literal /next.md',title:'Next guide' }
    const distant = Array.from({length:30},(_,index) => ({slug:`other-${index}`,title:`Other page ${index}`,updated_at:null}))
    vi.mocked(nativeKnowledgeApi.heads).mockResolvedValue({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'published',generation,heads:[{slug:document.slug,title:document.title,updated_at:null},...distant,{slug:next.slug,title:next.title,updated_at:null}]}})
    vi.mocked(nativeKnowledgeApi.page).mockResolvedValueOnce({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'published',generation,page:{state:'found',document:{...document,previous:{target:'Missing prior',slug:null,title:null},next}}}})
      .mockResolvedValueOnce({selection,source:{kind:'project_knowledge'},focused_source:null,native:{state:'published',generation,page:{state:'found',document:{...document,slug:next.slug,title:next.title}}}})
    page()
    await screen.findByRole('heading',{name:document.title})
    expect(screen.getByText('Previous page unavailable')).toBeTruthy()
    expect(screen.getByText('Previous page unavailable').parentElement?.textContent).toContain('Missing prior')
    expect(screen.queryByRole('button',{name:/Previous page:/})).toBeNull()
    await screen.findByRole('button',{name:next.title})
    vi.mocked(HTMLElement.prototype.scrollTo).mockClear()
    expect(HTMLElement.prototype.scrollTo).not.toHaveBeenCalled()
    await userEvent.setup().click(screen.getByRole('button',{name:'Next page: Next guide'}))
    await waitFor(() => expect(nativeKnowledgeApi.page).toHaveBeenLastCalledWith(selection,next.slug,expect.any(AbortSignal)))
    const nextReader = screen.getByRole('article',{name:'Knowledge document'})
    expect(vi.mocked(HTMLElement.prototype.scrollTo).mock.contexts.at(-1)).toBe(nextReader.closest('.work-page'))
    expect(HTMLElement.prototype.scrollTo).toHaveBeenLastCalledWith({top:0,behavior:'instant'})
  })

  it('reads an exact reload selection and preserves body, all metadata, integers and repeated literal targets', async () => {
    const user = userEvent.setup(); page()
    const dossier = await screen.findByRole('article', { name: 'Knowledge document' })
    await within(dossier).findByRole('heading', { name: document.title })
    expect(nativeKnowledgeApi.page).toHaveBeenCalledWith(selection, document.slug, expect.any(AbortSignal))
    const renderedBody = within(dossier).getByRole('heading', {name:'Document body'}).closest('section')!
    expect(renderedBody.textContent).not.toContain('recorded: true')
    const metadata = within(dossier).getByText('Document metadata').closest('details')!
    const identifiers = within(dossier).getByText('Linked page identifiers').closest('details')!
    expect(metadata.open).toBe(false); expect(identifiers.open).toBe(false)
    await user.click(within(dossier).getByText('Document metadata'))
    await user.click(within(dossier).getByText('Linked page identifiers'))
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
