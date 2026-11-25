import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { WorkPage, WorkHeader, WorkSection, WorkEmpty, WorkNotice, FactList } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { useCallback, useEffect, useRef, useState } from 'react'
import { daemonApi, type DaemonStatus } from '@/api/daemon'
import { useAppStore } from '@/store/appStore'

function identityScope() {
  const { apiBase, webIdentity } = useAppStore.getState()
  return JSON.stringify([apiBase, webIdentity])
}

export default function DaemonDashboard() {
  const apiBase = useAppStore(state => state.apiBase)
  const webIdentity = useAppStore(state => state.webIdentity)
  const scope = JSON.stringify([apiBase, webIdentity])
  const [result, setResult] = useState<{ scope: string; status: DaemonStatus | null; error: string; loading: boolean }>({ scope: '', status: null, error: '', loading: true })
  const pending = useRef<AbortController | null>(null)
  const fetchStatus = useCallback(async () => {
    pending.current?.abort()
    const controller = new AbortController()
    pending.current = controller
    const current = () => !controller.signal.aborted && pending.current === controller && identityScope() === scope
    setResult(previous => ({ scope, status: previous.scope === scope ? previous.status : null, error: '', loading: true }))
    try {
      const status = await daemonApi.getStatus(controller.signal)
      if (current()) setResult({ scope, status, error: '', loading: false })
    } catch (cause) {
      if (current()) setResult({ scope, status: null, error: cause instanceof Error ? cause.message : 'Could not read daemon status.', loading: false })
    } finally {
      if (pending.current === controller) pending.current = null
    }
  }, [scope])

  useEffect(() => {
    const timer = setTimeout(() => void fetchStatus(), 0)
    const interval = setInterval(() => { if (!pending.current) void fetchStatus() }, 5000)
    return () => {
      clearTimeout(timer)
      clearInterval(interval)
      pending.current?.abort()
      pending.current = null
    }
  }, [fetchStatus])
  usePageRefresh(fetchStatus)

  const status = result.scope === scope ? result.status : null
  const error = result.scope === scope ? result.error : ''
  const loading = result.scope !== scope || result.loading
  return (
    <WorkPage>
      <WorkHeader title="Daemon operations" description="Inspect the local service and connect an agent." />
      <div className="operations-layout">
        <section>
          <WorkSection title="Process status" description="Global service · refreshes every 5 seconds">
            {error && <WorkNotice tone="error" title="Daemon status read failed">{error}</WorkNotice>}
            {loading && !status ? <LoadingSpinner label="Loading daemon status…" /> : status ? <>
              <FactList items={[["State", status.state === 'running' ? 'Running' : 'Stopping'], ["PID", status.pid]]} />
              {loading && <LoadingSpinner label="Refreshing daemon status…" />}
            </> : <WorkEmpty title="Daemon status unavailable" action={<button className="work-button" onClick={() => void fetchStatus()}>Try again</button>}>
              Check your browser session and connection, then try again.
            </WorkEmpty>}
          </WorkSection>
        </section>
        <aside>
          <WorkSection title="Connect an agent" description="Add Graphit to your agent's MCP configuration.">
            <pre className="work-code">{JSON.stringify({ mcpServers: { graphit: { command: 'graphit', args: ['mcp', '--stdio'] } } }, null, 2)}</pre>
            <p className="text-xs text-muted-foreground mt-4 leading-relaxed">Make the graphit command available in your agent's PATH.</p>
          </WorkSection>
        </aside>
      </div>
    </WorkPage>
  )
}
