import { useEffect, useRef, useState } from 'react'
import { taskApi, type TaskContext, type TaskDetail, type TaskSpecification } from '@/api/task'
import { nativeTimestamp as timestamp } from '@/api/nativeJson'
import { WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { invalidateTaskWorkspace } from './taskWorkspace'
import { scheduleLabel } from './taskTimeline'

const rich = (content: string) => <MarkdownContent content={content} />

function useAuditPage<K extends 'history' | 'comments'>(section: K, detail: TaskDetail, selection: TaskContext) {
 const start = section === 'history' ? detail.history_start_cursor : detail.comments_start_cursor
 const [state, setState] = useState({ page: detail[section], cursor: start, previous: [] as string[], loading: false, error: '' })
 const request = useRef<AbortController | null>(null)
 const retry = useRef({ cursor: start, previous: [] as string[] })
 useEffect(() => () => request.current?.abort(), [])
 const read = async (cursor: string, previous: string[]) => {
  request.current?.abort()
  const controller = new AbortController(); request.current = controller
  retry.current = { cursor, previous }
  setState(current => ({ ...current, loading: true, error: '' }))
  try {
   const value = await taskApi.get(selection, detail.task.specification.id, { signal: controller.signal, [section]: { page_size: 20, cursor } })
   if (controller.signal.aborted) return
   if (!value) throw new Error('This task is no longer available.')
   const page = value[section]
   if (!page) throw new Error('The requested ' + section + ' page was not returned.')
   setState({ page, cursor, previous, loading: false, error: '' })
  } catch (cause) {
   if (controller.signal.aborted) return
   invalidateTaskWorkspace(cause)
   setState(current => ({ ...current, loading: false, error: cause instanceof Error ? cause.message : 'Unable to read ' + section }))
  }
 }
 return {
  ...state,
  load: () => read(start, []),
  retry: () => read(retry.current.cursor, retry.current.previous),
  next: () => state.page?.next_cursor && read(state.page.next_cursor, [...state.previous, state.cursor]),
  back: () => state.previous.length && read(state.previous[state.previous.length - 1], state.previous.slice(0, -1)),
 }
}

function PageActions({ name, state }: { name: string; state: ReturnType<typeof useAuditPage> }) {
 return <>
  {state.loading && <LoadingSpinner label={'Loading ' + name + '…'} />}
  {state.error && <WorkNotice title={'Unable to read ' + name} tone="error">{state.error}<button className="work-button" onClick={() => void state.retry()} disabled={state.loading}>Retry {name}</button></WorkNotice>}
  {!state.page && !state.loading && !state.error && <><p>{name === 'history' ? 'Lifecycle history has not been loaded.' : 'Comments have not been loaded.'}</p><button className="work-button" onClick={() => void state.load()}>Load {name}</button></>}
  {state.page && <nav className="work-actions" aria-label={name + ' pages'}>
   <button className="work-button" disabled={state.loading || !state.previous.length} onClick={() => void state.back()}>Previous {name} page</button>
   <span role="status">Page {state.previous.length + 1}</span>
   <button className="work-button" disabled={state.loading || !state.page.next_cursor} onClick={() => void state.next()}>Next {name} page</button>
  </nav>}
 </>
}

export function TaskComments({ detail, selection }: { detail: TaskDetail; selection: TaskContext }) {
 const state = useAuditPage('comments', detail, selection)
 return <WorkSection id="task-comments" title="Comments"><div aria-busy={state.loading}>
  <PageActions name="comments" state={state} />
  {state.page && <div className="work-timeline">{state.page.comments.map(comment => <article key={comment.id}><small>{comment.kind} · {comment.actor} · {timestamp(comment.at)} / rev {comment.revision}</small>{rich(comment.body)}</article>)}</div>}
  {state.page?.comments.length === 0 && <p>No comments on this page.</p>}
 </div></WorkSection>
}

function Snapshot({ label, specification }: { label: string; specification: TaskSpecification }) {
 return <div className="work-panel"><strong>{label}</strong><h3>{specification.title}</h3>{rich(specification.description)}<p>{scheduleLabel(specification.schedule)}</p>{specification.checks.map(check => <div key={check.id}><small>{check.kind} · {check.status}</small>{rich(check.text)}</div>)}</div>
}

export function TaskHistory({ detail, selection }: { detail: TaskDetail; selection: TaskContext }) {
 const state = useAuditPage('history', detail, selection)
 const page = state.page
 return <>
  <WorkSection id="task-history" title="Lifecycle"><div aria-busy={state.loading}>
   <div className="work-timeline"><article><time>{timestamp(detail.creation.record.created_at)}</time><h3>Created</h3><small>{detail.creation.actor}</small></article></div>
   <PageActions name="history" state={state} />
   {page && <div className="work-timeline">{page.events.map(event => <article key={String(event.record.specification.revision)}><time>{timestamp(event.observed_at)} / rev {event.record.specification.revision}</time><h3>{(typeof event.kind === 'string' ? event.kind : Object.keys(event.kind)[0]).replace(/_/g, ' ')}</h3><small>{event.actor}</small>{event.record.progress && <>{rich(event.record.progress.summary)}{rich(event.record.progress.next_step)}</>}{typeof event.kind === 'object' && 'flag_updated' in event.kind && rich(event.kind.flag_updated.reason)}{typeof event.kind === 'object' && 'commit_updated' in event.kind && <>{rich(event.kind.commit_updated.reason)}<p>Commit: {event.record.commit ? <code className="break-all">{event.record.commit}</code> : 'No commit associated'}</p></>}</article>)}</div>}
   {page?.events.length === 0 && <p>No lifecycle events on this page.</p>}
  </div></WorkSection>
  <WorkSection id="task-revisions" title="Specification revisions">
   {!page ? <p>Load lifecycle history to inspect specification revisions.</p> : <>{page.events.map((event, index) => typeof event.kind === 'object' && 'revised' in event.kind ? <details className="work-disclosure" key={String(event.record.specification.revision)}><summary>rev {event.record.specification.revision} · revised</summary><div>{rich(event.kind.revised.reason)}<small>{event.actor} · {timestamp(event.observed_at)}</small><div className="work-two-columns"><Snapshot label="Before" specification={(index ? page.events[index - 1].record : page.before).specification} /><Snapshot label="After" specification={event.record.specification} /></div></div></details> : null)}{!page.events.some(event => typeof event.kind === 'object' && 'revised' in event.kind) && <p>No specification revisions on this page.</p>}</>}
  </WorkSection>
 </>
}
