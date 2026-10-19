import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { hubNative, mutationArtifact, type InstalledQuery, type MutationSelection, type UpdateAllInput } from '@/api/hubNative'
import { WorkNotice } from '@/components/shared/EngineeringUI'
import { FileActionDialog, type FileIntent } from './FileActionDialog'
import { HubFailure, HubMutationFeedback } from './HubFeedback'
import { HubUpdateAllFeedback, UpdateAllDialog, type UpdateAllIntent } from './HubUpdateAll'
import { useHubMutation } from './useHubMutation'
type HubIntent = FileIntent | UpdateAllIntent
export function HubActions({ children, onCompleted, contextKey }: { contextKey: string; children: (actions: { open: (intent: HubIntent) => void; disabled: boolean; reason: string }) => ReactNode; onCompleted: () => Promise<void> }) {
  const mutation = useHubMutation()
  const [intent, setIntent] = useState<{ contextKey: string; value: HubIntent } | null>(null)
  const [inspecting, setInspecting] = useState(false), [inspection, setInspection] = useState<string | null>(null), [inspectionError, setInspectionError] = useState<unknown>(null)
  const pendingInspection = useRef<AbortController | null>(null), currentContext = useRef<string | null>(contextKey), currentCompleted = useRef(onCompleted)
  useLayoutEffect(() => { currentContext.current = contextKey; currentCompleted.current = onCompleted; return () => { currentContext.current = null } }, [contextKey, onCompleted])
  if (intent && intent.contextKey !== contextKey) setIntent(null)
  const activeIntent = intent?.contextKey === contextKey ? intent.value : null
  useEffect(() => () => pendingInspection.current?.abort(), [])
  async function inspect() {
    if (!mutation.result || mutation.pending || inspecting) return
    const epoch = mutation.result.epoch, request = structuredClone(mutation.result.request), collective = mutation.result.operation === 'update_all'
    const artifact = collective ? null : mutationArtifact(request as MutationSelection)
    const controller = new AbortController(); pendingInspection.current?.abort(); pendingInspection.current = controller
    setInspecting(true); setInspectionError(null); setInspection(null)
    try {
      let offset = 0, count = 0
      do {
        const query: InstalledQuery = { scope: request.scope, ...(request.scope.kind === 'project' && request.binding ? { binding: request.binding } : {}), offset, limit: 20,
          ...(artifact ? { artifact_type: artifact.artifact_type, id: artifact.id, ...(artifact.version ? { version: artifact.version } : {}) } : {}) }
        const page = await hubNative.installed(query, controller.signal)
        if (controller.signal.aborted) return
        count += page.entries.length
        if (!collective || page.next_offset === null) break
        if (page.next_offset <= offset) throw new Error('Invalid inventory cursor during captured-scope inspection')
        offset = page.next_offset
      } while (!controller.signal.aborted)
      if (!controller.signal.aborted && mutation.inspected(epoch)) setInspection(collective
        ? 'Read ' + count + ' current installations in the captured scope. Review the recorded item effects; inventory absence does not prove rollback or absence of physical residues.'
        : count ? 'The captured selection is listed in the authorized inventory. Review its current version and origin before another operation.' : 'The captured selection is not listed in the authorized inventory. This does not establish whether physical residues remain.')
    } catch (error) { if (!controller.signal.aborted) setInspectionError(error) }
    finally { if (!controller.signal.aborted) setInspecting(false) }
  }
  async function submit(input: MutationSelection | UpdateAllInput) {
    if (!activeIntent) return false
    const submittedContext = contextKey
    pendingInspection.current?.abort(); pendingInspection.current = null; setInspecting(false); setInspection(null); setInspectionError(null)
    const completed = activeIntent.operation === 'update_all' ? await mutation.perform('update_all', input as UpdateAllInput) : await mutation.perform(activeIntent.operation, input as MutationSelection)
    if (completed && currentContext.current === submittedContext) void currentCompleted.current()
    return completed
  }
  const disabled = mutation.pending || mutation.inspectionBlocked
  const result = mutation.result
  return <>
    {children({ open: value => { setInspection(null); setInspectionError(null); setIntent({ contextKey, value: structuredClone(value) }) }, disabled, reason: mutation.pending ? 'Another operation is still running.' : mutation.inspectionBlocked ? 'Inspect the original target before another operation.' : '' })}
    {result?.operation === 'update_all' ? <HubUpdateAllFeedback result={result} pending={mutation.pending} onInspect={() => { void inspect() }} inspecting={inspecting} /> : <HubMutationFeedback result={result} pending={mutation.pending} onInspect={() => { void inspect() }} inspecting={inspecting} />}
    {inspection && <WorkNotice title="Target inspection">{inspection}</WorkNotice>}
    {inspectionError !== null && <HubFailure error={inspectionError} />}
    {activeIntent && (activeIntent.operation === 'update_all' ? <UpdateAllDialog intent={activeIntent} pending={mutation.pending} blocked={mutation.inspectionBlocked} onSubmit={submit} onCancel={() => setIntent(null)} /> : <FileActionDialog intent={activeIntent} error={result?.error} pending={mutation.pending} blocked={mutation.inspectionBlocked} onSubmit={submit} onCancel={() => setIntent(null)} />)}
  </>
}
