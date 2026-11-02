import '@/test/contextControls'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nativeMemoryApi, type MemoryEnvelope, type NativeMemory, type NativeMemoryHistory, type NativeMemoryList } from '@/api/nativeMemory'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import { useAppStore } from '@/store/appStore'
import MemoryExplorerPage from './MemoryExplorerPage'

vi.mock('@/api/nativeMemory', async original => ({ ...await original<typeof import('@/api/nativeMemory')>(), nativeMemoryApi: { list: vi.fn(), get: vi.fn(), history: vi.fn() } }))
vi.mock('@/hooks/useToast', () => ({ showToast: vi.fn() }))
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: 'literal version' }
const memory: NativeMemory = { id: 'memory1', revision: 9007199254740993n, title: 'Literal title', body: '# Original body\n\nThe **native guidance**.', attributes: { classification: 'decision', important: true, mandatory: false }, tags: ['literal tag'], references: [{ target_kind: 'task', scope: { Project: 'project1' }, id: 'literal target', context: null, relation: 'supports', field: null }], reference_generation: 18446744073709551615n, created_by: 'fixture creator', updated_by: 'fixture updater', created_at: 0, updated_at: 10 }
const generation = { sdk_version: 18446744073709551615n, project_id: 'project1', project_version: selection.project_version, physical_branch: null, inherited_target: null }
const initial = { ...memory, revision: 1, title: 'Original title', body: 'Initial guidance.' }
const history: NativeMemoryHistory = { creation: { project_id: 'project1', operation_version: selection.project_version, actor: 'fixture creator', input: { idempotency_key: 'fixture-create', title: initial.title, body: initial.body, attributes: initial.attributes, tags: initial.tags, references: null }, memory: initial }, current: memory, deleted: false, revisions: [{ project_id: 'project1', operation_version: selection.project_version, actor: 'fixture updater', operation: { update: { expected_revision: 1, reason: 'literal change' } }, before: initial, after: memory }], generation }
const list = { state: 'published' as const, generation, memories: [memory] }
function page(path = projectRecordHref('memory', memory.id, selection)) { return render(<MemoryRouter initialEntries={[path]}><Routes><Route path="/memory/explorer/:scopeId/:memoryId?" element={<MemoryExplorerPage />} /></Routes></MemoryRouter>) }
beforeEach(() => {
  vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
  vi.clearAllMocks()
  useTaskWorkspace.setState({ module: 'memory', selection, contexts: [selection], phase: 'ready', error: '' })
  useAppStore.setState({ activeProjectKey: '', activeProjectOrigin: '', activeProjectId: '', activeProjectDir: '', projectsLoaded: true })
  vi.mocked(nativeMemoryApi.list).mockResolvedValue({ selection, native: list })
  vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: history })
})

