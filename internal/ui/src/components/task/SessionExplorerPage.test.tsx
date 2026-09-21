import { WorkspaceRefreshProvider } from "@/components/layout/WorkspaceRefresh"
import "@/test/contextControls"
import { WorkspaceSelectors } from "@/components/layout/WorkspaceSelectors"
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { sessionApi, type Session, type SessionDetail, type SessionSearchResult } from '@/api/taskSession'
import { useAppStore } from '@/store/appStore'
import SessionExplorerPage from './SessionExplorerPage'

vi.mock('@/hooks/useToast', () => ({ showToast: vi.fn() }))
vi.mock('@/api/taskSession', async importOriginal => {
  const actual = await importOriginal<typeof import('@/api/taskSession')>()
  return { ...actual, sessionApi: { list: vi.fn(), get: vi.fn() } }
})
vi.stubGlobal('matchMedia', vi.fn().mockImplementation(query => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: vi.fn(),
  removeListener: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
})))

const firstSession: Session = {
  id: 'ses-aaaa', project_id: 'project-1', idempotency_key: 'first', title: 'Archive export session',
  description: 'Preserve the **archive export** demand across turns.', strategy: 'Investigate then split delivery.',
  status: 'in_progress', owner: 'agent-a', claim_epoch: 1, checkpoint_sequence: 1,
  progress_summary: '**Producer** landed.', next_step: 'Verify the **Session Explorer** UI.',
  created_at: '2026-09-04T10:00:00Z', updated_at: '2026-09-04T11:00:00Z', revision: 4,
}

const secondSession: Session = {
  ...firstSession, id: 'ses-bbbb', idempotency_key: 'second', title: 'Second session',
  description: 'Resolve a follow-up demand.', status: 'open', owner: undefined,
  progress_summary: undefined, next_step: undefined, revision: 1,
}

const firstSummary: SessionSearchResult = {
  id: firstSession.id, title: firstSession.title, status: firstSession.status,
  owner: firstSession.owner, updated_at: firstSession.updated_at, revision: firstSession.revision,
}

const secondSummary: SessionSearchResult = {
  id: secondSession.id, title: secondSession.title, status: secondSession.status,
  owner: secondSession.owner, updated_at: secondSession.updated_at, revision: secondSession.revision,
}

const firstDetail: SessionDetail = {
  session: firstSession,
  events: [],
  checkpoints: [{
    key: 'ses-aaaa/1', session_id: 'ses-aaaa', sequence: 1, revision: 2, actor: 'agent-a', at: '2026-09-04T10:30:00Z',
    summary: '**Endpoint** landed.', problems: 'Retry identifier was late.', decisions: 'Preserve the existing job id.',
    next_step: 'Verify the UI renders checkpoints.',
  }],
  spec_revisions: [{
    key: 'ses-aaaa/1', session_id: 'ses-aaaa', source_revision: 2, actor: 'agent-a', reason: 'User added a **CSV** requirement.', at: '2026-09-04T10:15:00Z',
    before: { title: 'Archive export session', description: 'Initial **description**.', strategy: 'Initial strategy.' },
    after: { title: 'Archive export session', description: 'Revised **description**.', strategy: 'Revised strategy.' },
  }],
  tasks: [{ id: 'tsk-linked', session_id: 'ses-aaaa', title: 'Linked worker task', type: 'task', status: 'completed', priority: 1, flagged: false, ready: true, updated_at: '2026-09-04T10:45:00Z' }],
}

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>
}

