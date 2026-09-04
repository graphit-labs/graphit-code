import { api } from './client'

export type TaskStatus = 'open' | 'in_progress' | 'completed' | 'cancelled'

export interface TaskCheck {
  id: string
  kind: string
  text: string
  status: string
  evidence?: string
  verified_by?: string
  verified_at?: string
  superseded_by?: string
  superseded_reason?: string
  superseded_at?: string
  replacement_check_id?: string
}

export interface Task {
  id: string
  project_id: string
  parent_id?: string
  idempotency_key: string
  title: string
  description: string
  type: string
  status: TaskStatus
  priority: number
  depends_on?: string[]
  checks: TaskCheck[]
  flagged: boolean
  flag_reason?: string
  owner?: string
  claim_epoch: number
  claimed_at?: string
  lease_expires_at?: string
  heartbeat_at?: string
  progress_sequence: number
  comment_sequence: number
  progress_summary?: string
  next_step?: string
  completed_by?: string
  completed_at?: string
  created_at: string
  updated_at: string
  revision: number
  ready: boolean
  blocked_by?: string[]
}

export interface TaskDependency {
  key: string
  task_id: string
  depends_on: string
  active: boolean
  created_at: string
  created_by: string
  revision: number
}

export interface TaskCheckRecord extends TaskCheck {
  key: string
  task_id: string
  active: boolean
  revision: number
}

export interface TaskEvent {
  key: string
  task_id: string
  sequence: number
  type: string
  actor: string
  at: string
  from_status?: TaskStatus
  to_status?: TaskStatus
  summary?: string
  next_step?: string
  revision: number
}

export interface TaskComment {
  id: string
  task_id: string
  idempotency_key: string
  sequence: number
  kind: string
  body: string
  actor: string
  at: string
  revision: number
}

export interface TaskSpec {
  title: string
  description: string
  type: string
  priority: number
  parent_id?: string
  depends_on?: string[]
  checks: TaskCheck[]
}

export interface TaskSpecRevision {
  key: string
  task_id: string
  source_revision: number
  kind: string
  subject_id?: string
  actor: string
  reason: string
  at: string
  before: TaskSpec
  after: TaskSpec
}

export interface TaskExportDocument {
  schema_version: number
  project_id: string
  task_id?: string
  tasks: Task[]
  dependencies: TaskDependency[]
  checks: TaskCheckRecord[]
  events: TaskEvent[]
  comments: TaskComment[]
  spec_revisions: TaskSpecRevision[]
}

export interface TaskCatalogItem {
  id: string
  title: string
  type: string
  status: TaskStatus
  priority: number
  owner?: string
  flagged: boolean
  ready: boolean
  blocked_by?: string[]
  updated_at: string
}

export interface TaskCatalogPage {
  results: TaskCatalogItem[]
  next_cursor: string
}

export interface TaskCatalogOptions {
  projectDir?: string
  query?: string
  status?: string
  pageSize?: number
  cursor?: string
}

export const taskApi = {
  list: ({ projectDir, query, status, pageSize = 20, cursor }: TaskCatalogOptions) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (query) params.set('query', query)
    if (status && status !== 'all') params.set('status', status)
    params.set('page_size', String(pageSize))
    if (cursor) params.set('cursor', cursor)
    return api.get<TaskCatalogPage>(`/tasks?${params.toString()}`)
  },
  export: (projectDir?: string, id?: string) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (id) params.set('id', id)
    const query = params.toString()
    return api.get<TaskExportDocument>(`/tasks/export${query ? `?${query}` : ''}`)
  },
}
