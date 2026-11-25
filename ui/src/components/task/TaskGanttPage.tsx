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
import { projectRecordHref } from '@/api/nativeRecordNavigation'
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
  <TaskFilters query={query} onQuery={setQuery} status={status} onStatus={setStatus} flagged={flagged} onFlagged={setFlagged} blocked={blocked} onBlocked={setBlocked} />
 {catalogue.error && <WorkNotice tone="error" title="Unable to read tasks">{catalogue.error}<button className="work-button" onClick={() => void catalogue.refresh().catch(() => {})}>Retry</button></WorkNotice>}
 {catalogue.loading ? <LoadingSpinner label="Loading tasks…" /> : <>
 <div className="work-catalogue-summary"><span>{catalogue.tasks.length}{catalogue.cursor ? '+' : ''} tasks in loaded pages</span></div>
 {catalogue.tasks.length && catalogue.selection ? <TaskTimeline state="ready" items={catalogue.tasks} onOpenTask={id => { if (catalogue.selection) navigate(projectRecordHref('task', id, catalogue.selection)) }} /> : !catalogue.error && <WorkEmpty title="No tasks match this view.">Change a filter or choose a project with recorded work.</WorkEmpty>}
 {catalogue.paginationError && <WorkNotice title="Unable to load more tasks" tone="error">{catalogue.paginationError}</WorkNotice>}
 {catalogue.cursor && <button className="work-button" disabled={catalogue.loadingMore} onClick={() => void catalogue.loadMore()}>{catalogue.loadingMore ? 'Loading…' : catalogue.paginationError ? 'Retry next page' : 'Load more'}</button>}
 </>}
 </>}
 </WorkPage>
}
