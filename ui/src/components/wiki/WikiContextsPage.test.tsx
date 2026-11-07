import '@/test/contextControls'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { afterAll, beforeAll, beforeEach, expect, it, vi } from 'vitest'
import { nativeKnowledgeApi } from '@/api/nativeKnowledge'
import { parseNativeJson } from '@/api/nativeJson'
import { importedSourceKey } from '@/api/nativeRecordNavigation'
import WikiContextsPage from './WikiContextsPage'
import WikiExplorerPage from './WikiExplorerPage'

vi.mock('@/api/nativeKnowledge', async original => ({ ...await original<typeof import('@/api/nativeKnowledge')>(), nativeKnowledgeApi: { importedContexts: vi.fn(), importedHeads: vi.fn(), importedPage: vi.fn(), heads: vi.fn(), page: vi.fn() } }))
const source = { kind: 'imported_knowledge' as const, artifact: { id: 'literal@artifact', version: ' pinned / version ', context_alias: 'literal alias' } }
const generation = { corpus_uri: 'import-corpus', version: 18446744073709551615n, physical_branch: null, inherited_target: null }
function Location() { return <output aria-label="Current URI">{useLocation().search}</output> }
function mount() { return render(<MemoryRouter initialEntries={['/knowledge/contexts']}><Location /><Routes><Route path="/knowledge/contexts" element={<WikiContextsPage />} /><Route path="/knowledge/explorer" element={<WikiExplorerPage />} /></Routes></MemoryRouter>) }
const scrollToDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollTo')
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, 'scrollTo', { configurable: true, writable: true, value: vi.fn() })
})
afterAll(() => {
  if (scrollToDescriptor) Object.defineProperty(HTMLElement.prototype, 'scrollTo', scrollToDescriptor)
  else Reflect.deleteProperty(HTMLElement.prototype, 'scrollTo')
})
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(nativeKnowledgeApi.importedContexts).mockResolvedValue({ contexts: [source] })
  vi.mocked(nativeKnowledgeApi.importedHeads).mockResolvedValue({ source, native: { state: 'published', generation, heads: [{ slug: 'literal /slug.md', title: '', updated_at: '-9223372036854775808' }] } })
  vi.mocked(nativeKnowledgeApi.importedPage).mockResolvedValue({ source, focused_source: null, native: { state: 'published', generation, page: { state: 'page_absent' } } })
})
it('loads imported inventory without PROJECT and requires an explicit artifact selection', async () => {
  mount(); await screen.findByRole('combobox', { name: 'Imported Knowledge artifact' })
  expect(screen.getByText('Select an installed artifact')).toBeTruthy()
  expect(nativeKnowledgeApi.importedContexts).toHaveBeenCalledWith(expect.any(AbortSignal))
  expect(nativeKnowledgeApi.importedHeads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.importedPage).not.toHaveBeenCalled()
  expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
})
it('opens observed heads with the complete tuple, signed timestamp and literal slug in the URI', async () => {
  mount()
  fireEvent.keyDown(await screen.findByRole('combobox', { name: 'Imported Knowledge artifact' }), { key: 'ArrowDown' })
  fireEvent.click(await screen.findByRole('option', { name: /literal@artifact/ }))
  const library = await screen.findByRole('navigation', { name: 'Published imported Knowledge pages' })
  expect(library.textContent).toContain('-9223372036854775808')
  expect(screen.getByText('Publication version').nextElementSibling?.textContent).toBe('18446744073709551615')
  expect(nativeKnowledgeApi.importedHeads).toHaveBeenCalledWith(source, expect.any(AbortSignal))
  fireEvent.click(screen.getByRole('button', { name: /literal \/slug.md/ }))
  await screen.findByText('Knowledge page not found')
  const params = new URLSearchParams(screen.getByLabelText('Current URI').textContent!)
  expect(importedSourceKey(parseNativeJson(params.get('source')!))).toBe(importedSourceKey(source))
  expect(params.get('page')).toBe('literal /slug.md'); expect(params.has('selection')).toBe(false)
  expect(nativeKnowledgeApi.heads).not.toHaveBeenCalled(); expect(nativeKnowledgeApi.page).not.toHaveBeenCalled()
})
it('keeps registry failure causal and aborts a pending inventory on unmount', async () => {
  vi.mocked(nativeKnowledgeApi.importedContexts).mockRejectedValueOnce(new Error('registry unavailable'))
  const first = mount(); await screen.findByText('registry unavailable')
  expect(screen.queryByText('No Knowledge artifacts are installed')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: 'Retry artifacts' }))
  await screen.findByRole('combobox', { name: 'Imported Knowledge artifact' }); first.unmount()
  let finish!: (value: { contexts: typeof source[] }) => void
  vi.mocked(nativeKnowledgeApi.importedContexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  const next = mount(); await waitFor(() => expect(nativeKnowledgeApi.importedContexts).toHaveBeenCalledTimes(3))
  const signal = vi.mocked(nativeKnowledgeApi.importedContexts).mock.calls[2][0]!
  next.unmount(); expect(signal.aborted).toBe(true)
  await act(async () => finish({ contexts: [source] }))
})
