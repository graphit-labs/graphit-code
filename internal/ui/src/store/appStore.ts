import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { getAppMode, getApiBase } from '@/lib/utils'
import { hubApi, type GlobalProject, type HubProject } from '@/api/hub'

export type ProjectOrigin = 'workspace' | 'hub'

export interface ProjectTarget {
  key: string
  origin: ProjectOrigin
  id: string
  name: string
  description?: string
  dir?: string
  cluster?: Record<string, string[]>
  revision?: number
  availability?: 'available' | 'stale'
}

export interface LogicalProject {
  id: string
  name: string
  description?: string
  cluster: Record<string, string[]>
  workspace?: GlobalProject
  hub?: HubProject
}

export function workspaceTarget(project: GlobalProject): ProjectTarget {
  return {
    key: `workspace:${project.id}:${project.dir}`,
    origin: 'workspace', id: project.id, name: project.name, description: project.description,
    dir: project.dir, cluster: project.cluster, availability: 'available',
  }
}

export function hubTarget(project: HubProject): ProjectTarget {
  return {
    key: `hub:${project.id}`,
    origin: 'hub', id: project.id, name: project.name, description: project.description,
    cluster: project.cluster, revision: project.revision, availability: 'available',
  }
}

export function mergeProjectCatalog(workspace: GlobalProject[], hub: HubProject[]): LogicalProject[] {
  const merged = new Map<string, LogicalProject>()
  for (const project of workspace) {
    merged.set(project.id, {
      id: project.id, name: project.name, description: project.description,
      cluster: project.cluster ?? {}, workspace: project,
    })
  }
  for (const project of hub) {
    const current = merged.get(project.id)
    merged.set(project.id, {
      id: project.id,
      name: current?.name || project.name,
      description: current?.description || project.description,
      cluster: Object.keys(current?.cluster ?? {}).length ? current!.cluster : (project.cluster ?? {}),
      workspace: current?.workspace,
      hub: project,
    })
  }
  return [...merged.values()].sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id))
}

interface AppState {
  apiBase: string
  appMode: 'hub' | 'ast' | 'unified'
  webMode: boolean
  webUser: string
  typeFilter: string
  projectFilter: string
  search: string
  activeAgent: string
  activeProjectId: string
  activeProjectKey: string
  activeProjectOrigin: ProjectOrigin | ''
  activeContextId: string | null
  projects: GlobalProject[]
  projectTargets: ProjectTarget[]
  projectCatalog: LogicalProject[]
  activeProjectDir: string
  projectName: string
  projectsLoaded: boolean
  projectsError: string
  workspaceProjectsError: string
  hubProjectsError: string
  supportedAgents: string[]
  setTypeFilter: (v: string) => void
  setProjectFilter: (v: string) => void
  setSearch: (v: string) => void
  setActiveAgent: (v: string) => void
  setActiveProjectId: (v: string) => void
  setActiveContextId: (v: string | null) => void
  setProjectName: (v: string) => void
  loadProjects: () => Promise<void>
  switchProject: (dir: string) => void
  switchProjectTarget: (key: string) => void
  isGlobalLoading: boolean
  activeRequests: number
  incrementLoading: () => void
  decrementLoading: () => void
}

