import { createContext, useCallback, useContext, useLayoutEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'
import { useAppStore } from '@/store/appStore'

type Loader = () => void | Promise<unknown>
type Registration = { scope: string; run: Loader }
const RefreshContext = createContext<{
  register: (entry: Registration) => () => void
  scope: string
  refresh: () => Promise<void>
  refreshing: boolean
  error: string
} | null>(null)

export function WorkspaceRefreshProvider({ children }: { children: React.ReactNode }) {
  const { pathname } = useLocation()
  const { activeProjectDir, activeAgent, loadProjects } = useAppStore()
  const scope = JSON.stringify([pathname, activeProjectDir, activeAgent])
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
      const state = useAppStore.getState()
      return currentScope.current === scope && state.activeProjectDir === activeProjectDir && state.activeAgent === activeAgent
    }
    try {
      let catalogError: unknown
      try {
        await loadProjects()
        if (useAppStore.getState().projectsError) catalogError = new Error(useAppStore.getState().projectsError)
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
  }, [scope, activeProjectDir, activeAgent, loadProjects])
  return <RefreshContext.Provider value={{ register, scope, refresh, refreshing, error: failure.scope === scope ? failure.message : '' }}>{children}</RefreshContext.Provider>
}

/** Registers data work, never a page reset. Existing local state stays mounted. */
export function usePageRefresh(loader: Loader) {
  const context = useContext(RefreshContext)
  const latest = useRef(loader)
  useLayoutEffect(() => { latest.current = loader })
  const register = context?.register, scope = context?.scope
  useLayoutEffect(() => {
    if (!register || scope === undefined) return
    return register({ scope, run: () => latest.current() })
  }, [register, scope])
}

export function useWorkspaceRefresh() { return useContext(RefreshContext) }

/** Wait for every data source, including when another one fails. */
export async function refreshAll<T extends readonly unknown[]>(operations: { [K in keyof T]: Promise<T[K]> }): Promise<T> {
  const results = await Promise.allSettled(operations)
  const rejected = results.find(result => result.status === 'rejected')
  if (rejected?.status === 'rejected') throw rejected.reason
  return results.map(result => (result as PromiseFulfilledResult<unknown>).value) as unknown as T
}
