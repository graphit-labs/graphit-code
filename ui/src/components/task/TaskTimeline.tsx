import type { TaskTimelineItem } from '@/api/task'
import { WorkNotice, WorkEmpty, WorkStatusBadge } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { calendarAxis, civilDateLabel, timelineRows, type TimelineRow } from './taskTimeline'
import './task-timeline.css'

function Identity({ row, onOpenTask }: { row: TimelineRow; onOpenTask?: (id: string) => void }) {
 return <div className="task-timeline-identity">{onOpenTask ? <button className="task-timeline-link" onClick={() => onOpenTask(row.task.id)}>{row.task.title}</button> : <strong>{row.task.title}</strong>}<code>{row.task.id}</code><WorkStatusBadge status={row.task.status} /></div>
}
function Dependencies({ row, onOpenTask }: { row: TimelineRow; onOpenTask?: (id: string) => void }) {
 return row.dependencies.length ? <div className="task-timeline-dependencies"><span>Depends on</span><ul>{row.dependencies.map(dependency => <li key={dependency.id}>{onOpenTask ? <button className="task-timeline-link" aria-label={'Open dependency ' + dependency.id} onClick={() => onOpenTask(dependency.id)}>{dependency.title ?? dependency.id}</button> : <code>{dependency.id}</code>}{dependency.title === undefined && <span>Outside loaded tasks</span>}</li>)}</ul></div> : null
}
export function TaskTimeline({ state, items = [], message, onOpenTask }: { state: 'ready' | 'loading' | 'error'; items?: readonly TaskTimelineItem[]; message?: string; onOpenTask?: (id: string) => void }) {
 if (state === 'loading') return <div role="status"><LoadingSpinner label="Loading task dates…" /></div>
 if (state === 'error') return <WorkNotice tone="error" title="Unable to read task dates">{message}</WorkNotice>
 if (!items.length) return <WorkEmpty title="No tasks to display">Change a filter or choose a project with recorded work.</WorkEmpty>
 const rows = timelineRows(items)
 const scheduled = rows.filter(row => row.dates !== null)
 const undated = rows.filter(row => row.dates === null && !row.error)
 const invalid = rows.filter(row => row.error)
 const axis = calendarAxis(rows)
 return <div className="task-timeline">
 {invalid.length > 0 && <WorkNotice tone="error" title="Some task dates are invalid">Invalid calendar data remains visible below; no dates are corrected.</WorkNotice>}
 {axis && <section aria-label="Scheduled tasks"><div className="task-timeline-heading"><h2>Scheduled tasks</h2><p>Inclusive ranges and explicit date markers</p></div><ul className="task-timeline-legend"><li><span className="task-timeline-swatch" />Start through end</li><li><span className="task-timeline-symbol start" />Single endpoint</li><li><span className="task-timeline-symbol due" />Due date</li></ul>
 <div className="task-timeline-scroll" tabIndex={0} aria-label="Scrollable task calendar"><div className="task-timeline-columns"><div className="task-timeline-column-label">Task / status</div><div className="task-timeline-axis" aria-hidden="true">{axis.ticks.map(date => <time key={date.getTime()} style={{ left: axis.scale(date) + '%' }}>{civilDateLabel(date)}</time>)}</div></div><ul className="task-timeline-scheduled-list">
 {scheduled.map(row => <li className="task-timeline-row" key={row.task.id}><div className="task-timeline-label"><Identity row={row} onOpenTask={onOpenTask} /><dl className="task-timeline-facts">{row.dates!.start && <div><dt>Start</dt><dd>{civilDateLabel(row.dates!.start)}</dd></div>}{row.dates!.end && <div><dt>End</dt><dd>{civilDateLabel(row.dates!.end)}</dd></div>}{row.dates!.due && <div><dt>Due</dt><dd>{civilDateLabel(row.dates!.due)}</dd></div>}{row.dates!.durationDays !== undefined && <div><dt>Duration</dt><dd>{row.dates!.durationDays} {row.dates!.durationDays === 1 ? 'day' : 'days'} inclusive</dd></div>}</dl><Dependencies row={row} onOpenTask={onOpenTask} /></div>
 <div className="task-timeline-track" data-status={row.task.status} aria-hidden="true">{axis.ticks.map(date => <div className="task-timeline-gridline" key={date.getTime()} style={{ left: axis.scale(date) + '%' }} />)}{row.dates!.start && row.dates!.end ? <div className="task-timeline-range" style={{ left: axis.scale(row.dates!.start) + '%', width: axis.rangeEnd(row.dates!.end) - axis.scale(row.dates!.start) + '%' }} /> : <>{row.dates!.start && <div className="task-timeline-marker start" style={{ left: axis.scale(row.dates!.start) + '%' }} />}{row.dates!.end && <div className="task-timeline-marker end" style={{ left: axis.scale(row.dates!.end) + '%' }} />}</>}{row.dates!.due && <div className="task-timeline-marker due" style={{ left: axis.scale(row.dates!.due) + '%' }} />}</div>
 </li>)}
 </ul></div></section>}
 {undated.length > 0 && <section aria-label="Without dates"><div className="task-timeline-heading"><h2>Without dates</h2><p>No schedule or duration is inferred.</p></div><ul className="task-timeline-plain-list">{undated.map(row => <li key={row.task.id}><Identity row={row} onOpenTask={onOpenTask} /><Dependencies row={row} onOpenTask={onOpenTask} /></li>)}</ul></section>}
 {invalid.length > 0 && <section aria-label="Invalid task dates"><h2>Invalid task dates</h2><ul className="task-timeline-plain-list">{invalid.map(row => <li key={row.task.id}><Identity row={row} onOpenTask={onOpenTask} /><p>{row.error}</p></li>)}</ul></section>}
 </div>
}
