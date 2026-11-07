import '@/test/contextControls'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi, type NativeKnowledgeAskResult } from '@/api/nativeKnowledge'
import { NativeKnowledgeAsk } from './NativeKnowledgeAsk'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { ask: vi.fn() } }))
vi.mock('./WikiMarkdown', () => ({ MarkdownContent: ({ content }: { content: string }) => <div>{content}</div> }))
const selection = { project_dir: "/ÿ", project_id: 'literal', project_version: 'branch' }
const heads = [{ slug: 'literal /page.md', title: 'Evidence page', updated_at: null }]
const answer: NativeKnowledgeAskResult = { status: 'answer', provided_slugs: [heads[0].slug], receipt: { completion: { answer: 'Observed answer', requested_model: ' literal model ', ordinal: 18446744073709551615n } } }
function draft() {
  fireEvent.change(screen.getByRole('textbox', { name: 'Knowledge question' }), { target: { value: ' literal question ' } })
  fireEvent.change(screen.getByRole('textbox', { name: 'Knowledge requested model' }), { target: { value: ' literal model ' } })
}
function submit() { fireEvent.click(screen.getByRole('button', { name: 'Ask question' })) }
async function choose(control: string, option: string) {
  fireEvent.keyDown(screen.getByRole('combobox', { name: control }), { key: 'ArrowDown' })
  fireEvent.click(await screen.findByRole('option', { name: option }))
}
beforeEach(() => { vi.clearAllMocks(); vi.mocked(nativeKnowledgeApi.ask).mockResolvedValue(answer) })

it('defaults to FTS question retrieval without starting work until Ask', async () => {
  render(<NativeKnowledgeAsk selection={selection} heads={heads} />)
  expect(screen.queryByRole('checkbox')).toBeNull()
  expect(screen.getByRole('combobox', {name:'Knowledge question evidence'}).textContent).toContain('Retrieve pages')
  expect(screen.getByRole('combobox', {name:'Knowledge question retrieval mode'}).textContent).toBe('fts')
  expect(nativeKnowledgeApi.ask).not.toHaveBeenCalled()
  draft(); submit(); await screen.findByText('Observed answer')
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][1]).toEqual({query:' literal question ',model:' literal model ',retrieval:{mode:'fts',top_k:5}})
})

it('keeps explicit manual opt-in, empty selection, literal model and observed answer/receipt', async () => {
  render(<NativeKnowledgeAsk selection={selection} heads={heads} />)
  await choose('Knowledge question evidence','Choose pages manually')
  draft(); submit()
  expect(screen.getByRole('group', {name:'Choose pages for this question'})).toBeTruthy()
  expect(screen.getByText(/This list is the published library, not ranked search results/)).toBeTruthy()
  expect(screen.getByText('No pages selected.')).toBeTruthy()
  await screen.findByText('Observed answer')
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][1]).toEqual({ query: ' literal question ', model: ' literal model ', selected_slugs: [] })
  expect(screen.getByText(/"ordinal": 18446744073709551615/)).toBeTruthy()
  fireEvent.click(screen.getByRole('checkbox', { name: /Evidence page/ })); submit()
  expect(screen.getByText('1 page selected.')).toBeTruthy()
  await waitFor(() => expect(nativeKnowledgeApi.ask).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[1][1]).toEqual({ query: ' literal question ', model: ' literal model ', selected_slugs: [heads[0].slug] })
})

it('uses exclusive retrieval and omits metric for fts, preserves large positive limits', async () => {
  render(<NativeKnowledgeAsk selection={selection} heads={heads} />); draft()
  await choose('Knowledge question evidence', 'Retrieve pages')
  fireEvent.change(screen.getByRole('textbox', { name: 'Knowledge question page limit' }), { target: { value: '9007199254740993' } }); submit()
  await screen.findByText('Observed answer')
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][1]).toEqual({ query: ' literal question ', model: ' literal model ', retrieval: { mode: 'fts', top_k: 9007199254740993n } })
  await choose('Knowledge question retrieval mode', 'hybrid')
  await choose('Knowledge question distance metric', 'dot'); submit()
  await waitFor(() => expect(nativeKnowledgeApi.ask).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[1][1]).toMatchObject({ retrieval: { mode: 'hybrid', top_k: 9007199254740993n, metric: 'dot' } })
})

