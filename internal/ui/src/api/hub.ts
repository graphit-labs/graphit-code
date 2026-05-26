import { api } from './client'

export interface RegistryEntry {
  id: string
  name: string
  description?: string
  type: string
  latest?: string
  versions?: string[]
  author?: { username: string; avatar_url?: string }
  project_id?: string
  tags?: string[]
}

export interface InstalledArtifact {
  local_id: string
  remote_id: string
  type: string
  version?: string
  alias?: string
  has_update?: boolean
  path?: string
  published?: boolean
  origin?: string
  registry_name?: string
  registry_description?: string
  registry_tags?: string[]
  registry_author?: string
  registry_version?: string
  registry_dependencies?: Array<{ type: string; id: string; version: string }>
}

export interface RegistryResponse {
  entries: RegistryEntry[]
  installed: InstalledArtifact[]
  project_lock: Record<string, unknown>
  active_project: string
  active_project_id: string
  active_project_name: string
  project_path: string
  project_cluster?: Record<string, string>
  ide: string
  projects?: Array<{ name: string; remote_id: string }>
}

export interface Project {
  name: string
  remote_id: string
}

export interface GlobalProject {
  id: string
  name: string
  dir: string
}

export interface GlobalProjectsResponse {
  projects: GlobalProject[]
  current_project_dir: string
  current_ide: string
  supported_ides: string[]
}

export const hubApi = {
  getGlobalProjects: () => api.get<GlobalProjectsResponse>('/api/global-projects'),
  
  getRegistry: (projectDir?: string, ide?: string) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (ide) params.set('ide', ide)
    const qs = params.toString()
    return api.get<RegistryResponse>(`/api/registry${qs ? `?${qs}` : ''}`)
  },
  
  getProjectArtifacts: (projectDir?: string, ide?: string) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (ide) params.set('ide', ide)
    const qs = params.toString()
    return api.get<{
      project_artifacts: InstalledArtifact[]
      imported_artifacts: InstalledArtifact[]
      project_name: string
      project_path: string
      project_cluster?: Record<string, string>
      ide: string
    }>(`/api/project-artifacts${qs ? `?${qs}` : ''}`)
  },
  
  getGitAuthor: () => api.get<{ author: string }>('/api/git-author'),
  getProjects: () => api.get<{ projects: Project[] }>('/api/projects'),

  install: (id: string, alias: string | null, ide: string, type: string, projectDir?: string, version?: string) =>
    api.post<{ success: boolean; error?: string }>('/api/install', {
      id,
      alias: alias || undefined,
      ide,
      type,
      project_dir: projectDir || undefined,
      version: version || undefined,
    }),

  uninstall: (id: string, localId: string, ide: string, type: string, projectDir?: string) =>
    api.post<{ success: boolean; error?: string }>('/api/uninstall', {
      id,
      local_id: localId || undefined,
      ide,
      type,
      project_dir: projectDir || undefined,
    }),

  updateAll: (ide: string, projectDir?: string) =>
    api.post<{ success: boolean; errors: string[] }>('/api/update_all', {
      ide,
      project_dir: projectDir || undefined,
    }),

  updateOne: (id: string, type: string, ide: string, projectDir?: string) =>
    api.post<{ success: boolean; error?: string }>('/api/update_one', {
      id,
      type,
      ide,
      project_dir: projectDir || undefined,
    }),

  submit: (payload: Record<string, unknown>) =>
    api.post<{ success: boolean; error?: string }>('/api/submit', payload),

  unpublish: (id: string, type: string, projectDir?: string) =>
    api.post<{ success: boolean }>('/api/unpublish', { id, type, project_dir: projectDir }),

  unlinkLocal: (id: string, type: string, ide: string, projectDir: string) =>
    api.post('/api/unlink', { id, type, ide, project_dir: projectDir }),

  upload: (formData: FormData) => {
    const base = window.__API_BASE__ ?? ''
    const fullBase = base.endsWith('/api') ? base : `${base}/api`
    const headers: Record<string, string> = {}
    const token = document.cookie.match(/(^| )graphit_id_token=([^;]+)/)?.[2]
    if (token && window.__WEB_MODE__) {
      headers['Authorization'] = `Bearer ${token}`
    }
    return fetch(`${fullBase}/upload`, {
      method: 'POST',
      headers,
      body: formData,
    }).then((r) => r.json())
  },
}
