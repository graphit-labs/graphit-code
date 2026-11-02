import { useEffect, useRef, useState } from 'react'
import { sessionApi, type SessionDetail, type SessionInput } from '@/api/taskSession'
import type { TaskContext } from '@/api/task'
import { nativeTimestamp as timestamp } from '@/api/nativeJson'
import { WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { useAppStore } from '@/store/appStore'
import { invalidateTaskWorkspace, taskContextKey, useTaskWorkspace } from './taskWorkspaceState'

const rich = (content: string) => <MarkdownContent content={content} />
const identity = () => { const state = useAppStore.getState(); return JSON.stringify([state.apiBase, state.webIdentity]) }
function Snapshot({ label, input }: { label: string; input: SessionInput }) {
 return <div className="work-panel"><strong>{label}</strong><h3>{input.title}</h3>{rich(input.description)}<h4>Strategy</h4>{rich(input.strategy)}</div>
}

/** The dossier keys this reader to one current response and native scope. */
export function SessionAuditSections({ detail, selection }: { detail: SessionDetail; selection: TaskContext }) {
 const [state, setState] = useState({ page: detail.history, cursor: detail.history_start_cursor, previous: [] as string[], loading: false, error: '', stopped: false })
 const request = useRef<AbortController | null>(null)
 const retry = useRef({ cursor: detail.history_start_cursor, previous: [] as string[] })
 useEffect(() => () => request.current?.abort(), [])
 const read = async (cursor: string, previous: string[]) => {
  request.current?.abort()
  const controller = new AbortController(); request.current = controller
  const owner = identity(), scope = taskContextKey(selection)
  const current = () => !controller.signal.aborted && request.current === controller && identity() === owner && taskContextKey(useTaskWorkspace.getState().selection) === scope
  retry.current = { cursor, previous }
  setState(value => ({ ...value, loading: true, error: '', stopped: false }))
  try {
   const value = await sessionApi.get(selection, detail.session.id, { signal: controller.signal, history: { page_size: 20, cursor } })
   if (!current()) return
   if (!value) throw new Error('This session is no longer available.')
   if (!value.history) throw new Error('The requested session history page was not returned.')
   setState({ page: value.history, cursor, previous, loading: false, error: '', stopped: false })
  } catch (cause) {
   if (!current()) return
   invalidateTaskWorkspace(cause)
   setState(value => ({ ...value, loading: false, error: cause instanceof Error ? cause.message : 'Unable to read session history' }))
  }
 }
 const cancel = () => { request.current?.abort(); setState(value => ({ ...value, loading: false, stopped: true })) }
 const page = state.page
 const checkpoints = page?.events.filter(event => event.kind === 'progress' && event.record.progress?.observation.kind === 'checkpoint') ?? []
 return <>
  <WorkSection id="session-history" title="History" description="Checkpoints, revisions and lifecycle share the selected history page.">
   <div aria-busy={state.loading}>
    {state.loading && <><LoadingSpinner label="Loading session history…" /><button className="work-button" onClick={cancel}>Cancel history</button></>}
    {state.stopped && <p>History read stopped.</p>}
    {state.error && <WorkNotice title="Unable to read session history" tone="error">{state.error}<button className="work-button" disabled={state.loading} onClick={() => void read(retry.current.cursor, retry.current.previous)}>Retry history</button></WorkNotice>}
    {!page && !state.loading && !state.error && <><p>Session history has not been loaded.</p><button className="work-button" onClick={() => void read(detail.history_start_cursor, [])}>Load history</button></>}
    {page && <nav className="work-actions" aria-label="Session history pages">
     <button className="work-button" disabled={state.loading || !state.previous.length} onClick={() => void read(state.previous[state.previous.length - 1], state.previous.slice(0, -1))}>Previous history page</button>
     <span role="status">Page {state.previous.length + 1}</span>
     <button className="work-button" disabled={state.loading || !page.next_cursor} onClick={() => page.next_cursor && void read(page.next_cursor, [...state.previous, state.cursor])}>Next history page</button>
    </nav>}
   </div>
  </WorkSection>
  <WorkSection id="session-checkpoints" title="Checkpoints">
   {!page ? <p>Load history to inspect checkpoints.</p> : <><p>{checkpoints.length} checkpoints on this page.</p><div className="work-timeline">{checkpoints.map(event => <article key={String(event.record.revision)}><time>{timestamp(event.observed_at)} / rev {event.record.revision}</time>{rich(event.record.progress!.summary)}<h3>Next step</h3>{rich(event.record.progress!.next_step)}<small>{event.actor}</small></article>)}</div>{!checkpoints.length && <p>No checkpoints on this page.</p>}</>}
  </WorkSection>
  <WorkSection id="session-revisions" title="Specification revisions">
   {!page ? <p>Load history to inspect specification revisions.</p> : <>{page.events.map((event, index) => typeof event.kind === 'object' && 'revised' in event.kind ? <details className="work-disclosure" key={String(event.record.revision)}><summary>rev {event.record.revision} · revised</summary>{rich(event.kind.revised.reason)}<small>{event.actor} · {timestamp(event.observed_at)}</small><div className="work-two-columns"><Snapshot label="Before" input={(index ? page.events[index - 1].record : page.before).input} /><Snapshot label="After" input={event.record.input} /></div></details> : null)}{!page.events.some(event => typeof event.kind === 'object' && 'revised' in event.kind) && <p>No specification revisions on this page.</p>}</>}
  </WorkSection>
  <WorkSection id="session-lifecycle" title="Lifecycle">
   <div className="work-timeline"><article><h3>Created</h3><time>{timestamp(detail.creation.record.created_at)}</time><small>{detail.creation.actor}</small></article>{page?.events.map(event => <article key={String(event.record.revision)}><time>{timestamp(event.observed_at)} / rev {event.record.revision}</time><h3>{(typeof event.kind === 'string' ? event.kind : Object.keys(event.kind)[0]).replace(/_/g, ' ')}</h3>{event.record.progress && rich(event.record.progress.summary)}<small>{event.actor}</small></article>)}</div>
   {!page ? <p>Load history to inspect lifecycle events.</p> : page.events.length === 0 && <p>No lifecycle events on this page.</p>}
  </WorkSection>
 </>
}
