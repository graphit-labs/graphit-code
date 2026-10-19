import type { TaskStatus, TaskSearchMode } from '@/api/task'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { WorkSearch } from '@/components/shared/EngineeringUI'
interface Props {
 query: string
 searchMode?: TaskSearchMode
 onSearchMode?: (value: TaskSearchMode) => void
 onQuery: (value: string) => void
 status: TaskStatus | 'all'
 onStatus: (value: TaskStatus | 'all') => void
 flagged: boolean | undefined
 onFlagged: (value: boolean | undefined) => void
 blocked: boolean | undefined
 onBlocked: (value: boolean | undefined) => void
}
export function TaskFilters({ query, onQuery, searchMode, onSearchMode, status, onStatus, flagged, onFlagged, blocked, onBlocked }: Props) {
 return <><WorkSearch label="Search tasks" value={query} onChange={onQuery} placeholder={searchMode ? "Search specifications, checks and comments" : "Filter task titles"} />
 {searchMode && onSearchMode && <StyledSelect aria-label="Task search mode" menuLabel="Search mode" value={searchMode} onChange={event => onSearchMode(event.target.value as TaskSearchMode)}>
 <option value="fts">Text</option><option value="semantic">Semantic</option><option value="hybrid">Hybrid</option>
 </StyledSelect>}
 <StyledSelect aria-label="Filter task status" menuLabel="Task statuses" value={status} onChange={event => onStatus(event.target.value as TaskStatus | 'all')}>
 <option value="all">All statuses</option><option value="open">Open</option><option value="in_progress">In progress</option><option value="completed">Completed</option><option value="cancelled">Cancelled</option>
 </StyledSelect>
 <StyledSelect aria-label="Filter task flags" menuLabel="Task flags" value={flagged === undefined ? 'all' : String(flagged)} onChange={event => onFlagged(event.target.value === 'all' ? undefined : event.target.value === 'true')}>
 <option value="all">All flags</option><option value="true">Flagged</option><option value="false">Not flagged</option>
 </StyledSelect>
 <StyledSelect aria-label="Filter task blockers" menuLabel="Dependency blockers" value={blocked === undefined ? 'all' : String(blocked)} onChange={event => onBlocked(event.target.value === 'all' ? undefined : event.target.value === 'true')}>
 <option value="all">All dependency states</option><option value="true">Blocked open tasks</option><option value="false">Not blocked</option>
 </StyledSelect></>
}
