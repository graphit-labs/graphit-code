import { useCallback, useLayoutEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'
import { useAppStore } from '@/store/appStore'
import { taskContextKey, useTaskWorkspace } from '@/components/task/taskWorkspace'
import { importedKnowledgeLocation, installedHubRoute, projectReadModule, requestedRecordSelection, userMemoryRoute } from '@/api/nativeRecordNavigation'
import { RefreshContext, type Registration } from './WorkspaceRefresh'

export function WorkspaceRefreshProvider({ children }: { children: React.ReactNode }) {
  const { pathname, search } = useLocation()
  const { activeProjectDir, activeAgent, loadProjects, apiBase, webIdentity } = useAppStore()
  const { selection, refresh: refreshTaskContexts } = useTaskWorkspace()
  const module = projectReadModule(pathname, search)
  const independent = installedHubRoute(pathname) || userMemoryRoute(pathname) || (importedKnowledgeLocation(pathname, search)) || ((pathname === '/ast/contexts' || pathname.startsWith('/ast/explorer')) && module !== 'ast')
  const taskKey = taskContextKey(selection)
  const scope = JSON.stringify(installedHubRoute(pathname) ? [pathname, apiBase, webIdentity] : independent ? [pathname, search] : module ? [module, pathname, taskKey] : [pathname, activeProjectDir, activeAgent])
  const currentScope = useRef(scope)
  useLayoutEffect(() => { currentScope.current = scope }, [scope])
  const entries = useRef(new Set<Registration>())
  const running = useRef(false)
  const [refreshing, setRefreshing] = useState(false)
  const [failure, setFailure] = useState({ scope: '', message: '' })
  const register = useCallback((entry: Registration) => {
    entries.current.add(entry)
    return () => { entries.current.delete(entry) }
  }, [])
  const refresh = useCallback(async () => {
    if (running.current) return
    running.current = true
    setRefreshing(true)
    setFailure({ scope, message: '' })
    const targets = [...entries.current].filter(entry => entry.scope === scope)
    const stillCurrent = () => {
      if (independent) return currentScope.current === scope
      const state = useAppStore.getState()
      return currentScope.current === scope && (module ? useTaskWorkspace.getState().module === module && taskContextKey(useTaskWorkspace.getState().selection) === taskKey : state.activeProjectDir === activeProjectDir && state.activeAgent === activeAgent)
    }
    try {
      let catalogError: unknown
      try {
        if (module) await refreshTaskContexts(requestedRecordSelection(search), module)
        else if (!independent) {
          await loadProjects()
          if (useAppStore.getState().projectsError) catalogError = new Error(useAppStore.getState().projectsError)
        }
      } catch (error) { catalogError = error }
      if (!stillCurrent()) return
      const results = await Promise.allSettled(targets.filter(entry => entries.current.has(entry)).map(entry => Promise.resolve().then(entry.run)))
      const rejected = results.find(result => result.status === 'rejected')
      if (rejected?.status === 'rejected') throw rejected.reason
      if (catalogError) throw catalogError
    } catch (error) {
      if (stillCurrent()) setFailure({ scope, message: error instanceof Error ? error.message : 'Could not refresh this page. Try again.' })
    } finally {
      running.current = false
      setRefreshing(false)
    }
  }, [scope, search, activeProjectDir, activeAgent, loadProjects, module, independent, taskKey, refreshTaskContexts])
  return <RefreshContext.Provider value={{ register, scope, refresh, refreshing, error: failure.scope === scope ? failure.message : '' }}>{children}</RefreshContext.Provider>
}
