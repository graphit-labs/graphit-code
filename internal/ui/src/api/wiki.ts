const API = () => (window as any).__API_BASE__ ?? ''

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

export interface WikiDirRef {
  id: string; label: string; dir: string
}
export interface HubKnowRef {
  id: string; version: string
}
export interface MultiSearchResponse {
  answer: string; session_id: string; turns: number; tokens: number
  pages_consulted?: string[]; error?: string
}
export interface ChatResponse {
  answer: string; session_id: string; error?: string
}
export interface SessionItem {
  id: string; title: string; created_at: string; updated_at: string
  message_count: number; wiki_sources: WikiDirRef[]
}
export interface HubKnowledgeItem {
  id: string; name: string; description: string; version: string
  versions: string[]; installed: boolean
}

export async function multiSearchWiki(
  query: string, wikiDirs: WikiDirRef[], hubRefs: HubKnowRef[]
): Promise<MultiSearchResponse> {
  const r = await fetch(`${API()}/api/wiki/multi-search`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, wiki_dirs: wikiDirs, hub_refs: hubRefs }),
  })
  return r.json()
}

export async function chatWiki(sessionId: string, message: string): Promise<ChatResponse> {
  const r = await fetch(`${API()}/api/wiki/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, message }),
  })
  return r.json()
}

export async function fetchSessions(projectDir?: string): Promise<SessionItem[]> {
  const params = new URLSearchParams()
  if (projectDir) params.set('project_dir', projectDir)
  const qs = params.toString()
  const r = await fetch(`${API()}/api/wiki/sessions${qs ? `?${qs}` : ''}`)
  return r.json()
}

export async function deleteSession(id: string): Promise<void> {
  await fetch(`${API()}/api/wiki/sessions?id=${encodeURIComponent(id)}`, { method: 'DELETE' })
}

export async function fetchHubKnowledge(): Promise<HubKnowledgeItem[]> {
  const r = await fetch(`${API()}/api/wiki/hub-knowledge`)
  return r.json()
}

export interface MultiKeywordResult {
  source_id: string; source_label: string
  path: string; title: string; snippet: string; score: number
}

export async function multiKeywordSearchWiki(
  query: string, wikiDirs: WikiDirRef[], hubRefs: HubKnowRef[]
): Promise<MultiKeywordResult[]> {
  const r = await fetch(`${API()}/api/wiki/multi-keyword-search`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, wiki_dirs: wikiDirs, hub_refs: hubRefs }),
  })
  return r.json()
}

export interface ChatMessage {
  role: string; content: string; timestamp: string; tokens?: number
}

export async function loadSessionMessages(sessionId: string): Promise<ChatMessage[]> {
  const r = await fetch(`${API()}/api/wiki/sessions/messages?id=${encodeURIComponent(sessionId)}`)
  return r.json()
}