export const useAppStore = create<AppState>()(
  persist(
    (set, get) => ({
      apiBase: getApiBase(), appMode: getAppMode(), webMode: !!window.__WEB_MODE__, webUser: window.__WEB_USER__ ?? '',
      typeFilter: 'all', projectFilter: 'all', search: '', activeAgent: '', activeProjectId: '',
      activeProjectKey: '', activeProjectOrigin: '', activeContextId: null,
      projects: [], projectTargets: [], projectCatalog: [], activeProjectDir: '',
      projectName: window.__PROJECT_NAME__ ?? '', projectsLoaded: false, projectsError: '',
      workspaceProjectsError: '', hubProjectsError: '', supportedAgents: [],
      isGlobalLoading: false, activeRequests: 0,

      setTypeFilter: (typeFilter) => set({ typeFilter }),
      setProjectFilter: (projectFilter) => set({ projectFilter }),
      setSearch: (search) => set({ search }),
      setActiveAgent: (activeAgent) => set({ activeAgent }),
      setActiveProjectId: (activeProjectId) => set({ activeProjectId }),
      setActiveContextId: (activeContextId) => set({ activeContextId }),
      setProjectName: (projectName) => set({ projectName }),

      incrementLoading: () => set((state) => {
        const activeRequests = state.activeRequests + 1
        if (activeRequests === 1) document.body.classList.add('is-global-loading')
        return { activeRequests, isGlobalLoading: true }
      }),
      decrementLoading: () => set((state) => {
        const activeRequests = Math.max(0, state.activeRequests - 1)
        if (activeRequests === 0) document.body.classList.remove('is-global-loading')
        return { activeRequests, isGlobalLoading: activeRequests > 0 }
      }),

      loadProjects: async () => {
        set({ projectsError: '', workspaceProjectsError: '', hubProjectsError: '' })
        const [localResult, catalogResult] = await Promise.allSettled([
          hubApi.getGlobalProjects(), hubApi.getProjectCatalog(),
        ])
        const state = get()
        const local = localResult.status === 'fulfilled' ? localResult.value : null
        const catalog = catalogResult.status === 'fulfilled' ? catalogResult.value : null
        const workspace = local?.projects ?? catalog?.workspace_projects ?? []
        const remote = catalog?.hub_projects ?? []
        const workspaceError = catalog?.workspace_error || (localResult.status === 'rejected' ? 'Workspace projects are unavailable.' : '')
        const hubError = catalog?.hub_error || (catalogResult.status === 'rejected' ? 'Hub projects are unavailable.' : '')
        const targets = [...workspace.map(workspaceTarget), ...remote.map(hubTarget)]

        let selected = targets.find((target) => target.key === state.activeProjectKey)
        if (!selected && state.activeProjectDir) selected = targets.find((target) => target.origin === 'workspace' && target.dir === state.activeProjectDir)
        if (!selected && local?.current_project_dir) selected = targets.find((target) => target.origin === 'workspace' && target.dir === local.current_project_dir)
        if (!selected && state.activeProjectKey.startsWith('hub:') && state.activeProjectId) {
          selected = { key: state.activeProjectKey, origin: 'hub', id: state.activeProjectId, name: state.projectName || state.activeProjectId, availability: 'stale' }
          targets.push(selected)
        }

        let agent = state.activeAgent
        if (!agent) agent = local?.current_agent || 'claude'
        set({
          projects: workspace, projectTargets: targets, projectCatalog: mergeProjectCatalog(workspace, remote),
          activeProjectKey: selected?.key ?? '', activeProjectOrigin: selected?.origin ?? '',
          activeProjectDir: selected?.origin === 'workspace' ? selected.dir ?? '' : '',
          activeProjectId: selected?.id ?? '', projectName: selected?.name ?? '', activeAgent: agent,
          supportedAgents: local?.supported_agents ?? state.supportedAgents, projectsLoaded: true,
          workspaceProjectsError: workspaceError, hubProjectsError: hubError,
          projectsError: workspaceError && hubError ? 'Workspace and Hub project sources are unavailable.' : '',
        })
      },

      switchProjectTarget: (key: string) => {
        const target = get().projectTargets.find((candidate) => candidate.key === key)
        if (!target) return
        set({
          activeProjectKey: target.key, activeProjectOrigin: target.origin,
          activeProjectDir: target.origin === 'workspace' ? target.dir ?? '' : '',
          activeProjectId: target.id, projectName: target.name, activeContextId: null,
        })
      },

      switchProject: (dir: string) => {
        const project = get().projects.find((candidate) => candidate.dir === dir)
        const target = get().projectTargets.find((candidate) => candidate.origin === 'workspace' && candidate.dir === dir)
          ?? (project ? workspaceTarget(project) : undefined)
        if (target) {
          const known = get().projectTargets.some((candidate) => candidate.key === target.key)
          if (!known) set((state) => ({ projectTargets: [...state.projectTargets, target] }))
          get().switchProjectTarget(target.key)
          return
        }
        set({ activeProjectKey: '', activeProjectOrigin: '', activeProjectDir: dir, activeProjectId: '', projectName: dir.split('/').pop() || '' })
      },
    }),
    {
      name: 'graphit-app-state',
      partialize: (s) => ({
        typeFilter: s.typeFilter, projectFilter: s.projectFilter, activeAgent: s.activeAgent,
        activeContextId: s.activeContextId, activeProjectKey: s.activeProjectKey,
        activeProjectOrigin: s.activeProjectOrigin, activeProjectDir: s.activeProjectDir,
        activeProjectId: s.activeProjectId, projectName: s.projectName,
      }),
    },
  ),
)
