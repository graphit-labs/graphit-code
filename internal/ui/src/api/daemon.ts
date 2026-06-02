import { api } from './client'

export interface DaemonStatus {
  pid: number
  running: boolean
  started_at?: string
  uptime_seconds?: number
  pid_file_path: string
  scheduler_status: string
  recent_logs?: string[]
}

export const daemonApi = {
  getStatus: () => api.get<DaemonStatus>('/daemon/status'),
  stop: () => api.post<{ success: boolean; message: string }>('/daemon/stop', {}),
}
