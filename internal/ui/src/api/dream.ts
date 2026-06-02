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
  pending_subjects?: string[]
}

export interface DreamReport {
  id: string
  path: string
  created: string
  title: string
  size: number
  has_deep_sleep: boolean
}

export interface DreamSubject {
  Slug: string
  Title: string
  Body: string
  CreatedAt: string
  Done: boolean
  ResultPath: string
}

export const dreamApi = {
  getStatus: (projectDir: string) => api.get<DreamStatus>(`/dream/status?project_dir=${encodeURIComponent(projectDir)}`),
  getReports: (projectDir: string) => api.get<DreamReport[]>(`/dream/reports?project_dir=${encodeURIComponent(projectDir)}`),
  getSubjects: (projectDir: string) => api.get<DreamSubject[]>(`/dream/subjects?project_dir=${encodeURIComponent(projectDir)}`),
  addSubject: (projectDir: string, title: string, body: string) =>
    api.post<DreamSubject>(`/dream/subject?project_dir=${encodeURIComponent(projectDir)}`, { title, body }),
  removeSubject: (projectDir: string, slug: string) =>
    api.delete<{ success: boolean; message: string }>(`/dream/subject/${slug}?project_dir=${encodeURIComponent(projectDir)}`),
}
