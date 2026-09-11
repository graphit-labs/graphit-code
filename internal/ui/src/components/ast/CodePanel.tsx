import { useEffect, useRef, useState, useCallback } from 'react'
import hljs from 'highlight.js'
import { Copy, Search, X, ChevronUp, ChevronDown } from 'lucide-react'
import { showToast } from '@/hooks/useToast'
import { cn } from '@/lib/utils'

interface CodePanelProps {
  content: string
  filename: string

  highlightLine?: number | null
  onClose?: () => void
}

export function CodePanel({ content, filename, highlightLine, onClose }: CodePanelProps) {
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchTerm, setSearchTerm] = useState('')
  const [matchCount, setMatchCount] = useState(0)
  const [currentMatch, setCurrentMatch] = useState(0)
  const bodyRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const targetRef = useRef<HTMLDivElement>(null)

  const lang = (() => {
    const ext = filename.split('.').pop()?.toLowerCase() ?? ''
    const map: Record<string, string> = {
      ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript',
      go: 'go', py: 'python', java: 'java', sql: 'sql', xml: 'xml',
      json: 'json', yaml: 'yaml', yml: 'yaml', sh: 'bash', md: 'markdown',
      plsql: 'sql', pls: 'sql', pck: 'sql', pkb: 'sql', pks: 'sql',
    }
    return map[ext] ?? 'plaintext'
  })()

  const lines = content.split('\n')

  const highlightedLines = useCallback(() => {
    return lines.map((line) => {
      try {
        return hljs.highlight(line, { language: lang, ignoreIllegals: true }).value
      } catch {
        return hljs.highlightAuto(line).value
      }
    })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [content, lang])

  const copy = () => {
    navigator.clipboard.writeText(content).then(() => showToast('Copied to clipboard!', 'success'))
  }

  const toggleSearch = () => {
    setSearchOpen((v) => {
      if (!v) setTimeout(() => searchRef.current?.focus(), 50)
      return !v
    })
    if (searchOpen) {
      setSearchTerm('')
      setMatchCount(0)
      setCurrentMatch(0)
    }
  }

  useEffect(() => {
    if (!bodyRef.current || !searchTerm) {
      setMatchCount(0)
      setCurrentMatch(0)

      bodyRef.current?.querySelectorAll('.search-match, .search-match-active').forEach((el) => {
        el.classList.remove('search-match', 'search-match-active')
      })
      return
    }

    const codeLines = bodyRef.current.querySelectorAll<HTMLElement>('.code-line')
    let count = 0
    const matchLines: HTMLElement[] = []

    codeLines.forEach((line) => {
      if (line.textContent?.toLowerCase().includes(searchTerm.toLowerCase())) {
        line.classList.add('search-match')
        matchLines.push(line)
        count++
      } else {
        line.classList.remove('search-match', 'search-match-active')
      }
    })

    setMatchCount(count)
    setCurrentMatch(count > 0 ? 1 : 0)

    if (matchLines[0]) {
      matchLines.forEach((l) => l.classList.remove('search-match-active'))
      matchLines[0].classList.add('search-match-active')
      matchLines[0].scrollIntoView({ behavior: 'smooth', block: 'center' })
    }
  }, [searchTerm])

  useEffect(() => {
    if (!highlightLine || !content) return
    targetRef.current?.scrollIntoView({ block: 'center' })
  }, [highlightLine, content])

  const navigate = (dir: 1 | -1) => {
    if (!bodyRef.current || matchCount === 0) return
    const matchLines = Array.from(bodyRef.current.querySelectorAll<HTMLElement>('.search-match'))
    matchLines.forEach((l) => l.classList.remove('search-match-active'))
    const next = ((currentMatch - 1 + dir + matchCount) % matchCount)
    setCurrentMatch(next + 1)
    matchLines[next].classList.add('search-match-active')
    matchLines[next].scrollIntoView({ behavior: 'smooth', block: 'center' })
  }

  const hl = highlightedLines()

  return (
    <div className="flex flex-col h-full bg-card/60 backdrop-blur-md rounded-2xl border border-border/40 overflow-hidden shadow-sm">
      {}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border/40 bg-accent/20 shrink-0">
        <p className="text-xs font-mono font-semibold text-muted-foreground truncate flex-1 mr-2" title={filename}>
          {filename.split('/').slice(-1)[0]}
        </p>
        <div className="flex items-center gap-1">
          <button
            onClick={toggleSearch}
            className="p-1.5 rounded-lg hover:bg-accent border border-transparent hover:border-border/30 text-muted-foreground hover:text-foreground transition-all"
            title="Search"
          >
            <Search className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={copy}
            className="p-1.5 rounded-lg hover:bg-accent border border-transparent hover:border-border/30 text-muted-foreground hover:text-foreground transition-all"
            title="Copy Code"
          >
            <Copy className="w-3.5 h-3.5" />
          </button>
          {onClose && (
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-destructive/10 border border-transparent hover:border-destructive/20 text-muted-foreground hover:text-destructive transition-all"
              title="Close Panel"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {}
      {searchOpen && (
        <div className="flex items-center gap-2.5 px-4 py-2 border-b border-border/40 bg-accent/10 animate-in fade-in slide-in-from-top-1 duration-150">
          <input
            ref={searchRef}
            type="text"
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') navigate(e.shiftKey ? -1 : 1)
              if (e.key === 'Escape') toggleSearch()
            }}
            placeholder="Find in file..."
            className="flex-1 text-xs bg-transparent outline-none text-foreground placeholder-muted-foreground/60 font-medium"
          />
          <span className="text-[10px] font-mono font-bold text-muted-foreground/75 bg-accent/25 px-1.5 py-0.5 rounded border border-border/20 shrink-0">
            {matchCount > 0 ? `${currentMatch}/${matchCount}` : '0/0'}
          </span>
          <div className="flex items-center gap-0.5 border-l border-border/40 pl-2">
            <button
              onClick={() => navigate(-1)}
              disabled={matchCount === 0}
              className="p-1 rounded hover:bg-accent disabled:opacity-40 text-muted-foreground hover:text-foreground transition-colors"
            >
              <ChevronUp className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => navigate(1)}
              disabled={matchCount === 0}
              className="p-1 rounded hover:bg-accent disabled:opacity-40 text-muted-foreground hover:text-foreground transition-colors"
            >
              <ChevronDown className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={toggleSearch}
              className="p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors ml-1"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      )}

      {}
      <style>{`
        .code-line { display: flex; }
        .search-match { background: rgba(234,179,8,0.12); }
        .search-match-active { background: rgba(234,179,8,0.28); }
        .target-line { background: color-mix(in srgb, var(--primary, #3b82f6) 14%, transparent); box-shadow: inset 3px 0 0 var(--primary, #3b82f6); }
        .target-line .code-gutter { color: var(--primary, #3b82f6); font-weight: 700; }

        .hljs-keyword { color: var(--primary, #3b82f6); font-weight: 600; }
        .hljs-string { color: #10b981; }
        .hljs-comment { color: #888888; font-style: italic; }
        .hljs-number { color: #f59e0b; }
        .hljs-type { color: #06b6d4; font-weight: 500; }
        .hljs-title { color: #8b5cf6; }
        .hljs-built_in { color: #d946ef; }
        .hljs-attr { color: #f43f5e; }
        .hljs-tag { color: #3b82f6; }
        .hljs-meta { color: #6b7280; }
      `}</style>
      <div
        ref={bodyRef}
        className="flex-1 overflow-auto font-mono text-[12px] leading-6 py-3"
        id="codeBody"
        style={{ overflowX: 'auto', overflowY: 'auto' }}
      >
        {hl.map((lineHtml, i) => {
          const isTarget = highlightLine === i + 1
          return (
          <div
            key={i}
            ref={isTarget ? targetRef : undefined}
            className={cn('code-line hover:bg-accent/25 transition-colors group', isTarget && 'target-line')}
          >
            <span className="code-gutter select-none w-12 shrink-0 text-right pr-4 text-muted-foreground/35 group-hover:text-muted-foreground/70 border-r border-border/30 mr-4 text-[10px] font-mono">
              {i + 1}
            </span>
            <span
              className="flex-1 pr-4 whitespace-pre text-foreground/90 font-medium"
              dangerouslySetInnerHTML={{ __html: lineHtml || ' ' }}
            />
          </div>
          )
        })}
      </div>
    </div>
  )
}
