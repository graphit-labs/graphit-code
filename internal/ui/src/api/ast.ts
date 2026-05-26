import { api } from './client'

export interface Context {
  id: string
  name: string
  type: 'project' | 'import'
  database?: string
  node_count?: number
  edge_count?: number
  path?: string
  imported_at?: string
  db_path?: string
}

export interface ContextsResponse {
  contexts: Context[]
  project_root: string
  project_name: string
}

export interface GraphNode {
  id: string
  name: string
  label: string
  type: string
  file?: string
  properties?: Record<string, unknown>
}

export interface GraphEdge {
  source: string
  target: string
  type: string
}

export interface TabularResult {
  columns: string[]
  rows: unknown[][]
}

export interface GraphResponse {
  nodes: GraphNode[]
  links: GraphEdge[]
  files: string[]
  fileContents: Record<string, string>
  tabular?: TabularResult
}

export interface SchemaNodeStat {
  label: string
  count: number
}

export interface SchemaEdgeStat {
  type: string
  count: number
}

export interface SchemaResponse {
  nodes: SchemaNodeStat[]
  edges: SchemaEdgeStat[]
  node_labels: string[]
  edge_types: string[]
  backend: string
}

export interface QueryResult {
  records: Record<string, unknown>[]
  stats: {
    nodes_created: number
    relationships_created: number
    properties_set: number
  }
}

export interface StatusResponse {
  status: string
  backend: string
  connected: boolean
  port: number
  repo: string
}

export const astApi = {
  getContexts: (projectDir?: string) => {
    const params = new URLSearchParams()
    if (projectDir) params.set('project_dir', projectDir)
    const qs = params.toString()
    return api.get<ContextsResponse>(`/api/contexts${qs ? `?${qs}` : ''}`)
  },
  getSchema: (context?: string, projectDir?: string) => {
    const qs = new URLSearchParams()
    if (context) qs.set('context', context)
    if (projectDir) qs.set('project_dir', projectDir)
    return api.get<SchemaResponse>(`/api/schema?${qs}`)
  },
  getGraph: (params: {
    context?: string
    cypher_query?: string
    repo_path?: string
    project_dir?: string
  }) => {
    const qs = new URLSearchParams()
    if (params.context) qs.set('context', params.context)
    if (params.cypher_query) qs.set('cypher_query', params.cypher_query)
    if (params.repo_path) qs.set('repo_path', params.repo_path)
    if (params.project_dir) qs.set('project_dir', params.project_dir)
    return api.get<GraphResponse>(`/api/graph?${qs}`)
  },
  getFile: (path: string, context?: string, projectDir?: string) => {
    const qs = new URLSearchParams({ path })
    if (context) qs.set('context', context)
    if (projectDir) qs.set('project_dir', projectDir)
    return api.get<{ content: string; source: string }>(`/api/file?${qs}`)
  },
  query: (cypher: string, context?: string, projectDir?: string) =>
    api.post<QueryResult>('/api/query', { cypher, context, project_dir: projectDir }),
  generateCypher: (prompt: string, context?: string, projectDir?: string) =>
    api.post<{ cypher: string; explanation?: string }>('/api/generate-cypher', {
      query: prompt,
      context,
      project_dir: projectDir,
    }),
  getStatus: () => api.get<StatusResponse>('/api/status'),
  deleteContext: (name: string) => api.delete<{ success: boolean }>(`/api/context/${name}`),
}
