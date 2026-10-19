import { useState } from 'react'
import type { TaskStatus } from '@/api/task'
import { WorkPage, WorkHeader, WorkNotice, WorkEmpty } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { TaskModeToggle } from './TaskModeToggle'
import { TaskFilters } from './TaskFilters'
import { TaskTimeline } from './TaskTimeline'
import { useTaskCatalogue } from './useTaskCatalogue'
import { TaskWorkspaceNotice } from './TaskWorkspace'
import { useNavigate } from 'react-router-dom'
export default function TaskGanttPage() {
 const navigate = useNavigate()
 const [query, setQuery] = useState('')
 const [status, setStatus] = useState<TaskStatus | 'all'>('all')
 const [flagged, setFlagged] = useState<boolean | undefined>(undefined)
 const [blocked, setBlocked] = useState<boolean | undefined>(undefined)
 const catalogue = useTaskCatalogue(query, status, flagged, blocked)
 usePageRefresh(catalogue.refresh)
 return <WorkPage><WorkHeader title="Task calendar" description="Explicit start, end and due dates. Dependencies never move the schedule." /><TaskModeToggle mode="gantt" />
 {!catalogue.ready ? <TaskWorkspaceNotice /> : <>
  <div className="work-toolbar"><TaskFilters query={query} onQuery={setQuery} status={status} onStatus={setStatus} flagged={flagged} onFlagged={setFlagged} blocked={blocked} onBlocked={setBlocked} /></div>
 {catalogue.error && <WorkNotice tone="error" title="Unable to read tasks">{catalogue.error}<button className="work-button" onClick={() => void catalogue.refresh().catch(() => {})}>Retry</button></WorkNotice>}
 {catalogue.loading ? <LoadingSpinner label="Loading tasks…" /> : <>
 <div className="work-catalogue-summary"><span>{catalogue.tasks.length}{catalogue.cursor ? '+' : ''} tasks in loaded pages</span><span>Circle: start/end · Diamond: due · Range: inclusive calendar days</span></div>
 {catalogue.tasks.length ? <TaskTimeline state="ready" items={catalogue.tasks} onOpenTask={id => navigate('/task/explorer/' + encodeURIComponent(id))} /> : !catalogue.error && <WorkEmpty title="No tasks match this view.">Change a filter or choose a project with recorded work.</WorkEmpty>}
 {catalogue.cursor && <button className="work-button" disabled={catalogue.loadingMore} onClick={() => void catalogue.loadMore()}>{catalogue.loadingMore ? 'Loading…' : 'Load more'}</button>}
 </>}
 </>}
 </WorkPage>
}
