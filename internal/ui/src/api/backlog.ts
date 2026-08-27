import { api } from './client'

export interface BacklogItem {
  slug: string
  title: string
  body?: string
  path: string
  created_at: string
  done: boolean
  result_path?: string
}

export const backlogApi = {
  list: (projectDir: string) =>
    api.get<BacklogItem[]>(`/backlog?project_dir=${encodeURIComponent(projectDir)}`),
  add: (projectDir: string, title: string, body: string) =>
    api.post<BacklogItem>(`/backlog/item?project_dir=${encodeURIComponent(projectDir)}`, { title, body }),
  remove: (projectDir: string, slug: string) =>
    api.delete<{ success: boolean; message: string }>(
      `/backlog/item/${slug}?project_dir=${encodeURIComponent(projectDir)}`,
    ),
}
