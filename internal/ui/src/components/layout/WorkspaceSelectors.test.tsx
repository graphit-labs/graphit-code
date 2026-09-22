import "@/test/contextControls"
import { WorkspaceRefreshProvider } from "./WorkspaceRefresh"
import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Link, MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { AppShell } from './AppShell'
import { WorkspaceSelectors } from './WorkspaceSelectors'
import WorkspacePage from '../system/WorkspacePage'

vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
const originalLoadProjects = useAppStore.getState().loadProjects
beforeEach(() => {
  useAppStore.setState({
    projectsError: '',
    projects: [{ id: 'p1', name: 'Platform', dir: '/platform' }, { id: 'p2', name: 'Service', dir: '/service' }] as never,
    projectTargets: [], projectCatalog: [], activeProjectKey: '', activeProjectOrigin: '',
    activeProjectDir: '/platform', projectName: 'Platform', activeProjectId: 'p1',
    supportedAgents: ['codex', 'claude-code'], activeAgent: 'codex', projectsLoaded: true,
    loadProjects: originalLoadProjects,
  })
})

describe('Global working context', () => {
  it('owns selectors once in the shared header and preserves both selections across routes', async () => {
    const user = userEvent.setup()
    render(<MemoryRouter initialEntries={['/workspace']}><AppShell>
      <Routes>
        <Route path="/workspace" element={<WorkspacePage />} />
        <Route path="/task/explorer" element={<Link to="/workspace">Return to workspace</Link>} />
      </Routes>
    </AppShell></MemoryRouter>)
    const header = screen.getByRole('banner', { name: 'Workspace header' })
    expect(screen.getAllByRole('combobox', { name: 'Project' })).toHaveLength(1)
    expect(screen.getAllByRole('combobox', { name: 'Agent' })).toHaveLength(1)
    expect(screen.queryByText('Switch working context')).toBeNull()
    const project = within(header).getByRole('combobox', { name: 'Project' }) as HTMLSelectElement
    const agent = within(header).getByRole('combobox', { name: 'Agent' }) as HTMLSelectElement
    await user.click(project)
    await user.click(screen.getByRole('option', { name: 'Service' }))
    expect(useAppStore.getState().activeProjectId).toBe('p2')
    expect(useAppStore.getState().activeAgent).toBe('codex')
    await user.click(agent)
    await user.click(screen.getByRole('option', { name: 'Claude Code' }))
    expect(useAppStore.getState().activeProjectDir).toBe('/service')
    await user.click(screen.getByRole('tab', { name: 'Continue work' }))
    await user.click(screen.getByRole('link', { name: /Review a delivery/ }))
    expect(project.textContent).toContain('Service')
    expect(agent.textContent).toContain('Claude Code')
    await user.click(screen.getByRole('link', { name: 'Return to workspace' }))
    expect(screen.getAllByRole('combobox', { name: 'Project' })).toHaveLength(1)
    const persisted = JSON.parse(localStorage.getItem('graphit-app-state')!).state
    expect(persisted.activeProjectDir).toBe('/service')
    expect(persisted.activeAgent).toBe('claude-code')
  })

  it('keeps loading and empty controls visible and lets users refresh their catalogue', async () => {
    const user = userEvent.setup()
    const loadProjects = vi.fn(async () => { useAppStore.setState({ projectsLoaded: true }) })
    useAppStore.setState({ projects: [], activeProjectDir: '', supportedAgents: [], activeAgent: '', projectsLoaded: false, loadProjects })
    render(<MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors /></WorkspaceRefreshProvider></MemoryRouter>)
    const project = screen.getByRole('combobox', { name: 'Project' }) as HTMLSelectElement
    expect(project.disabled).toBe(true)
    expect(project.textContent).toContain('Loading projects')
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    expect(loadProjects).toHaveBeenCalledOnce()
    expect(project.textContent).toContain('No projects available')
    expect(screen.getByRole('combobox', { name: 'Agent' }).textContent).toContain('No agents available')
  })

  it('keeps a single project visible and supports keyboard focus without opening navigation', async () => {
    const user = userEvent.setup()
    useAppStore.setState({ projects: [useAppStore.getState().projects[0]] })
    render(<MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors /></WorkspaceRefreshProvider></MemoryRouter>)
    const project = screen.getByRole('combobox', { name: 'Project' }) as HTMLSelectElement
    await user.tab()
    expect(document.activeElement).toBe(project)
    expect(project.textContent).toContain('Platform')
    await user.tab()
    expect(document.activeElement).toBe(screen.getByRole('combobox', { name: 'Agent' }))
  })

  it('disambiguates duplicate project names and never displays an unrelated first option for a missing selection', async () => {
    const user = userEvent.setup()
    useAppStore.setState({ projects: [
      { id: 'p1', name: 'Platform', dir: '/platform' }, { id: 'p2', name: 'Platform', dir: '/other' },
    ] as never })
    render(<MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors /></WorkspaceRefreshProvider></MemoryRouter>)
    await user.click(screen.getByRole('combobox', { name: 'Project' }))
    expect(screen.getByRole('listbox').textContent).toContain('/platform')
    expect(screen.getByRole('listbox').textContent).toContain('/other')
    await user.keyboard('{Escape}')
    expect(document.activeElement).toBe(screen.getByRole('combobox', { name: 'Project' }))
    act(() => useAppStore.setState({ activeProjectDir: '/missing', projectName: 'Missing', activeAgent: 'legacy' }))
    const project = screen.getByRole('combobox', { name: 'Project' }) as HTMLSelectElement
    expect(project.textContent).toContain('Missing (unavailable)')
    expect(screen.getByRole('combobox', { name: 'Agent' }).textContent).toContain('Legacy (unavailable)')
  })

  it('keeps a remote Hub project visibly selected in the global control', async () => {
    const user = userEvent.setup()
    const remoteId = '01ARZ3NDEKTSV4RRFFQ69G5FAV'
    useAppStore.setState({
      projectTargets: [
        { key: 'workspace:p1:/platform', origin: 'workspace', id: 'p1', name: 'Platform', dir: '/platform' },
        { key: `hub:${remoteId}`, origin: 'hub', id: remoteId, name: 'Remote platform' },
      ],
      activeProjectKey: 'workspace:p1:/platform', activeProjectOrigin: 'workspace', activeProjectDir: '/platform',
    })
    render(<MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors /></WorkspaceRefreshProvider></MemoryRouter>)
    await user.click(screen.getByRole('combobox', { name: 'Project' }))
    expect(screen.getByText('Hub · remote')).toBeTruthy()
    await user.click(screen.getByRole('option', { name: 'Remote platform' }))
    expect(useAppStore.getState().activeProjectKey).toBe(`hub:${remoteId}`)
    expect(useAppStore.getState().activeProjectDir).toBe('')
    expect(screen.getByRole('combobox', { name: 'Project' }).textContent).toContain('Remote platform · Hub')
  })
})
