/**
 * Wiki page browsing.
 *
 * The live search moved to ./search.ts, along with its sessions and follow-up chat:
 * it spans wikis and code graphs, so it was never a wiki-only concern and its
 * endpoints are no longer under /api/wiki.
 */

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

export async function fetchModules(projectDir?: string): Promise<WikiModule[]> {
  const params = new URLSearchParams()
  if (projectDir) params.set('project_dir', projectDir)
  const qs = params.toString()
  const r = await fetch(`${API()}/api/wiki/modules${qs ? `?${qs}` : ''}`)
  return r.json()
}
export async function fetchPages(dir: string): Promise<WikiPageMeta[]> {
  const r = await fetch(`${API()}/api/wiki/pages?dir=${encodeURIComponent(dir)}`)
  return r.json()
}
export async function fetchPage(dir: string, path: string): Promise<WikiPageContent> {
  const r = await fetch(`${API()}/api/wiki/page?dir=${encodeURIComponent(dir)}&path=${encodeURIComponent(path)}`)
  return r.json()
}
export async function searchWiki(dir: string, q: string): Promise<SearchResult[]> {
  const r = await fetch(`${API()}/api/wiki/search?dir=${encodeURIComponent(dir)}&q=${encodeURIComponent(q)}`)
  return r.json()
}
export async function aiSearchWiki(dir: string, query: string): Promise<AISearchResponse> {
  const r = await fetch(`${API()}/api/wiki/ai-search`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ dir, query }),
  })
  return r.json()
}
