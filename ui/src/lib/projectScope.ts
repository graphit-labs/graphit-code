import type { ProjectOrigin } from '@/store/appStore'

export interface ProjectScopeState {
  activeProjectKey: string
  activeProjectOrigin: ProjectOrigin | ''
  activeProjectDir: string
  activeProjectId: string
}

export interface ProjectRequestScope {
  key: string
  origin: ProjectOrigin | ''
  projectDir?: string
  projectId?: string
  remote: boolean
}

export function projectRequestScope(state: ProjectScopeState): ProjectRequestScope {
  const remote = state.activeProjectOrigin === 'hub'
  return {
    key: state.activeProjectKey || (remote ? `hub:${state.activeProjectId}` : `workspace:${state.activeProjectDir}`),
    origin: state.activeProjectOrigin,
    projectDir: remote ? undefined : state.activeProjectDir || undefined,
    projectId: remote ? state.activeProjectId || undefined : undefined,
    remote,
  }
}

export function appendProjectScope(params: URLSearchParams, scope: Pick<ProjectRequestScope, 'projectDir' | 'projectId'>) {
  if (scope.projectDir) params.set('project_dir', scope.projectDir)
  if (scope.projectId) params.set('project_id', scope.projectId)
  return params
}
