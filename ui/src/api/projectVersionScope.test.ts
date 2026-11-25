import { beforeEach, describe, expect, it } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { scopeToProjectVersion } from './client'

const local = { activeProjectKey: 'workspace:p1:/platform', activeProjectOrigin: 'workspace' as const, activeProjectDir: '/platform', activeProjectId: 'p1' }

describe('scopeToProjectVersion', () => {
  beforeEach(() => useAppStore.setState({ ...local, activeProjectVersion: 'feature/x' }))

  it('adds the selected version to a request that names the active checkout', () => {
    const url = scopeToProjectVersion('/api/tasks?project_dir=%2Fplatform&page_size=20')
    expect(new URL(url, 'http://x').searchParams.get('project_version')).toBe('feature/x')
    expect(new URL(url, 'http://x').searchParams.get('page_size')).toBe('20')
  })

  it('copies a JSON body scope to the query so the server selects the version first', () => {
    const params = new URL(scopeToProjectVersion('/api/query', JSON.stringify({ cypher: 'RETURN 1', project_dir: '/platform' })), 'http://x').searchParams
    expect(params.get('project_dir')).toBe('/platform')
    expect(params.get('project_version')).toBe('feature/x')
  })

  it('reads a multipart upload scope', () => {
    const form = new FormData()
    form.set('project_dir', '/platform')
    expect(new URL(scopeToProjectVersion('/api/upload', form), 'http://x').searchParams.get('project_version')).toBe('feature/x')
  })

  it('leaves other projects, unscoped requests and explicit versions alone', () => {
    expect(scopeToProjectVersion('/api/tasks?project_dir=%2Fservice')).toBe('/api/tasks?project_dir=%2Fservice')
    expect(scopeToProjectVersion('/api/projects')).toBe('/api/projects')
    expect(scopeToProjectVersion('/api/tasks?project_dir=%2Fplatform&project_version=main')).toBe('/api/tasks?project_dir=%2Fplatform&project_version=main')
    useAppStore.setState({ activeProjectVersion: '' })
    expect(scopeToProjectVersion('/api/tasks?project_dir=%2Fplatform')).toBe('/api/tasks?project_dir=%2Fplatform')
  })

  it('scopes a Hub project by its ID', () => {
    useAppStore.setState({ activeProjectKey: 'hub:p9', activeProjectOrigin: 'hub', activeProjectDir: '', activeProjectId: 'p9', activeProjectVersion: 'feature/r' })
    expect(new URL(scopeToProjectVersion('/api/tasks?project_id=p9'), 'http://x').searchParams.get('project_version')).toBe('feature/r')
    expect(scopeToProjectVersion('/api/tasks?project_id=other')).toBe('/api/tasks?project_id=other')
  })
})

describe('project version selection', () => {
  it('is remembered per project and restored when switching back', () => {
    useAppStore.setState({
      ...local, activeProjectVersion: '', projectVersions: {},
      projectTargets: [
        { key: 'workspace:p1:/platform', origin: 'workspace', id: 'p1', name: 'Platform', dir: '/platform' },
        { key: 'workspace:p2:/service', origin: 'workspace', id: 'p2', name: 'Service', dir: '/service' },
      ],
    })
    useAppStore.getState().setProjectVersion('feature/x')
    useAppStore.getState().switchProjectTarget('workspace:p2:/service')
    expect(useAppStore.getState().activeProjectVersion).toBe('')
    useAppStore.getState().switchProjectTarget('workspace:p1:/platform')
    expect(useAppStore.getState().activeProjectVersion).toBe('feature/x')
    useAppStore.getState().setProjectVersion('')
    expect(useAppStore.getState().projectVersions['workspace:p1:/platform']).toBeUndefined()
  })
})
