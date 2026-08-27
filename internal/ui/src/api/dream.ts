import { api } from './client'

export interface DreamStatus {
  enabled: boolean
  daemon_running: boolean
  daemon_pid?: number
  status: string
  session_id?: string
  last_dream_at?: string
  last_user_edit_at?: string
  idle_timeout: string
  max_duration: string
  total_reports: number
  pending_backlog?: string[]
}

export interface DreamReport {
  id: string
  path: string
  created: string
  title: string
  size: number
  has_deep_sleep: boolean
}

export const dreamApi = {
  getStatus: (projectDir: string) =>
    api.get<DreamStatus>(`/dream/status?project_dir=${encodeURIComponent(projectDir)}`),
  getReports: (projectDir: string) =>
    api.get<DreamReport[]>(`/dream/reports?project_dir=${encodeURIComponent(projectDir)}`),
}
