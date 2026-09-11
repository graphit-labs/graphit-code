import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { getAppMode, getApiBase } from '@/lib/utils'
import { hubApi, type GlobalProject } from '@/api/hub'

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

  activeContextId: string | null

  projects: GlobalProject[]
  activeProjectDir: string
  projectName: string
  projectsLoaded: boolean
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

  isGlobalLoading: boolean
  activeRequests: number
  incrementLoading: () => void
  decrementLoading: () => void
}

export const useAppStore = create<AppState>()(
  persist(
    (set, get) => ({
      apiBase: getApiBase(),
      appMode: getAppMode(),
      webMode: !!window.__WEB_MODE__,
      webUser: window.__WEB_USER__ ?? '',

      typeFilter: 'all',
      projectFilter: 'all',
      search: '',
      activeAgent: '',
      activeProjectId: '',

      activeContextId: null,

      projects: [],
      activeProjectDir: '',
      projectName: window.__PROJECT_NAME__ ?? '',
      projectsLoaded: false,
      supportedAgents: [],

      isGlobalLoading: false,
      activeRequests: 0,

      setTypeFilter: (typeFilter) => set({ typeFilter }),
      setProjectFilter: (projectFilter) => set({ projectFilter }),
      setSearch: (search) => set({ search }),
      setActiveAgent: (activeAgent) => set({ activeAgent }),
      setActiveProjectId: (activeProjectId) => set({ activeProjectId }),
      setActiveContextId: (activeContextId) => set({ activeContextId }),
      setProjectName: (projectName) => set({ projectName }),

      incrementLoading: () => set((state) => {
        const newCount = state.activeRequests + 1
        if (newCount === 1) document.body.classList.add('is-global-loading')
        return { activeRequests: newCount, isGlobalLoading: newCount > 0 }
      }),
      decrementLoading: () => set((state) => {
        const newCount = Math.max(0, state.activeRequests - 1)
        if (newCount === 0) document.body.classList.remove('is-global-loading')
        return { activeRequests: newCount, isGlobalLoading: newCount > 0 }
      }),

      loadProjects: async () => {
        try {
          const data = await hubApi.getGlobalProjects()
          const projects = data.projects ?? []
          const state = get()
          let activeDir = state.activeProjectDir
          let name = state.projectName
          if (!activeDir && data.current_project_dir) {
            activeDir = data.current_project_dir
            const match = projects.find((p) => p.dir === activeDir)
            if (match) name = match.name
          }
          if (activeDir && projects.length > 0) {
            const match = projects.find((p) => p.dir === activeDir)
            if (!match) {
              activeDir = data.current_project_dir || ''
              const fallback = projects.find((p) => p.dir === activeDir)
              name = fallback?.name || ''
            } else {
              name = match.name
            }
          }
          
          let agent = state.activeAgent
          if (!agent) {
            agent = data.current_agent || 'claude'
          }
          
          set({
            projects,
            activeProjectDir: activeDir,
            projectName: name,
            activeAgent: agent,
            supportedAgents: data.supported_agents ?? [],
            projectsLoaded: true,
          })
        } catch {
          set({ projectsLoaded: true })
        }
      },

      switchProject: (dir: string) => {
        const { projects } = get()
        const match = projects.find((p) => p.dir === dir)
        set({
          activeProjectDir: dir,
          projectName: match?.name || dir.split('/').pop() || '',
          activeProjectId: match?.id || '',
        })
      },
    }),
    {
      name: 'graphit-app-state',
      partialize: (s) => ({
        typeFilter: s.typeFilter,
        projectFilter: s.projectFilter,
        activeAgent: s.activeAgent,
        activeContextId: s.activeContextId,
        activeProjectDir: s.activeProjectDir,
      }),
    },
  ),
)
