import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { memoryApi, type MemoryCatalog, type MemoryTrace } from '@/api/memory'
import { useAppStore } from '@/store/appStore'
import MemoryExplorerPage from './MemoryExplorerPage'

vi.mock('@/hooks/useToast', () => ({ showToast: vi.fn() }))
vi.mock('@/api/memory', async importOriginal => {
  const actual = await importOriginal<typeof import('@/api/memory')>()
  return {
    ...actual,
    memoryApi: {
      scopes: vi.fn(), list: vi.fn(), detail: vi.fn(), create: vi.fn(), update: vi.fn(), remove: vi.fn(),
    },
  }
})
vi.stubGlobal('matchMedia', vi.fn().mockImplementation(query => ({
  matches: false, media: query, onchange: null,
  addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
})))

const catalog: MemoryCatalog = {
  total: 1,
  types: ['decision', 'fact'],
  tags: ['architecture', 'memory', 'project'],
  results: [{
    id: '01MEMORY', title: 'Single authoritative store', type: 'decision',
    tags: ['memory', 'project', 'architecture'], important: true, mandatory: false,
    created_at: '2026-09-01T10:00:00Z', updated_at: '2026-09-02T10:00:00Z', revision: 2,
    snippet: 'Memory is searched directly in LanceDB.',
  }],
}

const trace: MemoryTrace = {
  memory_id: '01MEMORY',
  current: {
    key: '01MEMORY', id: '01MEMORY', status: 'current', title: 'Single authoritative store',
    body: '# Current design\n\nSearch the **authoritative LanceDB table** directly. [Trace source](https://example.com/trace).', type: 'decision',
    tags: ['memory', 'project', 'architecture'], important: true, mandatory: false,
    created_at: '2026-09-01T10:00:00Z', updated_at: '2026-09-02T10:00:00Z', revision: 2,
    updated_by: 'unit-current', previous: 'history/01MEMORY/01REV.md', scope: 'project',
    scope_id: '01PROJECT', content_hash: 'current-hash',
  },
  revisions: [{
    key: '01MEMORY/01REV', id: '01MEMORY', revision_id: '01REV', status: 'superseded',
    title: 'Compiled memory wiki', body: 'The **old design** used a projection.', type: 'fact',
    tags: ['memory', 'project'], important: false, mandatory: false,
    created_at: '2026-09-01T10:00:00Z', updated_at: '2026-09-01T10:00:00Z', revision: 1,
    updated_by: 'unit-original', next: '01MEMORY.md', scope: 'project', scope_id: '01PROJECT',
    content_hash: 'old-hash',
  }],
}

function Location() { return <output data-testid="location">{useLocation().pathname}</output> }

function renderExplorer(path = '/memory/explorer/project/01MEMORY') {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/memory/explorer/:scopeId/:memoryId?" element={<><MemoryExplorerPage /><Location /></>} />
      </Routes>
    </MemoryRouter>,
  )
}

