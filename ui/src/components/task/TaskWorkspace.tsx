import { useEffect, useMemo, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { RefreshCw } from 'lucide-react'
import { requestedRecordSelection } from '@/api/nativeRecordNavigation'
import { stringifyNativeJson } from '@/api/nativeJson'
import { nativeBindings } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import { bindingLabel } from '@/components/hub/hubContext'
import type { ProjectReadModule } from '@/api/nativeRecordRead'
import { ContextSelect } from '@/components/layout/ContextSelect'
import { useWorkspaceRefresh } from '@/components/layout/WorkspaceRefresh'
import { WorkEmpty, WorkNotice } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { stopTaskWorkspace, taskContextKey, taskProjectKey, useTaskWorkspace } from './taskWorkspaceState'

export function TaskWorkspaceBootstrap({ children, module = 'task' }: { children: React.ReactNode; module?: ProjectReadModule }) {
 const { search } = useLocation()
 const workspace = useTaskWorkspace()
 const identity = useAppStore(state => JSON.stringify([state.apiBase, state.webIdentity]))
 const boundary = module + identity
 const selectionSearch = new URLSearchParams()
 for (const value of new URLSearchParams(search).getAll('selection')) selectionSearch.append('selection', value)
 const selectionQuery = selectionSearch.toString()
 const requested = useMemo(() => {
  try { return { context: requestedRecordSelection(selectionQuery), error: '' } }
  catch (cause) { return { context: null, error: cause instanceof Error ? cause.message : 'The record selection is invalid' } }
 }, [selectionQuery])
 const handledSelection = useRef<typeof requested | null>(null)
 const [readyBoundary, setReadyBoundary] = useState<string | null>(null)
 useEffect(() => {
  let live = true
  handledSelection.current = null
  stopTaskWorkspace(module, identity)
  queueMicrotask(() => { if (live) setReadyBoundary(boundary) })
  return () => { live = false; stopTaskWorkspace(module, identity) }
 }, [module, boundary, identity])
 useEffect(() => {
  try {
   if (requested.error) throw new Error(requested.error)
   const state = useTaskWorkspace.getState()
   if (state.module !== module) return
   if (state.phase === 'loading') {
    if (!state.refreshing || handledSelection.current !== requested) {
     handledSelection.current = requested
     void state.refresh(requested.context, module).catch(() => {})
    }
   } else if (state.phase === 'ready' && requested.context) {
    const key = taskContextKey(requested.context)
    if (state.contexts.some(context => taskContextKey(context) === key)) {
     handledSelection.current = requested
     state.select(key)
    } else {
     handledSelection.current = requested
     void state.refresh(requested.context, module).catch(() => {})
    }
   }
  } catch (cause) {
   stopTaskWorkspace(module, identity)
   useTaskWorkspace.setState({ phase: 'error', error: cause instanceof Error ? cause.message : 'The record selection is invalid' })
  }
 }, [requested, module, boundary, identity, workspace.phase])
 const pendingSelection = workspace.phase === 'ready' && requested.context &&
  !workspace.contexts.some(context => taskContextKey(context) === taskContextKey(requested.context))
 if (workspace.module !== module || workspace.phase === 'loading' || readyBoundary !== boundary || pendingSelection) return <LoadingSpinner label="Loading working contexts…" />
 if (workspace.phase !== 'ready') return <TaskWorkspaceNotice module={module} />
 return children
}

export function TaskWorkspaceSelectors({ module = 'task' }: { module?: ProjectReadModule }) {
 const workspace = useTaskWorkspace()
 const { activeAgent, setActiveAgent } = useAppStore()
 const { select } = workspace
 const contexts = workspace.module === module ? workspace.contexts : []
 const selection = workspace.module === module ? workspace.selection : null
 const phase = workspace.module === module ? workspace.phase : 'loading'
 const location = useLocation(), navigate = useNavigate()
 const choose = (key: string) => {
  const next = contexts.find(context => taskContextKey(context) === key)
  if (!next) return
  select(key)
  const parameters = new URLSearchParams(location.search)
  parameters.set('selection', stringifyNativeJson(next))
  navigate(location.pathname + '?' + parameters, { replace: true })
 }
 const refresh = useWorkspaceRefresh()
 const projects = [...new Map(contexts.map(context => [taskProjectKey(context), context])).values()]
 const projectNames = new Map(workspace.projects.map(project => [project.project_id, project.name]))
 const projectName = (id: string) => projectNames.get(id)?.trim() || id
 const projectKey = selection ? taskProjectKey(selection) : ''
 const versions = contexts.filter(context => taskProjectKey(context) === projectKey)
 const display = phase === 'loading' ? 'Loading projects…' : phase === 'anonymous' ? 'Sign in to select a project' : phase === 'forbidden' ? 'Access denied' : phase === 'error' ? 'Unavailable' : selection ? projectName(selection.project_id) : projects.length ? 'Select a project' : 'No working contexts'
 return <div className="workspace-selectors" role="group" aria-label="Working context">
  <ContextSelect label="Project" value={projectKey} display={display} disabled={phase !== 'ready' || !projects.length}
   options={projects.map(context => ({ value: taskProjectKey(context), label: projectName(context.project_id), detail: context.project_dir }))}
   onChange={key => {
    const group = contexts.filter(context => taskProjectKey(context) === key)
    const next = group.find(context => context.project_version === selection?.project_version) ?? group[0]
    if (next) choose(taskContextKey(next))
   }} />
  <ContextSelect label="Version" value={selection ? taskContextKey(selection) : ''} display={selection?.project_version ?? 'No version selected'}
   disabled={phase !== 'ready' || !versions.length} options={versions.map(context => ({ value: taskContextKey(context), label: context.project_version }))} onChange={choose} />
  {module === 'hub' && <ContextSelect label="Agent" value={nativeBindings.includes(activeAgent as typeof nativeBindings[number]) ? activeAgent : ''} display={nativeBindings.includes(activeAgent as typeof nativeBindings[number]) ? bindingLabel(activeAgent) : 'Choose a native binding'} disabled={phase !== 'ready'} options={nativeBindings.map(binding => ({ value: binding, label: bindingLabel(binding) }))} onChange={setActiveAgent} />}
  <button className="context-refresh" type="button" onClick={() => void refresh?.refresh()} disabled={!refresh || refresh.refreshing}
   aria-label="Refresh" title="Refresh page and working context" aria-busy={refresh?.refreshing || false}>
   <RefreshCw size={17} className={refresh?.refreshing ? 'animate-spin' : ''} aria-hidden="true" />
  </button>
  {refresh?.error && <div className="workspace-refresh-error" role="alert">{refresh.error}</div>}
 </div>
}
export function TaskWorkspaceNotice({ module = 'task' }: { module?: ProjectReadModule }) {
 const { phase, error, refresh } = useTaskWorkspace()
 const { search } = useLocation()
 const retry = () => {
  try { void refresh(requestedRecordSelection(search), module).catch(() => {}) }
  catch (cause) { useTaskWorkspace.setState({ phase: 'error', error: cause instanceof Error ? cause.message : 'The record selection is invalid' }) }
 }
 const domain = module === 'now' ? 'project activity' : module === 'task' ? 'tasks' : module === 'memory' ? 'Memory' : module === 'ast' ? 'AST' : module === 'hub' ? 'Hub' : 'Knowledge'
 if (phase === 'loading') return <LoadingSpinner label="Loading working contexts…" />
 if (phase === 'anonymous') return <WorkEmpty title={'Sign in to read ' + domain}>Use Entrar in the header, then choose a project and version.</WorkEmpty>
 if (phase === 'forbidden' || phase === 'error') return <WorkNotice tone="error" title={phase === 'forbidden' ? 'Working context access denied' : 'Unable to read working contexts'}>
  {error}<button className="work-button" onClick={retry}>Retry working contexts</button>
 </WorkNotice>
 return <WorkEmpty title="Select a working context">Use the Project and Version menus in the header.</WorkEmpty>
}
