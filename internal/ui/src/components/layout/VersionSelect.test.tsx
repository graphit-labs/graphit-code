import "@/test/contextControls"
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { projectVersionsApi } from '@/api/projectVersions'
import { VersionSelect } from './VersionSelect'

vi.mock('@/api/projectVersions', () => ({ projectVersionsApi: { list: vi.fn() } }))
const list = vi.mocked(projectVersionsApi.list)

function Where() {
  return <span data-testid="where">{useLocation().pathname}</span>
}

function renderSelect(path = '/task/explorer') {
  return render(<MemoryRouter initialEntries={[path]}>
    <VersionSelect refreshTick={0} />
    <Routes><Route path="*" element={<Where />} /></Routes>
  </MemoryRouter>)
}

beforeEach(() => {
  list.mockReset()
  useAppStore.setState({
    activeProjectKey: 'workspace:p1:/platform', activeProjectOrigin: 'workspace', activeProjectDir: '/platform',
    activeProjectId: 'p1', activeProjectVersion: '', projectVersions: {}, projectsLoaded: true,
  })
})

describe('Version selector', () => {
  it('lists checked-out versions first and switches the active project version', async () => {
    list.mockResolvedValue({
      remote: false, git: true, default: 'main', current: 'main', versions: [
        { name: 'main', default: true, current: true, checkouts: ['/platform'], in_git: true, sources: ['checkout'] },
        { name: 'feature/x', default: false, current: false, checkouts: ['/wt/x'], in_git: true, sources: ['checkout', 'task'] },
        { name: 'old/gone', default: false, current: false, in_git: false, sources: ['ast'] },
      ],
    })
    const user = userEvent.setup()
    renderSelect()
    const trigger = screen.getByRole('combobox', { name: 'Version' })
    await waitFor(() => expect(trigger.textContent).toContain('main'))
    expect(list).toHaveBeenCalledWith({ projectDir: '/platform' })
    await user.click(trigger)
    expect(screen.getByText('Checked out')).toBeTruthy()
    expect(screen.getByText('Other versions with data')).toBeTruthy()
    expect(screen.getByText('Has code index, branch no longer in Git.')).toBeTruthy()
    await user.click(screen.getByRole('option', { name: /feature\/x/ }))
    expect(useAppStore.getState().activeProjectVersion).toBe('feature/x')
    expect(useAppStore.getState().projectVersions['workspace:p1:/platform']).toBe('feature/x')
    await user.click(trigger)
    await user.click(screen.getByRole('option', { name: /^main/ }))
    expect(useAppStore.getState().activeProjectVersion).toBe('')
  })

  it('reports a listing failure without losing the selection', async () => {
    list.mockRejectedValue(new Error('S3 is unreachable'))
    useAppStore.setState({ activeProjectVersion: 'feature/x' })
    renderSelect()
    const trigger = screen.getByRole('combobox', { name: 'Version' })
    await waitFor(() => expect(trigger.getAttribute('title')).toContain('S3 is unreachable'))
    expect(trigger.textContent).toContain('feature/x')
  })

  it('offers a Hub project its default version and shared branches, and re-picks published artifacts', async () => {
    list.mockResolvedValue({
      remote: true, git: false, default: '', current: '', versions: [
        { name: '', default: true, current: false, in_git: false, sources: [] },
        { name: 'feature/r', default: false, current: false, in_git: false, sources: ['memory', 'task'] },
      ],
    })
    useAppStore.setState({ activeProjectKey: 'hub:p9', activeProjectOrigin: 'hub', activeProjectDir: '', activeProjectId: 'p9' })
    const user = userEvent.setup()
    renderSelect('/ast/explorer/api-ast%401.0.0')
    const trigger = screen.getByRole('combobox', { name: 'Version' })
    await waitFor(() => expect(trigger.textContent).toContain('Default version'))
    expect(list).toHaveBeenCalledWith({ projectId: 'p9' })
    await user.click(trigger)
    await user.click(screen.getByRole('option', { name: /feature\/r/ }))
    expect(useAppStore.getState().activeProjectVersion).toBe('feature/r')
    expect(screen.getByTestId('where').textContent).toBe('/ast/explorer')
  })

  it('waits for a project before listing versions', () => {
    useAppStore.setState({ activeProjectKey: '', activeProjectOrigin: '', activeProjectDir: '', activeProjectId: '' })
    renderSelect()
    const trigger = screen.getByRole('combobox', { name: 'Version' })
    expect(trigger.textContent).toContain('Select a project')
    expect(trigger.hasAttribute('disabled') || trigger.getAttribute('data-disabled') !== null).toBe(true)
    expect(list).not.toHaveBeenCalled()
  })
})
