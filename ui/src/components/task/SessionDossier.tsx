import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { Download } from 'lucide-react'
import type { SessionDetail } from '@/api/taskSession'
import type { TaskContext } from '@/api/task'
import { nativeTimestamp as timestamp, stringifyNativeJson } from '@/api/nativeJson'
import { projectReferenceBinding } from '@/api/nativeRecordReferences'
import { referenceDestinationError } from '@/api/nativeReferenceNavigation'
import { WorkSection, WorkStatusBadge, WorkNotice, FactList } from '@/components/shared/EngineeringUI'
import { RecordReferences } from '@/components/shared/RecordReferences'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { useAppStore } from '@/store/appStore'
import { invalidateTaskWorkspace, taskContextKey, useTaskWorkspace } from './taskWorkspaceState'
import { SessionAuditSections } from './SessionAuditSections'
import { sessionAuditExport } from './sessionAuditExport'
const rich = (content: string) => <MarkdownContent content={content} />
const identity = () => { const state = useAppStore.getState(); return JSON.stringify([state.apiBase, state.webIdentity]) }
export function SessionDossier({ detail, selection, onBack, linkedTasks }: { detail: SessionDetail; selection: TaskContext; onBack: () => void; linkedTasks: ReactNode }) {
 const { session } = detail
 const location = useLocation()
 const exporter = useRef<AbortController | null>(null)
 const [exporting, setExporting] = useState(false)
 const [error, setError] = useState('')
 useEffect(() => () => exporter.current?.abort(), [])
 const exportSession = async () => {
  exporter.current?.abort()
  const controller = new AbortController(); exporter.current = controller
  const owner = identity(), scope = taskContextKey(selection)
  const current = () => !controller.signal.aborted && exporter.current === controller && identity() === owner && taskContextKey(useTaskWorkspace.getState().selection) === scope
  setExporting(true); setError('')
  try {
   const parts = await sessionAuditExport(detail, selection, controller.signal)
   if (!current()) return
   const url = URL.createObjectURL(new Blob(parts, { type: 'application/json' }))
   const anchor = document.createElement('a'); anchor.href = url; anchor.download = session.id + '.json'; anchor.click(); URL.revokeObjectURL(url)
  } catch (cause) {
   if (current()) { invalidateTaskWorkspace(cause); setError(cause instanceof Error ? cause.message : 'Unable to export session') }
  } finally { if (current()) setExporting(false) }
 }
 const binding = projectReferenceBinding(selection, 'session', session.id)
 const destinationError = referenceDestinationError(location.search, binding, true)
 if (destinationError) return <WorkNotice title="Reference destination unavailable" tone="error">{destinationError}</WorkNotice>
 return <article className="work-dossier" aria-label="Session brief">
  <header className="dossier-header"><div><div className="dossier-status"><WorkStatusBadge status={session.status} /><small><code>{session.id}</code> / revision {session.revision}</small></div><h2>{session.input.title}</h2></div><div className="work-actions"><button className="work-button" onClick={onBack}>Sessions</button><button className="work-button" disabled={exporting} aria-busy={exporting} onClick={() => void exportSession()}><Download size={15} />{exporting ? 'Exporting…' : 'Export complete JSON'}</button></div></header>
  {error && <WorkNotice title="Unable to export session" tone="error">{error}</WorkNotice>}
  <nav className="dossier-nav" aria-label="Session record sections"><a href="#session-intent">Request & strategy</a><a href="#session-work">Linked tasks</a><a href="#session-history">History</a><a href="#session-checkpoints">Checkpoints</a><a href="#session-revisions">Revisions</a><a href="#session-lifecycle">Lifecycle</a></nav>
  <div className="dossier-layout"><div className="dossier-content">
   {session.progress?.next_step && <WorkNotice title="Next step">{rich(session.progress.next_step)}</WorkNotice>}{session.progress?.summary && <WorkSection title="Latest progress">{rich(session.progress.summary)}</WorkSection>}
   <WorkSection id="session-intent" title="Request">{rich(session.input.description)}</WorkSection><WorkSection title="Strategy">{rich(session.input.strategy)}</WorkSection>
   {linkedTasks}
   <SessionAuditSections detail={detail} selection={selection} />
   <details className="work-disclosure"><summary>Current session response</summary><pre className="work-code">{stringifyNativeJson(detail, 2)}</pre></details>
  </div><aside className="dossier-context"><h3>Continuity</h3><FactList items={[["Owner", session.ownership.owner || 'Unclaimed'], ["Created", timestamp(session.created_at)], ["Updated", timestamp(session.updated_at)], ["Lease", timestamp(session.ownership.lease_expires_at)], ["Claim epoch", session.ownership.claim_epoch]]} /><WorkNotice title="One evolving request">The session preserves intent across tasks and agent handoffs.</WorkNotice><RecordReferences binding={binding} /></aside></div>
 </article>
}
