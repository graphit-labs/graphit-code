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
    expect(screen.getByRole('status').textContent).toContain('Workspace is available')
    expect((screen.getByRole('button', { name: 'Use workspace' }) as HTMLButtonElement).disabled).toBe(false)
  })
})
