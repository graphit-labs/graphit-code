import { api } from './client'
import type { TaskCatalogItem, TaskStatus } from './task'

export type SessionStatus = TaskStatus

export interface SessionSummary {
  id: string
  title: string
  status: SessionStatus
  owner?: string
  progress_summary?: string
  next_step?: string
  updated_at: string
  revision: number
}

export interface SessionSearchResult extends SessionSummary {
  score?: number
}

export interface SessionCheckpoint {
  key: string
  session_id: string
  sequence: number
  revision: number
  actor: string
  at: string
  summary: string
  problems?: string
  decisions?: string
  strategy?: string
  next_step: string
}

export interface SessionSpec {
  title: string
  description: string
  strategy: string
}

export interface SessionSpecRevision {
  key: string
  session_id: string
  source_revision: number
  actor: string
  reason: string
  at: string
  before: SessionSpec
  after: SessionSpec
}

export interface SessionEvent {
  key: string
  session_id: string
  revision: number
  type: string
  actor: string
  at: string
  from_status?: SessionStatus
  to_status: SessionStatus
  summary: string
  next_step?: string
}

export interface Session {
  id: string
  project_id: string
  idempotency_key: string
  title: string
  description: string
  strategy: string
  status: SessionStatus
  owner?: string
  claim_epoch: number
  claimed_at?: string
  lease_expires_at?: string
  heartbeat_at?: string
  checkpoint_sequence: number
  progress_summary?: string
  next_step?: string
  completed_by?: string
  completed_at?: string
  created_at: string
  updated_at: string
  revision: number
}

export interface SessionDetail {
  session: Session
  events: SessionEvent[]
  checkpoints: SessionCheckpoint[]
  spec_revisions: SessionSpecRevision[]
  tasks: TaskCatalogItem[]
}

export interface SessionCatalogPage {
  results: SessionSearchResult[]
  next_cursor: string
}

export interface SessionCatalogOptions {
  projectDir?: string
  projectId?: string
  query?: string
  status?: string
  active?: boolean
  pageSize?: number
  cursor?: string
}

export const sessionApi = {
  list: ({ projectDir, projectId, query, status, active, pageSize = 20, cursor }: SessionCatalogOptions) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (projectId) params.set('project_id', projectId)
    if (query) params.set('query', query)
    if (status && status !== 'all') params.set('status', status)
    if (active) params.set('active', 'true')
    params.set('page_size', String(pageSize))
    if (cursor) params.set('cursor', cursor)
    return api.get<SessionCatalogPage>(`/tasks/sessions?${params.toString()}`)
  },
  get: (projectDir: string | undefined, id: string, projectId?: string) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    if (projectId) params.set('project_id', projectId)
    const query = params.toString()
    return api.get<SessionDetail>(`/tasks/sessions/${encodeURIComponent(id)}${query ? `?${query}` : ''}`)
  },
}
