import { describe, test, expect, vi, beforeEach } from 'vitest'
import { useAppStore } from './appStore'
import { hubApi } from '@/api/hub'

vi.mock('@/api/hub', () => ({
  hubApi: {
    getGlobalProjects: vi.fn(),
  },
}))

describe('appStore', () => {
  beforeEach(() => {
    // Reset state before each test
    const store = useAppStore.getState()
    store.setTypeFilter('all')
    store.setProjectFilter('all')
    store.setSearch('')
    store.setActiveIde('')
    store.setActiveProjectId('')
    store.setActiveContextId(null)
    store.setProjectName('')
    
    // Reset loading state
    while (useAppStore.getState().activeRequests > 0) {
      useAppStore.getState().decrementLoading()
    }
  })

  test('basic setters and filters', () => {
    const store = useAppStore.getState()

    store.setTypeFilter('skill')
    expect(useAppStore.getState().typeFilter).toBe('skill')

    store.setProjectFilter('my-proj')
    expect(useAppStore.getState().projectFilter).toBe('my-proj')

    store.setSearch('hello')
    expect(useAppStore.getState().search).toBe('hello')

    store.setActiveIde('cursor')
    expect(useAppStore.getState().activeIde).toBe('cursor')

    store.setActiveProjectId('proj-123')
    expect(useAppStore.getState().activeProjectId).toBe('proj-123')

    store.setActiveContextId('ctx-abc')
    expect(useAppStore.getState().activeContextId).toBe('ctx-abc')

    store.setProjectName('My Project')
    expect(useAppStore.getState().projectName).toBe('My Project')
  })

  test('loading request counters and document classes', () => {
    const store = useAppStore.getState()
    expect(store.isGlobalLoading).toBe(false)
    expect(store.activeRequests).toBe(0)
    expect(document.body.classList.contains('is-global-loading')).toBe(false)

    // First request
    store.incrementLoading()
    expect(useAppStore.getState().isGlobalLoading).toBe(true)
    expect(useAppStore.getState().activeRequests).toBe(1)
    expect(document.body.classList.contains('is-global-loading')).toBe(true)

    // Second request
    store.incrementLoading()
    expect(useAppStore.getState().activeRequests).toBe(2)
    expect(document.body.classList.contains('is-global-loading')).toBe(true)

    // Decrement once
    store.decrementLoading()
    expect(useAppStore.getState().isGlobalLoading).toBe(true)
    expect(useAppStore.getState().activeRequests).toBe(1)
    expect(document.body.classList.contains('is-global-loading')).toBe(true)

    // Decrement twice (back to 0)
    store.decrementLoading()
    expect(useAppStore.getState().isGlobalLoading).toBe(false)
    expect(useAppStore.getState().activeRequests).toBe(0)
    expect(document.body.classList.contains('is-global-loading')).toBe(false)

    // Additional decrement should not go below 0
    store.decrementLoading()
    expect(useAppStore.getState().activeRequests).toBe(0)
  })

  test('loadProjects success - initial load without active project', async () => {
    const mockProjects = [
      { id: '1', name: 'Project One', dir: '/dir/one' },
      { id: '2', name: 'Project Two', dir: '/dir/two' },
    ]

    vi.mocked(hubApi.getGlobalProjects).mockResolvedValueOnce({
      projects: mockProjects,
      current_project_dir: '/dir/two',
      current_ide: 'cursor',
      supported_ides: ['cursor', 'antigravity'],
    })

    const store = useAppStore.getState()
    await store.loadProjects()

    const updated = useAppStore.getState()
    expect(updated.projectsLoaded).toBe(true)
    expect(updated.projects).toEqual(mockProjects)
    expect(updated.activeProjectDir).toBe('/dir/two')
    expect(updated.projectName).toBe('Project Two')
    expect(updated.activeIde).toBe('cursor')
    expect(updated.supportedIdes).toEqual(['cursor', 'antigravity'])
  })

  test('loadProjects success - with active project matching and non-matching fallback', async () => {
    const mockProjects = [
      { id: '1', name: 'Project One', dir: '/dir/one' },
    ]

    // Setup store with a project dir that exists
    useAppStore.setState({ activeProjectDir: '/dir/one' })

    vi.mocked(hubApi.getGlobalProjects).mockResolvedValueOnce({
      projects: mockProjects,
      current_project_dir: '/dir/one',
      current_ide: '',
      supported_ides: [],
    })

    await useAppStore.getState().loadProjects()
    expect(useAppStore.getState().projectName).toBe('Project One')
    expect(useAppStore.getState().activeIde).toBe('claude') // Default fallback

    // Now test with dir that doesn't match projects, should fallback to current_project_dir
    useAppStore.setState({ activeProjectDir: '/dir/not-exist' })
    vi.mocked(hubApi.getGlobalProjects).mockResolvedValueOnce({
      projects: mockProjects,
      current_project_dir: '/dir/one',
      current_ide: 'antigravity',
      supported_ides: ['antigravity'],
    })

    await useAppStore.getState().loadProjects()
    expect(useAppStore.getState().activeProjectDir).toBe('/dir/one')
    expect(useAppStore.getState().projectName).toBe('Project One')
  })

  test('loadProjects failure', async () => {
    vi.mocked(hubApi.getGlobalProjects).mockRejectedValueOnce(new Error('Network error'))

    const store = useAppStore.getState()
    await store.loadProjects()

    expect(useAppStore.getState().projectsLoaded).toBe(true)
  })

  test('switchProject action', () => {
    const mockProjects = [
      { id: '1', name: 'Project One', dir: '/dir/one' },
      { id: '2', name: 'Project Two', dir: '/dir/two' },
    ]
    useAppStore.setState({ projects: mockProjects })

    const store = useAppStore.getState()
    
    // Switch to Project One
    store.switchProject('/dir/one')
    let state = useAppStore.getState()
    expect(state.activeProjectDir).toBe('/dir/one')
    expect(state.projectName).toBe('Project One')
    expect(state.activeProjectId).toBe('1')

    // Switch to non-matching directory
    store.switchProject('/dir/three')
    state = useAppStore.getState()
    expect(state.activeProjectDir).toBe('/dir/three')
    expect(state.projectName).toBe('three')
    expect(state.activeProjectId).toBe('')
  })
})
