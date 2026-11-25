import { createContext, useContext, useLayoutEffect, useRef } from 'react'

type Loader = () => void | Promise<unknown>
export type Registration = { scope: string; run: Loader }
export const RefreshContext = createContext<{
  register: (entry: Registration) => () => void
  scope: string
  refresh: () => Promise<void>
  refreshing: boolean
  error: string
} | null>(null)

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
