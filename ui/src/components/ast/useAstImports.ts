import { useCallback, useEffect, useRef, useState } from 'react'
import { nativeAstApi, type AstInstalledContext } from '@/api/nativeAst'

export function useAstImports(enabled: boolean) {
  const [state, setState] = useState<{ items: AstInstalledContext[]; loading: boolean; error: string }>({ items: [], loading: enabled, error: '' })
  const controller = useRef<AbortController | null>(null)
  const epoch = useRef(0)
  const load = useCallback(async (keepItems = false) => {
    controller.current?.abort()
    const abort = new AbortController(); controller.current = abort
    const current = ++epoch.current
    setState(current => ({ items: keepItems ? current.items : [], loading: enabled, error: '' }))
    if (!enabled) return null
    try {
      const reply = await nativeAstApi.imports(abort.signal)
      if (abort.signal.aborted || current !== epoch.current) return null
      setState(current => ({ items: reply.imports.map(item => current.items.find(old => old.id === item.id && old.version === item.version && old.context === item.context && old.backend === item.backend) ?? item), loading: false, error: '' }))
      return reply
    } catch (cause) {
      if (!abort.signal.aborted && current === epoch.current) setState({ items: [], loading: false, error: cause instanceof Error ? cause.message : 'The installed AST contexts could not load.' })
      return null
    }
  }, [enabled])
  const dispose = useCallback(() => { epoch.current++; controller.current?.abort() }, [])
  useEffect(() => { let active = true; queueMicrotask(() => { if (active) void load() }); return () => { active = false; dispose() } }, [load, dispose])
  const refresh = useCallback(async () => { await load(true) }, [load])
  return { ...(enabled ? state : { items: [], loading: false, error: '' }), refresh, reload: load }
}
