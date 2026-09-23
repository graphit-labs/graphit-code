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
  last_run?: DreamRunRecord
}

export interface DreamRunRecord {
  run_id: string
  project_id?: string
  agent?: string
  cli?: string
  started_at: string
  finished_at?: string
  status: string
  tool_calls: number
  memory_mutation_attempts: number
  target_ids?: string[]
  error_summary?: string
}

export const dreamApi = {
  getStatus: (projectDir: string) =>
    api.get<DreamStatus>(`/dream/status?project_dir=${encodeURIComponent(projectDir)}`),
}