describe('Session Explorer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ loadProjects: vi.fn(async () => {}), projectsError: "" })
    useAppStore.setState({ activeProjectDir: '/project', projectName: 'Demo', projects: [], projectsLoaded: false })
    vi.mocked(sessionApi.list).mockImplementation(async options => options.cursor
      ? { results: [secondSummary], next_cursor: '' }
      : { results: [firstSummary], next_cursor: 'page-2' })
    vi.mocked(sessionApi.get).mockImplementation(async (_projectDir, id) => ({
      ...firstDetail,
      session: { ...firstDetail.session, id },
    }))
  })

  it('loads a bounded catalogue, renders exact detail with checkpoints/revisions, and appends the next page', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/sessions/:sessionId?" element={<><SessionExplorerPage /><Location /></>} />
        </Routes>
      </WorkspaceRefreshProvider></MemoryRouter>,
    )

    expect(await screen.findByRole('heading', { name: 'Request' })).toBeTruthy()
    expect(screen.getByText('archive export')).toBeTruthy()
    expect(screen.getAllByRole('heading', { name: 'Strategy' })[0]).toBeTruthy()
    expect(screen.getByText('Producer', { selector: 'strong' })).toBeTruthy()
    expect(screen.getByText('Endpoint', { selector: 'strong' })).toBeTruthy()
    expect(screen.getByText('Retry identifier was late.')).toBeTruthy()
    expect(screen.getByText('Preserve the existing job id.')).toBeTruthy()
    expect(screen.getByText('Linked worker task')).toBeTruthy()
    expect(sessionApi.list).toHaveBeenCalledWith({ projectDir: '/project', query: undefined, status: 'all', active: false, pageSize: 20, cursor: undefined })
    expect(sessionApi.get).toHaveBeenCalledWith('/project', 'ses-aaaa')

    const sessionList = screen.getByLabelText('Session catalogue')
    await user.click(within(sessionList).getByRole('button', { name: 'Load more' }))
    await waitFor(() => expect(sessionApi.list).toHaveBeenCalledWith({ projectDir: '/project', query: undefined, status: 'all', active: false, pageSize: 20, cursor: 'page-2' }))
    expect(await within(sessionList).findByText('Second session')).toBeTruthy()

    await user.click(within(sessionList).getByText('Second session'))
    await waitFor(() => expect(sessionApi.get).toHaveBeenCalledWith('/project', 'ses-bbbb'))
    expect(screen.getByTestId('location').textContent).toBe('/task/sessions/ses-bbbb')
  })

  it('navigates to a linked task and back to the tasks view via the mode toggle', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/sessions/:sessionId?" element={<><SessionExplorerPage /><Location /></>} />
          <Route path="/task/explorer/:taskId" element={<Location />} />
        </Routes>
      </WorkspaceRefreshProvider></MemoryRouter>,
    )

    await screen.findByText('Linked worker task')
    await user.click(screen.getByText('Linked worker task'))
    expect(screen.getByTestId('location').textContent).toBe('/task/explorer/tsk-linked')
  })

  it('filters by status and active-only, and searches sessions', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /></Routes>
      </WorkspaceRefreshProvider></MemoryRouter>,
    )

    await screen.findByText('Archive export session')
    const selector = screen.getByRole('combobox', { name: 'Filter session status' })
    await user.click(selector)
    const options = screen.getByRole('listbox', { name: 'Session statuses' })
    await user.click(within(options).getByRole('option', { name: 'In progress' }))
    await user.click(screen.getByRole('button', { name: 'Active only' }))
    await user.type(screen.getByLabelText('Search sessions'), 'archive')

    await waitFor(() => expect(sessionApi.list).toHaveBeenLastCalledWith({
      projectDir: '/project', query: 'archive', status: 'in_progress', active: true, pageSize: 20, cursor: undefined,
    }))
  })

  it('keeps the detail rendered when the selected session row is clicked again', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/sessions/:sessionId?" element={<><SessionExplorerPage /><Location /></>} />
        </Routes>
      </WorkspaceRefreshProvider></MemoryRouter>,
    )

    expect(await screen.findByRole('heading', { name: 'Request' })).toBeTruthy()
    const sessionList = screen.getByLabelText('Session catalogue')
    const loaded = vi.mocked(sessionApi.get).mock.calls.length

    await user.click(within(sessionList).getByText('Archive export session'))

    expect(screen.getByRole('heading', { name: 'Request' })).toBeTruthy()
    expect(screen.queryByText('Select a session')).toBeNull()
    expect(vi.mocked(sessionApi.get).mock.calls.length).toBe(loaded)
    expect(screen.getByTestId('location').textContent).toBe('/task/sessions/ses-aaaa')
  })

  it('switches the active project from the shared header', async () => {
    const user = userEvent.setup()
    useAppStore.setState({
      projects: [
        { id: 'p1', name: 'Demo', dir: '/project' },
        { id: 'p2', name: 'Other', dir: '/other' },
      ] as never,
      projectsLoaded: true,
    })
    render(
      <MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /></Routes>
      </WorkspaceRefreshProvider></MemoryRouter>,
    )

    await screen.findByText('Archive export session')
    await user.click(screen.getByRole('combobox', { name: 'Project' }))
    await user.click(screen.getByRole('option', { name: 'Other' }))

    expect(useAppStore.getState().activeProjectDir).toBe('/other')
    await waitFor(() => expect(sessionApi.list).toHaveBeenLastCalledWith({
      projectDir: '/other', query: undefined, status: 'all', active: false, pageSize: 20, cursor: undefined,
    }))
  })
  it('combines the project catalogue, session list and open session in one header refresh', async () => {
    const user = userEvent.setup()
    render(<MemoryRouter initialEntries={['/task/sessions']}><WorkspaceRefreshProvider>
      <WorkspaceSelectors /><Routes><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /></Routes>
    </WorkspaceRefreshProvider></MemoryRouter>)
    await screen.findByText('Archive export session')
    await waitFor(() => expect(sessionApi.get).toHaveBeenCalled())
    const beforeList = vi.mocked(sessionApi.list).mock.calls.length
    const beforeDetail = vi.mocked(sessionApi.get).mock.calls.length
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(vi.mocked(sessionApi.list).mock.calls.length).toBeGreaterThan(beforeList))
    expect(vi.mocked(sessionApi.get).mock.calls.length).toBeGreaterThan(beforeDetail)
    expect(useAppStore.getState().loadProjects).toHaveBeenCalledOnce()
    expect(screen.getAllByRole('button', { name: 'Refresh' })).toHaveLength(1)
  })

})