describe('native Memory reader UI', () => {
  it('uses exact selection, renders canonical guidance and full before/after history with exact integer metadata', async () => {
    const user = userEvent.setup(); page()
    await screen.findByRole('heading', { name: 'Authoritative metadata' })
    expect(nativeMemoryApi.history).toHaveBeenCalledWith({ kind: 'project', selection }, 'memory1', expect.any(AbortSignal))
    expect(screen.getByText('Reference generation').nextElementSibling?.textContent).toBe('18446744073709551615')
    expect(screen.getByText('Complete history document').parentElement?.querySelector('pre')?.textContent).toContain('"revision": 9007199254740993')
    expect(screen.getByText('Creation and changes')).toBeTruthy()
    expect(within(screen.getByRole('article', { name: 'Memory record' })).getByRole('heading', { name: 'Original body' })).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Revision 1' }))
    expect(await screen.findByText('Initial guidance.')).toBeTruthy()
    expect(screen.getByText('Historical revision')).toBeTruthy()
  })

  it('distinguishes retained deletion, missing history and a causal error with Retry', async () => {
    vi.mocked(nativeMemoryApi.history).mockResolvedValueOnce({ selection, native: { ...history, deleted: true } })
    const deleted = page(); await screen.findByText('Memory deleted')
    expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull(); deleted.unmount()
    vi.mocked(nativeMemoryApi.history).mockResolvedValueOnce({ selection, native: null })
    const missing = page(); await screen.findByText('Memory not found'); missing.unmount()
    vi.mocked(nativeMemoryApi.history).mockRejectedValueOnce(new Error('HTTP 403')).mockResolvedValue({ selection, native: history })
    page(); expect(await screen.findByText('HTTP 403')).toBeTruthy()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry memory' }))
    expect(await screen.findByRole('heading', { name: 'Authoritative metadata' })).toBeTruthy()
  })

  it('preserves no corpus and published empty without treating an error as an empty healthy list', async () => {
    vi.mocked(nativeMemoryApi.list).mockResolvedValueOnce({ selection, native: { state: 'no_corpus' } })
    const absent = page('/memory/explorer/project'); await screen.findByText('No Memory corpus has been published'); absent.unmount()
    vi.mocked(nativeMemoryApi.list).mockResolvedValueOnce({ selection, native: { ...list, memories: [] } })
    const empty = page('/memory/explorer/project'); await screen.findByText('No memories match this view'); empty.unmount()
    vi.mocked(nativeMemoryApi.list).mockRejectedValueOnce(new Error('HTTP 500'))
    page('/memory/explorer/project'); await screen.findByText('HTTP 500')
    expect(screen.queryByText('No memories match this view')).toBeNull()
  })

  it('retains SDK ordering and forwards real boolean/classification filters without a query alias', async () => {
    const second = { ...memory, id: 'memory2', title: 'Second in SDK order' }
    let finish!: (value: MemoryEnvelope<NativeMemoryList>) => void
    vi.mocked(nativeMemoryApi.list).mockResolvedValueOnce({ selection, native: { ...list, memories: [second, memory] } }).mockImplementationOnce(() => new Promise(done => { finish = done }))
    const user = userEvent.setup(); page('/memory/explorer/project')
    const catalogue = screen.getByLabelText('Memory catalogue')
    await within(catalogue).findByText('Second in SDK order')
    expect(within(catalogue).getAllByRole('button').map(button => button.textContent)).toEqual(['Second in SDK order', 'Literal title'])
    await user.click(screen.getByLabelText('Filter importance')); await user.click(screen.getByRole('option', { name: 'Not important' }))
    await waitFor(() => expect(nativeMemoryApi.list).toHaveBeenLastCalledWith({ kind: 'project', selection }, { important: false }, expect.any(AbortSignal)))
    expect(within(catalogue).queryByText('Second in SDK order')).toBeNull()
    expect(within(catalogue).queryByText('No memories match this view')).toBeNull()
    await act(async () => finish({ selection, native: { ...list, memories: [{ ...second, attributes: { ...second.attributes, important: false } }] } }))
    expect(await within(catalogue).findByText('Second in SDK order')).toBeTruthy()
  })

  it('rejects an ambiguous USER namespace without issuing any reader request', async () => {
    page('/memory/explorer/user/memory1?scope_id=first&scope_id=second')
    await screen.findByText('The requested User Memory scope is ambiguous')
    expect(nativeMemoryApi.list).not.toHaveBeenCalled(); expect(nativeMemoryApi.history).not.toHaveBeenCalled()
  })

  it('aborts the old physical selection and ignores its late result', async () => {
    let finish!: (value: MemoryEnvelope<NativeMemoryList>) => void
    vi.mocked(nativeMemoryApi.list).mockImplementationOnce(() => new Promise(done => { finish = done }))
    const mirror = { ...selection, project_dir: "/m" }
    page('/memory/explorer/project')
    await waitFor(() => expect(nativeMemoryApi.list).toHaveBeenCalledOnce())
    const signal = vi.mocked(nativeMemoryApi.list).mock.calls[0][2]!
    vi.mocked(nativeMemoryApi.list).mockResolvedValue({ selection: mirror, native: { ...list, memories: [{ ...memory, title: 'Current mirror' }] } })
    act(() => useTaskWorkspace.setState({ selection: mirror, contexts: [selection, mirror] }))
    expect(await screen.findByText('Current mirror')).toBeTruthy(); expect(signal.aborted).toBe(true)
    await act(async () => finish({ selection, native: list }))
    expect(screen.queryByText('Literal title')).toBeNull()
  })

  it('reads USER independently of PROJECT selection and preserves its observed namespace', async () => {
    const scopeId = '["fixture-installation","https://issuer.invalid","fixture subject"]'
    const owner = { installation_id: 'fixture-installation', identity: { issuer: 'https://issuer.invalid', subject: 'fixture subject' } }
    vi.mocked(nativeMemoryApi.list).mockResolvedValue({ scope_id: scopeId, currentOwner: owner, native: list })
    vi.mocked(nativeMemoryApi.history).mockResolvedValue({ scope_id: scopeId, currentOwner: owner, native: history })
    useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'forbidden' })
    page('/memory/explorer/user/memory1?' + new URLSearchParams({ scope_id: scopeId }))
    await screen.findByRole('heading', { name: 'Authoritative metadata' })
    expect(nativeMemoryApi.list).toHaveBeenCalledWith({ kind: 'user', requested_scope_id: scopeId }, {}, expect.any(AbortSignal))
    expect(nativeMemoryApi.history).toHaveBeenCalledWith({ kind: 'user', requested_scope_id: scopeId }, 'memory1', expect.any(AbortSignal))
    const count = vi.mocked(nativeMemoryApi.list).mock.calls.length
    act(() => useTaskWorkspace.setState({ selection, phase: 'ready' }))
    expect(nativeMemoryApi.list).toHaveBeenCalledTimes(count)
  })
})
