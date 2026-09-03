import { useState, useRef, useEffect } from 'react'
import { astApi } from '@/api/ast'
import { showToast } from '@/hooks/useToast'
import { cn, agentFeaturesEnabled } from '@/lib/utils'
import { Code2, Loader2, Send, ChevronUp, ChevronDown, Sparkles } from 'lucide-react'

type Mode = 'cypher' | 'nl'

interface QueryBarProps {
  contextId?: string
  projectDir?: string
  onQueryResult: (result: unknown) => void
  loading: boolean
  setLoading: (v: boolean) => void
  collapsed?: boolean
  onCollapsedClick?: () => void
}

const EXAMPLE_QUERIES = [
  'MATCH (n) RETURN n LIMIT 50',
  'MATCH (n:Function) RETURN n LIMIT 30',
  'MATCH (n)-[r]->(m) RETURN n,r,m LIMIT 40',
  'MATCH (n:File) RETURN n LIMIT 20',
  "MATCH (n:Class {cluster: 'my-cluster'}) RETURN n.name, n.path",
  "MATCH (n:Function {cluster: 'backend'}) RETURN n.name LIMIT 20",
]

export function QueryBar({ contextId, projectDir, onQueryResult, loading, setLoading, collapsed, onCollapsedClick: _onCollapsedClick }: QueryBarProps) {

  const aiEnabled = agentFeaturesEnabled()
  const [mode, setMode] = useState<Mode>('cypher')
  const [query, setQuery] = useState('')
  const [generating, setGenerating] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!collapsed) {
      setTimeout(() => textareaRef.current?.focus(), 50)
    }
  }, [collapsed])

  const handleExecute = async () => {
    const q = query.trim()
    if (!q) return
    setLoading(true)
    try {
      const result = await astApi.getGraph({ context: contextId, cypher_query: q, project_dir: projectDir })
      onQueryResult(result)
    } catch (e: unknown) {
      showToast(`Query failed: ${(e as Error).message}`, 'error')
    } finally {
      setLoading(false)
    }
  }

  const handleGenerate = async () => {
    const prompt = query.trim()
    if (!prompt || mode !== 'nl' || !aiEnabled) return
    setGenerating(true)
    try {
      const result = await astApi.generateCypher(prompt, contextId, projectDir)
      if (result.cypher) {
        setQuery(result.cypher)
        setMode('cypher')
        setLoading(true)
        const graphResult = await astApi.getGraph({ context: contextId, cypher_query: result.cypher, project_dir: projectDir })
        onQueryResult(graphResult)
        setLoading(false)
      }
    } catch {
      showToast('AI generation failed. Check AI config with: graphit config --global ai.provider <provider>', 'error')
    } finally {
      setGenerating(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault()
      if (mode === 'nl') handleGenerate()
      else handleExecute()
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {}
      <div className="flex items-center gap-1 bg-accent/40 border border-border/40 rounded-xl p-1 w-fit backdrop-blur-md">
        <button
          onClick={() => setMode('cypher')}
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-200',
            mode === 'cypher'
              ? 'bg-card text-foreground shadow-[0_2px_8px_rgba(0,0,0,0.06)] border border-border/40'
              : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Code2 className="w-3.5 h-3.5" />
          Cypher
        </button>
        {aiEnabled && (
          <button
            onClick={() => setMode('nl')}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-200',
              mode === 'nl'
                ? 'bg-card text-primary shadow-[0_2px_8px_rgba(0,0,0,0.06)] border border-border/40'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            <Sparkles className="w-3.5 h-3.5 text-primary" />
            AI Assistant
          </button>
        )}
      </div>

      {}
      <div className="relative group">
        <textarea
          ref={textareaRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={
            mode === 'cypher'
              ? "MATCH (n:Function {cluster: 'my-cluster'}) RETURN n.name LIMIT 50"
              : 'Describe what to explore... e.g. "Show all functions that query the database"'
          }
          rows={mode === 'cypher' ? 2 : 3}
          className={cn(
            'w-full px-4 py-3 pr-24 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm font-mono transition-all duration-200',
            'outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 resize-y',
            mode === 'nl' && 'font-sans',
          )}
        />
        <div className="absolute right-3 bottom-3 flex items-center gap-2">
          {mode === 'nl' && (
            <button
              onClick={handleGenerate}
              disabled={generating || !query.trim()}
              title="Generate with AI (Ctrl+Enter)"
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-semibold hover:bg-primary/95 disabled:opacity-40 transition-all shadow-sm"
            >
              {generating ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Sparkles className="w-3.5 h-3.5" />}
              Generate
            </button>
          )}
          <button
            onClick={mode === 'nl' ? handleGenerate : handleExecute}
            disabled={loading || generating || !query.trim()}
            title={mode === 'cypher' ? 'Execute (Ctrl+Enter)' : 'Generate & Execute'}
            className={cn(
              "flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition-all shadow-sm",
              mode === 'cypher'
                ? "bg-foreground text-background hover:bg-foreground/90"
                : "bg-primary text-primary-foreground hover:bg-primary/90"
            )}
          >
            {loading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
            {mode === 'cypher' ? 'Run' : 'Ask'}
          </button>
        </div>
      </div>

      {}
      {mode === 'cypher' && (
        <div className="flex items-center gap-2 flex-wrap pt-1">
          <span className="text-[10px] uppercase font-bold tracking-wider text-muted-foreground/60">Examples:</span>
          {EXAMPLE_QUERIES.map((eq) => (
            <button
              key={eq}
              onClick={() => { setQuery(eq); setMode('cypher'); textareaRef.current?.focus() }}
              className="text-[10px] px-2.5 py-1 rounded-lg border border-border/30 bg-muted/40 hover:bg-accent hover:border-primary/20 text-muted-foreground hover:text-foreground transition-all duration-150 font-mono"
            >
              {eq.length > 32 ? eq.slice(0, 30) + '…' : eq}
            </button>
          ))}
        </div>
      )}
      {mode === 'nl' && (
        <p className="text-[11px] text-muted-foreground/80 flex items-center gap-1">
          <span>Press</span>
          <kbd className="px-1.5 py-0.5 rounded border border-border/50 bg-muted text-[10px] font-mono font-semibold shadow-sm">Ctrl + Enter</kbd>
          <span>to generate and execute your query automatically.</span>
        </p>
      )}
    </div>
  )
}

export function QueryBarCollapsed({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="flex items-center gap-3 w-full px-5 py-3.5 text-xs text-muted-foreground hover:text-foreground bg-accent/25 hover:bg-accent/40 border border-border/30 rounded-xl transition-all duration-200 text-left shadow-sm group"
      title="Click to expand query bar"
    >
      <Code2 className="w-4 h-4 shrink-0 text-muted-foreground/80 group-hover:scale-105 transition-transform" />
      <span className="flex-1 font-medium">Click or search the codebase graph with Cypher or AI...</span>
      <ChevronDown className="w-4 h-4 shrink-0 text-muted-foreground/60 group-hover:translate-y-0.5 transition-transform" />
    </button>
  )
}

export function QueryBarCollapseButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="p-2 rounded-xl hover:bg-accent border border-transparent hover:border-border/30 text-muted-foreground hover:text-foreground shrink-0 transition-all duration-150"
      title="Minimize query bar"
    >
      <ChevronUp className="w-3.5 h-3.5" />
    </button>
  )
}
