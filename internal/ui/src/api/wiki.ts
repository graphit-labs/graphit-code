import { postAgentStream, type StreamOptions } from "./agentStream";


const API = () => window.__API_BASE__ ?? ''

export interface WikiModule {
  id: string; label: string; path: string; context: string; pages: number; hasLog: boolean
}
export interface WikiPageMeta {
  path: string; title: string; type: string; wordCount: number; links: string[]; tags: string[]
  confidence: number; source: string
}
export interface WikiPageContent extends WikiPageMeta { content: string }
export interface SearchResult { path: string; title: string; snippet: string; score: number }

export interface AISearchResult {
  path: string; title: string; relevance: string; score: number
}
export interface AISearchResponse {
  answer: string; results: AISearchResult[]; session_id?: string; error?: string
}

export interface WikiReadScope { projectId: string; context: string }

function readParams(dir: string, scope?: WikiReadScope) {
  const params = new URLSearchParams()
  if (dir) params.set('dir', dir)
  if (scope) {
    params.set('project_id', scope.projectId)
    params.set('context', scope.context)
  }
  return params
}

export async function fetchModules(projectDir?: string): Promise<WikiModule[]> {
  const params = new URLSearchParams()
  if (projectDir) params.set('project_dir', projectDir)
  const qs = params.toString()
  const r = await fetch(`${API()}/api/wiki/modules${qs ? `?${qs}` : ''}`)
  if (!r.ok) throw new Error(`Knowledge request failed (HTTP ${r.status})`)
  return r.json()
}
export async function fetchPages(dir: string, scope?: WikiReadScope): Promise<WikiPageMeta[]> {
  const r = await fetch(`${API()}/api/wiki/pages?${readParams(dir, scope).toString()}`)
  if (!r.ok) throw new Error(`Knowledge request failed (HTTP ${r.status})`)
  return r.json()
}
export async function fetchPage(dir: string, path: string, scope?: WikiReadScope): Promise<WikiPageContent> {
  const params = readParams(dir, scope); params.set('path', path)
  const r = await fetch(`${API()}/api/wiki/page?${params.toString()}`)
  if (!r.ok) throw new Error(`Knowledge request failed (HTTP ${r.status})`)
  return r.json()
}
export async function searchWiki(dir: string, q: string, scope?: WikiReadScope): Promise<SearchResult[]> {
  const params = readParams(dir, scope); params.set('q', q)
  const r = await fetch(`${API()}/api/wiki/search?${params.toString()}`)
  if (!r.ok) throw new Error(`Knowledge request failed (HTTP ${r.status})`)
  return r.json()
}
export async function aiSearchWiki(dir: string, query: string, projectDir?: string, options: StreamOptions = {}, scope?: WikiReadScope): Promise<AISearchResponse> {
  return postAgentStream<AISearchResponse>('/api/wiki/ai-search', {
    dir, query, project_dir: projectDir, project_id: scope?.projectId, context: scope?.context,
  }, options);
}
