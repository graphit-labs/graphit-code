import { useState, useEffect, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  fetchModules, fetchHubKnowledge, multiSearchWiki, multiKeywordSearchWiki,
  fetchSessions, deleteSession,
  type WikiModule, type WikiDirRef, type HubKnowRef, type HubKnowledgeItem,
  type SessionItem, type AISearchResponse, type MultiKeywordResult,
} from '@/api/wiki'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { useAppStore } from '@/store/appStore'
import { cn } from '@/lib/utils'
import {
  Search, BookOpen, Database, Plus, Clock, Trash2, X,
  ChevronDown, ChevronRight, Sparkles, MessageSquare, ArrowLeft,
  Check, Package, ExternalLink, Globe,
} from 'lucide-react'

function WikiSourcePicker({
  modules, hubItems,
  selected, hubSelected, hubVersions,
  onToggle, onHubToggle, onHubVersionChange,
}: {
  modules: WikiModule[]
  hubItems: HubKnowledgeItem[]
  selected: Set<string>
  hubSelected: Set<string>
  hubVersions: Record<string, string>
  onToggle: (id: string) => void
  onHubToggle: (id: string) => void
  onHubVersionChange: (id: string, version: string) => void
}) {
  const [showHub, setShowHub] = useState(false)
  const [hubFilter, setHubFilter] = useState('')

  const local = modules.filter(m => m.context === 'project' || m.context === 'user')
  const imported = modules.filter(m => m.context !== 'project' && m.context !== 'user')

  const filteredHub = hubItems.filter(h => {
    if (!hubFilter.trim()) return true
    const q = hubFilter.toLowerCase()
    return h.name.toLowerCase().includes(q)
      || h.id.toLowerCase().includes(q)
      || h.description.toLowerCase().includes(q)
  })

  return (
    <div className="space-y-4">
      {local.length > 0 && (
        <div>
          <h3 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 mb-2 flex items-center gap-1.5">
            <Database className="w-3 h-3" />
            Project Wikis
          </h3>
          <div className="flex flex-wrap gap-2">
            {local.map(m => (
              <button
                key={m.id}
                onClick={() => onToggle(m.id)}
                className={cn(
                  "flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border transition-all duration-200",
                  selected.has(m.id)
                    ? "bg-primary/10 border-primary/30 text-primary shadow-sm"
                    : "bg-card/60 border-border/40 text-muted-foreground hover:bg-accent/40 hover:text-foreground"
                )}
              >
                {selected.has(m.id) && <Check className="w-3 h-3" />}
                <BookOpen className="w-3 h-3" />
                {m.label}
                <span className="text-[10px] opacity-60">{m.pages} pages</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {imported.length > 0 && (
        <div>
          <h3 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 mb-2 flex items-center gap-1.5">
            <Globe className="w-3 h-3" />
            Ecosystem Wikis
          </h3>
          <div className="flex flex-wrap gap-2">
            {imported.map(m => (
              <button
                key={m.id}
                onClick={() => onToggle(m.id)}
                className={cn(
                  "flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-medium border transition-all duration-200",
                  selected.has(m.id)
                    ? "bg-blue-500/10 border-blue-500/30 text-blue-500 shadow-sm"
                    : "bg-card/60 border-border/40 text-muted-foreground hover:bg-accent/40 hover:text-foreground"
                )}
              >
                {selected.has(m.id) && <Check className="w-3 h-3" />}
                <ExternalLink className="w-3 h-3" />
                {m.label}
                <span className="text-[10px] opacity-60">{m.pages} pages</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {hubItems.length > 0 && (
        <div>
          <button
            onClick={() => setShowHub(v => !v)}
            className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 mb-2 flex items-center gap-1.5 hover:text-foreground transition-colors"
          >
            <Package className="w-3 h-3" />
            Hub Knowledge
            {showHub ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
            <span className="text-[10px] font-normal normal-case tracking-normal opacity-60 ml-1">
              {hubItems.length} available · auto-downloads if not cached
            </span>
          </button>

          {showHub && (
            <div className="space-y-3 mt-1">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground/40" />
                <input
                  type="text"
                  value={hubFilter}
                  onChange={e => setHubFilter(e.target.value)}
                  placeholder="Search hub knowledge artifacts…"
                  className="w-full pl-9 pr-3 py-2 rounded-lg border border-border/30 bg-background/60 text-xs focus:outline-none focus:ring-1 focus:ring-purple-500/30 focus:border-purple-500/40 transition-all placeholder:text-muted-foreground/40"
                />
                {hubFilter && (
                  <button
                    onClick={() => setHubFilter('')}
                    className="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/40 hover:text-foreground transition-colors"
                  >
                    <X className="w-3 h-3" />
                  </button>
                )}
              </div>

              <div className="grid gap-2 max-h-72 overflow-y-auto scrollbar-thin pr-1">
                {filteredHub.length === 0 ? (
                  <p className="text-xs text-muted-foreground/50 italic py-3 text-center">
                    No artifacts matching "{hubFilter}"
                  </p>
                ) : filteredHub.map(h => {
                  const isSelected = hubSelected.has(h.id)
                  const selectedVersion = hubVersions[h.id] || h.version

                  return (
                    <div
                      key={h.id}
                      className={cn(
                        "rounded-xl border p-3 transition-all duration-200",
                        isSelected
                          ? "bg-purple-500/5 border-purple-500/25 shadow-sm"
                          : "bg-card/40 border-border/30 hover:border-border/50"
                      )}
                    >
                      <div className="flex items-start gap-2.5">
                        <button
                          onClick={() => onHubToggle(h.id)}
                          className={cn(
                            "mt-0.5 w-4 h-4 rounded border flex items-center justify-center shrink-0 transition-all duration-200",
                            isSelected
                              ? "bg-purple-500 border-purple-500 text-white"
                              : "border-border/60 hover:border-purple-400"
                          )}
                        >
                          {isSelected && <Check className="w-2.5 h-2.5" />}
                        </button>

                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <Package className="w-3.5 h-3.5 text-purple-500/70 shrink-0" />
                            <span className="text-xs font-semibold text-foreground truncate">{h.name}</span>
                            {h.installed && (
                              <span className="text-[9px] font-bold text-emerald-500 bg-emerald-500/10 px-1.5 py-0.5 rounded">CACHED</span>
                            )}
                          </div>
                          {h.description && (
                            <p className="text-[11px] text-muted-foreground/60 mt-0.5 line-clamp-1">{h.description}</p>
                          )}
                          <p className="text-[10px] text-muted-foreground/40 font-mono mt-0.5">{h.id}</p>
                        </div>

                        <div className="shrink-0">
                          <div className="relative">
                            <select
                              value={selectedVersion}
                              onChange={e => onHubVersionChange(h.id, e.target.value)}
                              className={cn(
                                "appearance-none text-[11px] font-mono pl-2 pr-6 py-1 rounded-lg border bg-background cursor-pointer transition-all",
                                "focus:outline-none focus:ring-1 focus:ring-purple-500/30",
                                isSelected
                                  ? "border-purple-500/30 text-purple-600 dark:text-purple-400"
                                  : "border-border/30 text-muted-foreground"
                              )}
                            >
                              {(h.versions?.length > 0 ? h.versions : [h.version]).map(v => (
                                <option key={v} value={v}>
                                  v{v}{v === h.version ? ' (latest)' : ''}
                                </option>
                              ))}
                            </select>
                            <ChevronDown className="absolute right-1.5 top-1/2 -translate-y-1/2 w-3 h-3 text-muted-foreground/40 pointer-events-none" />
                          </div>
                        </div>
                      </div>
                    </div>
                  )
                })}
              </div>

              {hubSelected.size > 0 && (
                <p className="text-[10px] text-purple-500/70 font-medium flex items-center gap-1">
                  <Check className="w-3 h-3" />
                  {hubSelected.size} hub source{hubSelected.size !== 1 ? 's' : ''} selected
                  {Array.from(hubSelected).map(id => {
                    const ver = hubVersions[id] || hubItems.find(h => h.id === id)?.version
                    return <span key={id} className="font-mono opacity-70 ml-1">({id}@{ver})</span>
                  })}
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function SessionHistory({
  sessions, onResume, onDelete, onNew,
}: {
  sessions: SessionItem[]
  onResume: (s: SessionItem) => void
  onDelete: (id: string) => void
  onNew: () => void
}) {
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 flex items-center gap-1.5">
          <Clock className="w-3 h-3" />
          Recent Sessions
        </h3>
        <button
          onClick={onNew}
          className="flex items-center gap-1 text-xs text-primary hover:text-primary/80 font-medium transition-colors"
        >
          <Plus className="w-3 h-3" />
          New Search
        </button>
      </div>
      {sessions.length === 0 ? (
        <p className="text-xs text-muted-foreground/60 italic py-2">No previous sessions</p>
      ) : (
        <div className="space-y-1.5 max-h-48 overflow-y-auto scrollbar-thin">
          {sessions.slice(0, 8).map(s => (
            <div
              key={s.id}
              className="flex items-center gap-2.5 px-3 py-2.5 rounded-xl border border-border/30 bg-card/40 hover:bg-accent/30 transition-all cursor-pointer group"
              onClick={() => onResume(s)}
            >
              <MessageSquare className="w-3.5 h-3.5 text-muted-foreground/60 shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-xs font-medium text-foreground truncate">{s.title}</p>
                <p className="text-[10px] text-muted-foreground/60">
                  {s.message_count} msgs · {new Date(s.updated_at).toLocaleDateString()}
                </p>
              </div>
              <button
                onClick={(e) => { e.stopPropagation(); onDelete(s.id) }}
                className="opacity-0 group-hover:opacity-100 p-1 rounded-lg hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all"
              >
                <Trash2 className="w-3 h-3" />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

export default function WikiSearchPage() {
  const { activeProjectDir } = useAppStore()
  const navigate = useNavigate()

  const [modules, setModules] = useState<WikiModule[]>([])
  const [hubItems, setHubItems] = useState<HubKnowledgeItem[]>([])
  const [sessions, setSessions] = useState<SessionItem[]>([])
  const [loadingData, setLoadingData] = useState(true)

  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [hubSelected, setHubSelected] = useState<Set<string>>(new Set())
  const [hubVersions, setHubVersions] = useState<Record<string, string>>({})
  const [query, setQuery] = useState('')

  const [searchMode, setSearchMode] = useState<'ai' | 'keyword'>('ai')
  const [searching, setSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)

  useEffect(() => {
    setLoadingData(true)
    Promise.all([
      fetchModules(activeProjectDir || undefined).catch(() => []),
      fetchHubKnowledge().catch(() => []),
      fetchSessions(activeProjectDir || undefined).catch(() => []),
    ]).then(([mods, hub, sess]) => {
      setModules(mods ?? [])
      setHubItems(hub ?? [])
      setSessions(sess ?? [])

      const autoSelect = new Set<string>()
      for (const m of (mods ?? [])) {
        if (m.id === 'knowledge' || m.id === 'memory-project') {
          autoSelect.add(m.id)
        }
      }
      setSelected(autoSelect)
    }).finally(() => setLoadingData(false))
  }, [activeProjectDir])

  const handleToggle = (id: string) => {
    setSelected(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const handleHubToggle = (id: string) => {
    setHubSelected(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const handleHubVersionChange = (id: string, version: string) => {
    setHubVersions(prev => ({ ...prev, [id]: version }))
  }

  const handleSearch = useCallback(async () => {
    if (!query.trim() || (selected.size === 0 && hubSelected.size === 0)) return

    setSearching(true)
    setSearchError(null)

    const wikiDirs: WikiDirRef[] = modules
      .filter(m => selected.has(m.id))
      .map(m => ({ id: m.id, label: m.label, dir: m.path }))

    const hubRefs: HubKnowRef[] = hubItems
      .filter(h => hubSelected.has(h.id))
      .map(h => ({ id: h.id, version: hubVersions[h.id] || h.version }))

    try {
      if (searchMode === 'keyword') {
        const results = await multiKeywordSearchWiki(query, wikiDirs, hubRefs)
        
        navigate('/wiki/search/results', {
          state: {
            keywordResults: results,
            searchQuery: query,
          },
        })
      } else {
        const resp = await multiSearchWiki(query, wikiDirs, hubRefs)

        if (resp.error) {
          setSearchError(resp.error)
          setSearching(false)
          return
        }

        const wikiLinkRegex = /\[\[([^\]]+)\]\]/g
        const refs: Array<{ path: string; title: string; relevance: string; score: number }> = []
        const seen = new Set<string>()
        let match: RegExpExecArray | null
        while ((match = wikiLinkRegex.exec(resp.answer)) !== null) {
          const title = match[1]
          if (!seen.has(title)) {
            seen.add(title)
            refs.push({ path: title.replace(/\s+/g, '_') + '.md', title, relevance: 'Referenced in answer', score: 80 })
          }
        }

        if (resp.pages_consulted) {
          for (const p of resp.pages_consulted) {
            if (!seen.has(p)) {
              seen.add(p)
              refs.push({ path: p.replace(/\s+/g, '_') + '.md', title: p, relevance: 'Consulted during search', score: 70 })
            }
          }
        }

        const aiResponse: AISearchResponse = {
          answer: resp.answer,
          results: refs,
          session_id: resp.session_id,
        }

        navigate(`/wiki/search/results/${resp.session_id}`, {
          state: { aiResponse, searchQuery: query, sessionId: resp.session_id },
        })
      }
    } catch (e: any) {
      setSearchError(e.message)
      setSearching(false)
    }
  }, [query, selected, hubSelected, modules, hubItems, hubVersions, navigate, searchMode])

  const handleResumeSession = (s: SessionItem) => {
    navigate(`/wiki/search/results/${s.id}`)
  }

  const handleDeleteSession = async (id: string) => {
    await deleteSession(id)
    setSessions(prev => prev.filter(s => s.id !== id))
  }

  if (loadingData) {
    return (
      <div className="flex items-center justify-center h-full min-h-[400px]">
        <LoadingSpinner size="lg" label="Loading wiki sources…" />
      </div>
    )
  }

  return (
    <div className="w-full max-w-4xl mx-auto px-8 py-10 animate-in fade-in duration-300">
      {}
      <div className="text-center mb-10">
        <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-gradient-to-br from-primary/20 to-purple-500/20 border border-primary/20 mb-4">
          <Sparkles className="w-7 h-7 text-primary" />
        </div>
        <h1 className="text-3xl font-heading font-extrabold tracking-tight text-foreground">
          Wiki Search
        </h1>
        <p className="text-sm text-muted-foreground mt-2 max-w-md mx-auto">
          Search across multiple knowledge wikis. Select sources and choose AI or keyword mode.
        </p>
      </div>

      {}
      <div className="glass-panel rounded-2xl p-6 space-y-6">
        {}
        <WikiSourcePicker
          modules={modules}
          hubItems={hubItems}
          selected={selected}
          hubSelected={hubSelected}
          hubVersions={hubVersions}
          onToggle={handleToggle}
          onHubToggle={handleHubToggle}
          onHubVersionChange={handleHubVersionChange}
        />

        {}
        <div className="border-t border-border/30" />

        {}
        <div className="flex items-center gap-1 bg-accent/25 border border-border/30 rounded-xl p-1 w-fit">
          <button
            onClick={() => setSearchMode('ai')}
            className={cn(
              'flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-semibold transition-all',
              searchMode === 'ai'
                ? 'bg-background shadow-sm text-foreground font-bold border border-border/20 scale-[1.02]'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Sparkles className="w-3.5 h-3.5 text-primary" /> AI Search
          </button>
          <button
            onClick={() => setSearchMode('keyword')}
            className={cn(
              'flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-semibold transition-all',
              searchMode === 'keyword'
                ? 'bg-background shadow-sm text-foreground font-bold border border-border/20 scale-[1.02]'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Search className="w-3.5 h-3.5" /> Keyword Search
          </button>
        </div>

        {}
        <div className="flex items-end gap-3">
          <div className="flex-1 relative">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground/50" />
            <input
              type="text"
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') handleSearch() }}
              placeholder={searchMode === 'ai'
                ? 'Ask a question across your wikis…'
                : 'Type keywords to search across wikis…'}
              className="w-full pl-11 pr-4 py-3.5 rounded-xl border border-border/40 bg-background text-sm focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary/50 transition-all placeholder:text-muted-foreground/50"
              autoFocus
            />
          </div>
          <button
            onClick={handleSearch}
            disabled={searching || !query.trim() || (selected.size === 0 && hubSelected.size === 0)}
            className={cn(
              "flex items-center gap-2 px-5 py-3.5 rounded-xl text-sm font-semibold transition-all duration-200",
              query.trim() && (selected.size > 0 || hubSelected.size > 0) && !searching
                ? "btn-premium"
                : "bg-accent text-muted-foreground cursor-not-allowed"
            )}
          >
            {searching
              ? <LoadingSpinner size="sm" />
              : searchMode === 'ai'
                ? <Sparkles className="w-4 h-4" />
                : <Search className="w-4 h-4" />}
            {searchMode === 'ai' ? 'AI Search' : 'Search'}
          </button>
        </div>

        {}
        {(selected.size > 0 || hubSelected.size > 0) && (
          <p className="text-[11px] text-muted-foreground/60 flex items-center gap-1.5">
            <Check className="w-3 h-3 text-emerald-500" />
            {selected.size + hubSelected.size} source{(selected.size + hubSelected.size) !== 1 ? 's' : ''} selected
          </p>
        )}

        {}
        {searchError && (
          <div className="bg-destructive/10 border border-destructive/20 rounded-2xl p-4">
            <p className="text-sm font-medium text-destructive mb-1">Search Error</p>
            <pre className="text-xs text-foreground/80 whitespace-pre-wrap">{searchError}</pre>
            <button
              onClick={() => setSearchError(null)}
              className="mt-2 text-xs text-muted-foreground hover:text-foreground transition-colors flex items-center gap-1"
            >
              <ArrowLeft className="w-3 h-3" /> Dismiss
            </button>
          </div>
        )}
      </div>

      {}
      {sessions.length > 0 && (
        <div className="mt-8 glass-panel rounded-2xl p-6">
          <SessionHistory
            sessions={sessions}
            onResume={handleResumeSession}
            onDelete={handleDeleteSession}
            onNew={() => { setQuery(''); setSearchError(null) }}
          />
        </div>
      )}
    </div>
  )
}
