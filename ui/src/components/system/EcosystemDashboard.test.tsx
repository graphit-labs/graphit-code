import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/store/appStore'
import EcosystemDashboard from './EcosystemDashboard'

vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))

vi.mock('@/api/hub', async importOriginal => {
  const original = await importOriginal<typeof import('@/api/hub')>()
  return { ...original, hubApi: { ...original.hubApi, setClusterLabel: vi.fn(), unsetClusterLabel: vi.fn(), unregisterProject: vi.fn() } }
})

describe('Project ecosystem catalogue', () => {
  beforeEach(() => {
    const id = '01ARZ3NDEKTSV4RRFFQ69G5FAV'
    useAppStore.setState({
      projectCatalog: [{
        id, name: 'Platform', description: 'Shared platform', cluster: { team: ['core'] },
        workspace: { id, name: 'Platform', dir: '/workspace/platform', cluster: { team: ['core'] } },
        hub: { id, name: 'Platform', revision: 3, status: 'active', cluster: { team: ['core'] } },
      }],
      projects: [{ id, name: 'Platform', dir: '/workspace/platform', cluster: { team: ['core'] } }],
      projectTargets: [
        { key: `workspace:${id}:/workspace/platform`, origin: 'workspace', id, name: 'Platform', dir: '/workspace/platform' },
        { key: `hub:${id}`, origin: 'hub', id, name: 'Platform' },
      ],
      activeProjectKey: `workspace:${id}:/workspace/platform`, activeProjectId: id,
      activeProjectOrigin: 'workspace', activeProjectDir: '/workspace/platform', projectName: 'Platform',
      projectsLoaded: true, workspaceProjectsError: '', hubProjectsError: '',
    })
  })

  it('shows one identity with both presences and selects its Hub target', async () => {
    const user = userEvent.setup()
    render(<EcosystemDashboard />)
    expect(screen.getAllByRole('row')).toHaveLength(2)
    expect(screen.getAllByText('Workspace').length).toBeGreaterThan(0)
    expect(screen.getByText('Hub · remote')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Use Hub context' }))
    expect(useAppStore.getState().activeProjectOrigin).toBe('hub')
    expect(useAppStore.getState().activeProjectDir).toBe('')
  })

  it('keeps the healthy source usable during a partial catalogue failure', () => {
    useAppStore.setState({ hubProjectsError: 'Hub unavailable' })
    render(<EcosystemDashboard />)
    expect(screen.getByRole('status').textContent).toContain('Partial project catalogue')
    expect(screen.getByRole('status').textContent).toContain('Hub unavailable')
    expect(screen.getByRole('button', { name: 'Retry project catalogue' })).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Use workspace' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('offers retry and names both failures when the catalogue is unavailable', async () => {
    const loadProjects = vi.fn(async () => {})
    const previous = useAppStore.getState().loadProjects
    useAppStore.setState({ projectCatalog: [], workspaceProjectsError: 'Workspace unavailable', hubProjectsError: 'Hub unavailable', loadProjects })
    try {
      render(<EcosystemDashboard />)
      const status = screen.getByRole('status')
      expect(status.textContent).toContain('Project catalogue unavailable')
      expect(status.textContent).toContain('Workspace unavailable')
      expect(status.textContent).toContain('Hub unavailable')
      expect(screen.queryByText('No projects available')).toBeNull()
      await userEvent.setup().click(screen.getByRole('button', { name: 'Retry project catalogue' }))
      expect(loadProjects).toHaveBeenCalledOnce()
    } finally {
      useAppStore.setState({ loadProjects: previous })
    }
  })
})
