import { useCallback, useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { nativeRecordRead } from '@/api/nativeRecordRead'
import { encodeProjectLocator, nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { FactList, WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSection } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'

interface LocalDreamStatus {
  enabled: boolean
  worker_attached: boolean
  idle_timeout_seconds: number
  max_duration_seconds: number
}

function duration(seconds: number) {
  if (seconds >= 3600 && seconds % 3600 === 0) return `${seconds / 3600} h`
  if (seconds >= 60 && seconds % 60 === 0) return `${seconds / 60} min`
  return `${seconds} s`
}

export function LocalDreamDashboard() {
  const selection = useTaskWorkspace(state => state.selection)
  const scope = nativeSelectionKey(selection)
  const [result, setResult] = useState<{ scope: string; status: LocalDreamStatus } | { scope: string; error: string } | null>(null)
  const request = useRef<AbortController | null>(null)
  const refresh = useCallback(async () => {
    if (!selection) return
    request.current?.abort()
    const controller = new AbortController()
    request.current = controller
    try {
      const status = await nativeRecordRead<LocalDreamStatus>('/dream/local/status', { selection }, controller.signal)
      if (!controller.signal.aborted) setResult({ scope, status })
    } catch (error) {
      if (!controller.signal.aborted) setResult({ scope, error: error instanceof Error ? error.message : 'Unable to read Dream status.' })
    }
  }, [scope, selection])
  useEffect(() => { let active = true; queueMicrotask(() => { if (active) void refresh() }); return () => { active = false; request.current?.abort() } }, [refresh])
  usePageRefresh(refresh)
  const current = result?.scope === scope ? result : null
  const status = current && 'status' in current ? current.status : null
  return <WorkPage>
    <WorkHeader title="Dream" description="Memory consolidation for the selected local checkout." />
    {!selection ? <WorkEmpty title="Select a project">Choose a local checkout in the header.</WorkEmpty>
      : !current ? <LoadingSpinner label="Loading Dream…" />
      : !status && 'error' in current ? <WorkNotice title="Dream status unavailable" tone="error">{current.error}<button className="work-button" onClick={() => void refresh()}>Retry status</button></WorkNotice>
      : status && <WorkSection title="Local Dream status" actions={<Link className="work-button" to={'/memory/explorer/project?' + new URLSearchParams({ selection: encodeProjectLocator(selection) })}>Open project Memory</Link>}>
        <FactList items={[
          ['Data version', selection.project_version],
          ['Configuration', status.enabled ? 'Enabled' : 'Disabled'],
          ['Worker', status.worker_attached ? 'Attached to this checkout' : 'No worker attached'],
          ['Idle timeout', duration(status.idle_timeout_seconds)],
          ['Maximum run time', duration(status.max_duration_seconds)],
        ]} />
        {status.enabled && <p>Run history is unavailable in the local daemon.</p>}
      </WorkSection>}
  </WorkPage>
}