describe('Memory Explorer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ activeProjectDir: '/project', projectName: 'Demo' })
    vi.mocked(memoryApi.list).mockResolvedValue(catalog)
    vi.mocked(memoryApi.detail).mockResolvedValue(trace)
    vi.mocked(memoryApi.create).mockResolvedValue(trace)
    vi.mocked(memoryApi.update).mockResolvedValue(trace)
    vi.mocked(memoryApi.remove).mockResolvedValue({ id: '01MEMORY', removed: true })
  })

  it('renders authoritative metadata and a navigable revision chain', async () => {
    const user = userEvent.setup()
    renderExplorer()

    expect(await screen.findByText('Authoritative metadata')).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Current design' })).toBeTruthy()
    expect(screen.getByText('authoritative LanceDB table')).toBeTruthy()
    expect(screen.getByRole('link', { name: 'Trace source' }).getAttribute('href')).toBe('https://example.com/trace')
    expect(screen.getByText('unit-current')).toBeTruthy()
    expect(screen.getByText('current-hash')).toBeTruthy()
    expect(screen.getByText('Revision chain')).toBeTruthy()
    expect(memoryApi.detail).toHaveBeenCalledWith('/project', 'project', '01MEMORY')

    await user.click(screen.getByRole('button', { name: /Revision 1/ }))
    expect(await screen.findByText('old design')).toBeTruthy()
    expect(screen.getByText('unit-original')).toBeTruthy()
    expect(screen.getByText('old-hash')).toBeTruthy()
    expect(screen.getAllByText('superseded').length).toBeGreaterThan(0)
  })

  it('sends domain filters to the dedicated memory catalogue API', async () => {
    const user = userEvent.setup()
    renderExplorer('/memory/explorer/project')
    await screen.findByText('Single authoritative store')

    await user.type(screen.getByLabelText('Search memories'), 'LanceDB')
    await user.selectOptions(screen.getByLabelText('Filter memory type'), 'decision')
    await user.selectOptions(screen.getByLabelText('Filter memory tag'), 'architecture')
    await user.selectOptions(screen.getByLabelText('Filter importance'), 'true')
    await user.selectOptions(screen.getByLabelText('Filter mandatory'), 'false')

    await waitFor(() => expect(memoryApi.list).toHaveBeenLastCalledWith({
      projectDir: '/project', scope: 'project', query: 'LanceDB', type: 'decision', tag: 'architecture', important: 'true', mandatory: 'false',
    }))
  })

  it('orders the catalogue by priority and then newest date', async () => {
    const base = catalog.results[0]
    vi.mocked(memoryApi.list).mockResolvedValueOnce({
      ...catalog,
      total: 5,
      results: [
        { ...base, id: 'normal', title: 'Normal newest', mandatory: false, important: false, updated_at: '2026-09-06T00:00:00Z' },
        { ...base, id: 'important-old', title: 'Important old', mandatory: false, important: true, updated_at: '2026-09-03T00:00:00Z' },
        { ...base, id: 'mandatory-old', title: 'Mandatory old', mandatory: true, important: false, updated_at: '2026-09-02T00:00:00Z' },
        { ...base, id: 'important-new', title: 'Important new', mandatory: false, important: true, updated_at: '2026-09-04T00:00:00Z' },
        { ...base, id: 'mandatory-new', title: 'Mandatory new', mandatory: true, important: false, updated_at: '2026-09-05T00:00:00Z' },
      ],
    })
    renderExplorer('/memory/explorer/project')

    const catalogue = screen.getByLabelText('Memory catalogue')
    await within(catalogue).findByText('Mandatory new')
    const titles = within(catalogue).getAllByRole('button').map(button => button.querySelector('p')?.textContent)
    expect(titles).toEqual(['Mandatory new', 'Mandatory old', 'Important new', 'Important old', 'Normal newest'])
  })

  it('creates and edits memories through domain-specific actions', async () => {
    const user = userEvent.setup()
    renderExplorer()
    await screen.findByText('Authoritative metadata')

    await user.click(screen.getByTitle('Create memory'))
    await user.type(screen.getByLabelText('Memory title'), 'New durable fact')
    await user.type(screen.getByLabelText('Memory content'), 'Remember this behavior.')
    await user.selectOptions(screen.getByLabelText('Memory type'), 'fact')
    await user.type(screen.getByLabelText('Memory tags'), 'runtime, trace')
    const createForm = screen.getByRole('dialog', { name: 'Create memory' })
    await user.click(within(createForm).getByRole('button', { name: 'Create memory' }))
    await waitFor(() => expect(memoryApi.create).toHaveBeenCalledWith('/project', 'project', expect.objectContaining({
      title: 'New durable fact', body: 'Remember this behavior.', type: 'fact', tags: ['runtime', 'trace'],
    })))

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    const form = screen.getByRole('dialog', { name: 'Edit memory' })
    const title = within(form).getByLabelText('Memory title')
    await user.clear(title)
    await user.type(title, 'Refined authoritative store')
    await user.click(within(form).getByLabelText('Mandatory memory'))
    await user.click(within(form).getByRole('button', { name: 'Save revision' }))
    await waitFor(() => expect(memoryApi.update).toHaveBeenCalledWith('/project', 'project', '01MEMORY', expect.objectContaining({
      title: 'Refined authoritative store', mandatory: true,
    })))
  })

  it('does not remove a memory until the user confirms', async () => {
    const user = userEvent.setup()
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true)
    renderExplorer()
    await screen.findByText('Authoritative metadata')

    await user.click(screen.getByRole('button', { name: 'Remove' }))
    expect(memoryApi.remove).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Remove' }))
    await waitFor(() => expect(memoryApi.remove).toHaveBeenCalledWith('/project', 'project', '01MEMORY'))
    expect(screen.getByTestId('location').textContent).toBe('/memory/explorer/project')
    confirm.mockRestore()
  })
})