it('preserves native progress, retries captured input and suppresses failed completion text', async () => {
  vi.mocked(nativeKnowledgeApi.ask).mockImplementationOnce(async (_selection, _input, options) => { options?.onProgress?.({ type: 'turn.started', ordinal: 18446744073709551615n }); throw new Error('response_limit') })
    .mockResolvedValueOnce({ ...answer, status: 'failed', error: 'execution_denied' })
  render(<NativeKnowledgeAsk selection={selection} heads={heads} />); draft(); submit()
  await screen.findByText('response_limit')
  expect(screen.getByText(/"type":"turn.started","ordinal":18446744073709551615/)).toBeTruthy()
  const captured = vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][1]
  fireEvent.click(screen.getByRole('button', { name: 'Retry question' }))
  await screen.findByText('Question result: failed')
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[1][1]).toEqual(captured)
  expect(screen.queryByRole('article', { name: 'Knowledge answer' })).toBeNull()
  expect(screen.getByText('execution_denied')).toBeTruthy()
})

it('aborts on cancel and scope change, hiding late progress, answer and denial', async () => {
  let finish!: (value: NativeKnowledgeAskResult) => void
  vi.mocked(nativeKnowledgeApi.ask).mockImplementationOnce(() => new Promise(done => { finish = done }))
  const view = render(<NativeKnowledgeAsk selection={selection} heads={heads} />); draft(); submit()
  const options = vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][2]!
  fireEvent.click(screen.getByRole('button', { name: 'Cancel question' })); expect(options.signal?.aborted).toBe(true)
  await act(async () => { options.onProgress?.({ type: 'late progress' }); finish(answer) })
  expect(screen.queryByText('Observed answer')).toBeNull(); expect(screen.queryByText(/late progress/)).toBeNull()
  let deny!: (reason: Error) => void
  vi.mocked(nativeKnowledgeApi.ask).mockImplementationOnce(() => new Promise((_done, reject) => { deny = reject }))
  fireEvent.click(screen.getByRole('button', { name: 'Retry question' }))
  const second = vi.mocked(nativeKnowledgeApi.ask).mock.calls[1][2]!
  view.rerender(<NativeKnowledgeAsk selection={{ ...selection, project_version: 'other' }} heads={[]} />)
  expect(second.signal?.aborted).toBe(true)
  await act(async () => deny(new Error('late denied')))
  expect(screen.queryByText('late denied')).toBeNull(); expect(screen.queryByText('Observed answer')).toBeNull()
})

it('keeps every published page selectable in a large evidence list', async () => {
  const pages = Array.from({ length: 200 }, (_, index) => ({ slug: `docs/page-${index}.md`, title: `Evidence ${index}`, updated_at: null }))
  render(<NativeKnowledgeAsk selection={selection} heads={pages} />)
  await choose('Knowledge question evidence','Choose pages manually')
  expect(screen.getAllByRole('checkbox')).toHaveLength(pages.length)
  fireEvent.click(screen.getByRole('checkbox', { name: 'Evidence 199 docs/page-199.md' }))
  fireEvent.click(screen.getByRole('checkbox', { name: 'Evidence 0 docs/page-0.md' }))
  draft(); submit()
  await screen.findByText('Observed answer')
  expect(vi.mocked(nativeKnowledgeApi.ask).mock.calls[0][1]).toEqual({ query: ' literal question ', model: ' literal model ', selected_slugs: [pages[199].slug, pages[0].slug] })
})
