import { api } from './client'

/** One version of a project: a Git branch, or the default version of a Hub project (empty name). */
export interface ProjectVersion {
  name: string
  default: boolean
  current: boolean
  checkouts?: string[]
  in_git: boolean
  /** Where the version has data or is checked out: ast, knowledge, task, memory, dream, checkout. */
  sources: string[]
}

export interface ProjectVersionsResponse {
  remote: boolean
  git: boolean
  default: string
  current: string
  versions: ProjectVersion[]
  warnings?: string[]
}

export const projectVersionsApi = {
  list: (scope: { projectDir?: string; projectId?: string }) => {
    const params = new URLSearchParams()
    if (scope.projectDir) params.set('project_dir', scope.projectDir)
    if (scope.projectId) params.set('project_id', scope.projectId)
    return api.get<ProjectVersionsResponse>(`/api/project/versions?${params}`)
  },
}
