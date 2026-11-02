import { create } from 'zustand'
import { taskApi, TaskReadError, type TaskContext, type TaskProjectMetadata } from '@/api/task'
import { nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { nativeRecordContexts, NativeRecordReadError, type ProjectReadModule } from '@/api/nativeRecordRead'

export const taskContextKey = nativeSelectionKey
export const taskProjectKey = (context: TaskContext) => JSON.stringify([context.project_dir, context.project_id])
export type WorkspacePhase = 'loading' | 'ready' | 'anonymous' | 'forbidden' | 'error'
interface TaskWorkspace {
 module: ProjectReadModule; contexts: TaskContext[]; projects: TaskProjectMetadata[]; selection: TaskContext | null; phase: WorkspacePhase; error: string; refreshing: boolean
 preferenceIdentity: string | null; preferredSelection: TaskContext | null
 select: (key: string) => void
 refresh: (requested?: TaskContext | null, module?: ProjectReadModule) => Promise<void>
}
let request: AbortController | null = null
let generation = 0
export const useTaskWorkspace = create<TaskWorkspace>((set, get) => ({
 module: 'task', contexts: [], projects: [], selection: null, phase: 'loading', error: '', refreshing: false,
 preferenceIdentity: null, preferredSelection: null,
 select: key => {
  const selection = get().contexts.find(context => taskContextKey(context) === key)
  if (selection) {
   generation++; request?.abort(); request = null
   if (taskContextKey(selection) !== taskContextKey(get().selection) || taskContextKey(selection) !== taskContextKey(get().preferredSelection) || get().refreshing) set({ selection, preferredSelection: selection, refreshing: false })
  }
 },
 refresh: async (requested, module = 'task') => {
  request?.abort()
  const controller = new AbortController(); request = controller
  const current = ++generation
  const sameModule = get().module === module
  set({ module, refreshing: true, error: '', ...(!sameModule ? { contexts: [], projects: [], selection: null, phase: 'loading' as const } : {}) })
  try {
   const result = module === 'task' ? await taskApi.contexts(controller.signal) : await nativeRecordContexts(module, controller.signal)
   if (controller.signal.aborted || current !== generation || get().module !== module) return
   if (result.current && !result.contexts.some(context => taskContextKey(context) === taskContextKey(result.current))) throw new Error('The current working context is unavailable. Retry.')
   const previous = get().selection
   const preferred = requested ?? previous ?? get().preferredSelection
   const selectedKey = taskContextKey(previous)
   const wanted = taskContextKey(preferred)
   const match = result.contexts.find(context => taskContextKey(context) === wanted)
   if (preferred && !match) throw new Error('The selected working context is unavailable. Retry.')
   const next = match ?? result.current
   const selection = previous && taskContextKey(next) === selectedKey ? previous : next
   const admitted = new Set(result.contexts.map(context => context.project_id))
   const projects = (result.projects ?? []).filter(project => admitted.has(project.project_id))
   set({ contexts: result.contexts, projects, selection, preferredSelection: selection, phase: 'ready', error: '' })
  } catch (cause) {
   if (controller.signal.aborted || current !== generation || get().module !== module) return
   const phase = cause instanceof TaskReadError || cause instanceof NativeRecordReadError ? cause.status === 401 ? 'anonymous' : cause.status === 403 ? 'forbidden' : 'error' : 'error'
   set({ contexts: [], projects: [], selection: null, phase, error: cause instanceof Error ? cause.message : 'Unable to load working contexts. Retry.' })
   throw cause
  } finally { if (current === generation) set({ refreshing: false }) }
 },
}))
export function stopTaskWorkspace(module: ProjectReadModule = 'task', identity?: string) {
 generation++; request?.abort(); request = null
 const state = useTaskWorkspace.getState()
 const preferredSelection = identity !== undefined && identity === state.preferenceIdentity ? state.preferredSelection : null
 useTaskWorkspace.setState({ module, preferenceIdentity: identity ?? null, preferredSelection, contexts: [], projects: [], selection: null, phase: 'loading', error: '', refreshing: false })
}

export function invalidateTaskWorkspace(cause: unknown, module: ProjectReadModule = 'task') {
 if (useTaskWorkspace.getState().module !== module) return
 if ((cause instanceof TaskReadError || cause instanceof NativeRecordReadError) && (cause.status === 401 || cause.status === 403)) {
  generation++; request?.abort(); request = null
  useTaskWorkspace.setState({ contexts: [], projects: [], selection: null, phase: cause.status === 401 ? 'anonymous' : 'forbidden', refreshing: false, error: cause.message })
 }
}
