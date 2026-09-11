import { api } from './client'

export type MemoryScope = 'project' | 'user'

export interface MemoryScopeInfo {
  id: MemoryScope
  label: string
  scope_id: string
  kind: MemoryScope
}

export interface MemoryCatalogItem {
  id: string
  title: string
  type: string
  tags: string[]
  important: boolean
  mandatory: boolean
  created_at: string
  updated_at: string
  revision: number
  snippet?: string
  score?: number
}

export interface MemoryCatalog {
  results: MemoryCatalogItem[]
  total: number
  types: string[]
  tags: string[]
}

export interface MemoryVersion {
  key: string
  id: string
  revision_id?: string
  status: 'current' | 'superseded'
  title: string
  body: string
  type: string
  tags: string[]
  important: boolean
  mandatory: boolean
  created_at: string
  updated_at: string
  revision: number
  updated_by?: string
  previous?: string
  next?: string
  scope: string
  scope_id: string
  project_id?: string
  content_hash?: string
}

export interface MemoryTrace {
  memory_id: string
  current?: MemoryVersion
  revisions: MemoryVersion[]
}

export interface MemoryCatalogOptions {
  projectDir?: string
  scope: MemoryScope
  query?: string
  type?: string
  tag?: string
  important?: string
  mandatory?: string
}

export interface MemoryWrite {
  title: string
  body: string
  type: string
  tags?: string[]
  important?: boolean
  mandatory?: boolean
}

export type MemoryUpdate = Partial<Omit<MemoryWrite, 'tags'>>

function memoryPriority(item: MemoryCatalogItem) {
  if (item.mandatory) return 0
  if (item.important) return 1
  return 2
}

function memoryDate(item: MemoryCatalogItem) {
  for (const value of [item.updated_at, item.created_at]) {
    const parsed = Date.parse(value)
    if (!Number.isNaN(parsed)) return parsed
  }
  return 0
}

// The API owns this contract; the client repeats it defensively so the explorer cannot present a
// stale or mocked response in a different order.
export function sortMemoryCatalogItems(items: MemoryCatalogItem[]) {
  return [...items].sort((left, right) => {
    const priority = memoryPriority(left) - memoryPriority(right)
    if (priority !== 0) return priority
    const leftDate = memoryDate(left)
    const rightDate = memoryDate(right)
    if (leftDate !== rightDate) return rightDate - leftDate
    const leftRaw = left.updated_at || left.created_at
    const rightRaw = right.updated_at || right.created_at
    if (leftRaw !== rightRaw) return rightRaw.localeCompare(leftRaw)
    return left.id.localeCompare(right.id)
  })
}

function params(projectDir: string | undefined, scope?: MemoryScope) {
  const values = new URLSearchParams()
  if (projectDir) values.set('project_dir', projectDir)
  if (scope) values.set('scope', scope)
  return values
}

export const memoryApi = {
  scopes(projectDir?: string) {
    const query = params(projectDir).toString()
    return api.get<MemoryScopeInfo[]>(`/memories/scopes${query ? `?${query}` : ''}`)
  },
  list(options: MemoryCatalogOptions) {
    const query = params(options.projectDir, options.scope)
    if (options.query) query.set('query', options.query)
    if (options.type && options.type !== 'all') query.set('type', options.type)
    if (options.tag && options.tag !== 'all') query.set('tag', options.tag)
    if (options.important && options.important !== 'all') query.set('important', options.important)
    if (options.mandatory && options.mandatory !== 'all') query.set('mandatory', options.mandatory)
    return api.get<MemoryCatalog>(`/memories?${query.toString()}`)
  },
  detail(projectDir: string | undefined, scope: MemoryScope, id: string) {
    const query = params(projectDir, scope)
    return api.get<MemoryTrace>(`/memories/${encodeURIComponent(id)}?${query.toString()}`)
  },
  create(projectDir: string | undefined, scope: MemoryScope, body: MemoryWrite) {
    const query = params(projectDir, scope)
    return api.post<MemoryTrace>(`/memories?${query.toString()}`, body)
  },
  update(projectDir: string | undefined, scope: MemoryScope, id: string, body: MemoryUpdate) {
    const query = params(projectDir, scope)
    return api.patch<MemoryTrace>(`/memories/${encodeURIComponent(id)}?${query.toString()}`, body)
  },
  remove(projectDir: string | undefined, scope: MemoryScope, id: string) {
    const query = params(projectDir, scope)
    query.set('confirm', 'true')
    return api.delete<{ id: string; removed: boolean }>(`/memories/${encodeURIComponent(id)}?${query.toString()}`)
  },
}
