import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react'
import { useParams, useNavigate, useLocation } from 'react-router-dom'
import {
  fetchModules, fetchPages, fetchPage, searchWiki, aiSearchWiki, chatWiki, loadSessionMessages,
  WikiModule, WikiPageMeta, WikiPageContent, SearchResult, AISearchResponse, MultiKeywordResult,
} from '@/api/wiki'
import {
  BookOpen, FileText, BarChart3, Hash, Search, ChevronRight, ChevronLeft,
  RefreshCw, ExternalLink, Clock, Layers, Users, Zap, GitBranch, ArrowLeft, ArrowRight,
  Wand2, Loader2, Send, X, Copy, Check
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

const LS = {
  get<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback } catch { return fallback }
  },
  set(key: string, value: unknown) {
    try { localStorage.setItem(key, JSON.stringify(value)) } catch { /* storage full */ }
  },
}

function useResizable(
  initial: number,
  min: number,
  max: number,
  direction: 'right' | 'left' = 'right',
  storageKey?: string,
) {
  const [size, setSize] = useState(() =>
    storageKey ? LS.get<number>(storageKey, initial) : initial
  )
  const dragging = useRef(false)
  const startX = useRef(0)
  const startSize = useRef(size)

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true
    startX.current = e.clientX
    startSize.current = size
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return
      const delta = direction === 'right' ? ev.clientX - startX.current : startX.current - ev.clientX
      const next = Math.max(min, Math.min(max, startSize.current + delta))
      setSize(next)
    }
    const onUp = () => {
      dragging.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      if (storageKey) LS.set(storageKey, sizeRef.current)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }, [size, min, max, direction, storageKey])

  const sizeRef = useRef(size)
  // eslint-disable-next-line react-hooks/immutability
  useEffect(() => { sizeRef.current = size }, [size])

  return { size, onMouseDown }
}

import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneLight, oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { useTheme } from '@/hooks/useTheme'

function wikiLinkFriendlyName(raw: string): string {
  return raw
    .replace(/\.md$/, '')
    .replace(/^ADR-_/, 'ADR: ')
    .replace(/^ADR:_/, 'ADR: ')
    .replace(/^community-/, '')
    .replace(/^god-node-/, '')
    .replace(/_/g, ' ')
    .replace(/—/g, ' — ')
    .replace(/\s+/g, ' ')
    .trim()
}

function plantumlEncode(text: string): string {
  const hex = Array.from(new TextEncoder().encode(text))
    .map(b => b.toString(16).padStart(2, '0')).join('')
  return '~h' + hex
}

function preprocessWikiLinks(md: string): string {
  let processed = md
  
  processed = processed.replace(/`\[([^\]]+)\]\/\[\[([^\]]+)\]\]`/g, (_, module, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(module)}/${encodeURIComponent(target)})`
  })

  processed = processed.replace(/\[([^\]]+)\]\/\[\[([^\]]+)\]\]/g, (_, module, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(module)}/${encodeURIComponent(target)})`
  })

  processed = processed.replace(/`\[\[([^\]]+)\]\]`/g, (_, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(target)})`
  })

  processed = processed.replace(/\[\[(.+?)\]\]/g, (_, target) => {
    const friendly = wikiLinkFriendlyName(target)
    return `[${friendly}](wiki://${encodeURIComponent(target)})`
  })

  processed = processed.replace(/`(\[[^\]]+\]\(wiki:\/\/[^)]+\))`/g, '$1')

  return processed
}

function CodeCopyButton({ code }: { code: string }) {
  const [copied, setCopied] = useState(false)
  const handleCopy = () => {
    navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }
  return (
    <button
      onClick={handleCopy}
      className="p-1 rounded bg-muted/60 hover:bg-accent/40 text-muted-foreground hover:text-foreground transition-all flex items-center gap-1 text-[10px] font-semibold"
    >
      {copied ? (
        <>
          <Check className="w-3.5 h-3.5 text-emerald-500" />
          Copied!
        </>
      ) : (
        <>
          <Copy className="w-3.5 h-3.5" />
          Copy
        </>
      )}
    </button>
  )
}

function WikiMarkdown({ content, onLink }: { content: string; onLink: (page: string) => void }) {
  const processed = useMemo(() => preprocessWikiLinks(content), [content])
  const { theme } = useTheme()

  return (
    <div className="wiki-prose prose prose-sm dark:prose-invert max-w-none">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={(url) => url}
        components={{
          h1: ({ children }) => (
            <h1 className="text-2xl font-heading font-extrabold tracking-tight text-foreground mt-8 mb-4 pb-2 border-b border-border/40 leading-snug">
              {children}
            </h1>
          ),
          h2: ({ children }) => (
            <h2 className="text-xl font-heading font-bold tracking-tight text-foreground mt-7 mb-3.5 leading-snug">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="text-lg font-heading font-semibold tracking-tight text-foreground mt-6 mb-3 leading-snug">
              {children}
            </h3>
          ),
          h4: ({ children }) => (
            <h4 className="text-base font-heading font-medium tracking-tight text-foreground mt-5 mb-2 leading-snug">
              {children}
            </h4>
          ),
          p: ({ children }) => (
            <p className="text-[14px] text-foreground/80 leading-relaxed mb-4">
              {children}
            </p>
          ),
          a: ({ href, children }) => {
            if (href) {
              const isExternal = /^https?:\/\//i.test(href) || href.startsWith('mailto:') || href.startsWith('tel:')
              if (href.startsWith('wiki://') || !isExternal) {
                const target = href.startsWith('wiki://')
                  ? decodeURIComponent(href.slice(7))
                  : decodeURIComponent(href)
                return (
                  <button
                    onClick={() => onLink(target)}
                    className="text-primary hover:text-primary/80 hover:underline underline-offset-2 font-semibold transition-colors"
                  >
                    {children}
                  </button>
                )
              }
            }
            return (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline inline-flex items-center gap-0.5"
              >
                {children}
                <ExternalLink className="w-3 h-3 inline shrink-0 opacity-60" />
              </a>
            )
          },
          code: ({ children }) => {
            return (
              <code className="bg-accent/40 text-primary border border-border/20 px-1.5 py-0.5 rounded-md text-[12px] font-mono">
                {children}
              </code>
            )
          },
          pre: ({ children }) => {
            const codeEl = (children as any)?.props ?? {}
            const className = codeEl.className || ''
            const match = /language-(\w+)/.exec(className)
            const lang = match?.[1] ?? ''
            const code = String(codeEl.children ?? '').replace(/\n$/, '')

            if (lang === 'plantuml' || lang === 'puml') {
              const encoded = plantumlEncode(code)
              const src = `https://www.plantuml.com/plantuml/svg/${encoded}`
              return (
                <div className="my-6 flex justify-center not-prose">
                  <img
                    src={src}
                    alt="PlantUML diagram"
                    className="max-w-full rounded-2xl border border-border/40 shadow-sm bg-white p-3 cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                    onClick={() => document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
                  />
                </div>
              )
            }

            if (lang === 'mermaid') {
              const encoded = btoa(unescape(encodeURIComponent(code)))
              const src = `https://mermaid.ink/svg/${encoded}`
              return (
                <div className="my-6 flex justify-center not-prose">
                  <img
                    src={src}
                    alt="Mermaid diagram"
                    className="max-w-full rounded-2xl border border-border/40 shadow-sm bg-white p-3 cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                    onClick={() => document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
                  />
                </div>
              )
            }

            return (
              <div className="my-6 rounded-2xl overflow-hidden border border-border/40 shadow-sm bg-card/40 backdrop-blur-sm group relative not-prose">
                <div className="bg-accent/35 px-4 py-2 text-[10px] text-muted-foreground font-mono border-b border-border/30 flex items-center justify-between">
                  <span className="uppercase tracking-widest font-bold text-muted-foreground/75">{lang || 'text'}</span>
                  <CodeCopyButton code={code} />
                </div>
                {lang ? (
                  <SyntaxHighlighter
                    language={lang}
                    style={theme === 'dark' ? oneDark : oneLight}
                    customStyle={{
                      margin: 0,
                      padding: '1.25rem',
                      background: 'transparent',
                      fontSize: '13px',
                      lineHeight: '1.6',
                      fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace'
                    }}
                  >
                    {code}
                  </SyntaxHighlighter>
                ) : (
                  <pre className="p-4 text-[13px] font-mono overflow-x-auto text-foreground/80 leading-relaxed whitespace-pre bg-transparent">
                    <code>{code}</code>
                  </pre>
                )}
              </div>
            )
          },
          table: ({ children }) => (
            <div className="my-6 overflow-x-auto rounded-2xl border border-border/40 shadow-sm bg-card/25 backdrop-blur-sm">
              <table className="w-full text-xs text-left border-collapse">{children}</table>
            </div>
          ),
          thead: ({ children }) => <thead className="border-b border-border/40 bg-accent/20">{children}</thead>,
          tbody: ({ children }) => <tbody className="divide-y divide-border/20">{children}</tbody>,
          tr: ({ children }) => <tr className="hover:bg-accent/15 transition-colors">{children}</tr>,
          th: ({ children }) => (
            <th className="px-4 py-3 text-xs font-bold text-muted-foreground uppercase tracking-wider">
              {children}
            </th>
          ),
          td: ({ children }) => <td className="px-4 py-2.5 text-foreground/85 font-medium">{children}</td>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-4 border-primary pl-4 py-2.5 my-5 text-[14px] text-muted-foreground/90 italic bg-primary/5 rounded-r-xl border-dashed">
              {children}
            </blockquote>
          ),
          ul: ({ children }) => <ul className="pl-5 space-y-1 mb-4 text-[14px] text-foreground/80 list-disc">{children}</ul>,
          ol: ({ children }) => <ol className="pl-5 space-y-1 mb-4 text-[14px] text-foreground/80 list-decimal">{children}</ol>,
          li: ({ children }) => <li className="leading-relaxed pl-0.5">{children}</li>,
          hr: () => <hr className="border-border/40 my-6" />,
          img: ({ src, alt }) => (
            <div className="my-6 flex justify-center">
              <img
                src={src}
                alt={alt || ''}
                className="max-w-full rounded-2xl border border-border/40 shadow-sm cursor-zoom-in hover:shadow-md transition-all hover:scale-[1.005]"
                onClick={() => src && document.dispatchEvent(new CustomEvent('wiki-lightbox', { detail: src }))}
              />
            </div>
          ),
          strong: ({ children }) => <strong className="font-bold text-foreground">{children}</strong>,
          em: ({ children }) => <em className="italic text-foreground/90">{children}</em>,
          del: ({ children }) => <del className="line-through text-muted-foreground/60">{children}</del>,
        }}
      >
        {processed}
      </ReactMarkdown>
    </div>
  )
}

function preprocessContent(raw: string, title: string, type?: string): string {
  const lines = raw.split('\n')
  let i = 0
  if (lines[0]?.trim() === '---') {
    i++
    while (i < lines.length && lines[i].trim() !== '---') i++
    i++
  }

  const hasContentHeader = lines.some(line => line.trim() === '## Content')
  const isSpecialPage = type === 'log' || type === 'index' || type === 'community' || type === 'god-node'

  if (isSpecialPage || !hasContentHeader) {
    while (i < lines.length && lines[i].trim() === '') i++
    const out: string[] = []
    for (; i < lines.length; i++) {
      const line = lines[i].trim()
      if (line === '---' && i + 1 < lines.length && lines[i + 1].trim().startsWith('*Navigate:')) break
      if (line.startsWith('*Navigate:') && line.endsWith('*')) continue
      out.push(lines[i])
    }
    return out.join('\n')
  }

  const preambleRe = /^(#{1,2}\s|>\s|\*\*Source:\*\*|\*\*Type:\*\*|\*\*Confidence:\*\*|\*Provenance:|\*Navigate:|---$|$)/
  let foundContent = false
  while (i < lines.length) {
    const line = lines[i].trim()
    if (line === '## Content') { i++; foundContent = true; break }
    if (line === '## Cross-References') { i++; continue }
    if (preambleRe.test(line)) { i++; continue }
    if (line.startsWith('- [[')) { i++; continue }
    break
  }
  while (i < lines.length && lines[i].trim() === '') i++
  const out: string[] = []
  for (; i < lines.length; i++) {
    const line = lines[i].trim()
    if (line === '---' && i + 1 < lines.length && lines[i + 1].trim().startsWith('*Navigate:')) break
    if (line.startsWith('*Navigate:') && line.endsWith('*')) continue
    out.push(lines[i])
  }
  return out.join('\n')
}

const TYPE_META: Record<string, { label: string; color: string; icon: React.ReactNode }> = {
  index:     { label: 'Index',     color: 'bg-emerald-500/10 text-emerald-500 border-emerald-500/20',   icon: <Layers className="w-3 h-3" /> },
  log:       { label: 'Log',       color: 'bg-indigo-500/10 text-indigo-500 border-indigo-500/20',   icon: <Clock className="w-3 h-3" /> },
  community: { label: 'Community', color: 'bg-purple-500/10 text-purple-500 border-purple-500/20', icon: <Users className="w-3 h-3" /> },
  'god-node':{ label: 'God Node',  color: 'bg-amber-500/10 text-amber-500 border-amber-500/20', icon: <Zap className="w-3 h-3" /> },
  entity:    { label: 'Entity',    color: 'bg-blue-500/10 text-blue-500 border-blue-500/20', icon: <FileText className="w-3 h-3" /> },
  other:     { label: 'Page',      color: 'bg-slate-500/10 text-slate-500 border-slate-500/20', icon: <FileText className="w-3 h-3" /> },
}

function TypeBadge({ type }: { type: string }) {
  const m = TYPE_META[type] ?? TYPE_META.other
  return (
    <span className={cn('inline-flex items-center gap-1.5 text-[9px] px-2 py-0.5 rounded-md border font-bold uppercase tracking-wider', m.color)}>
      {m.icon}{m.label}
    </span>
  )
}

function ModuleSelector({ modules, selected, onSelect }: {
  modules: WikiModule[]; selected: WikiModule | null; onSelect: (m: WikiModule) => void
}) {
  const grouped = useMemo(() => {
    const map: Record<string, WikiModule[]> = {}
    modules.forEach(m => {
      const group = m.context === 'project' || m.context === 'user' ? 'Local Base' : 'Imported Base'
      ;(map[group] ??= []).push(m)
    })
    return map
  }, [modules])

  return (
    <div className="flex flex-col gap-1">
      {Object.entries(grouped).map(([group, items]) => (
        <div key={group} className="mb-2">
          <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/60 px-2.5 py-1 mb-1">{group}</p>
          <div className="space-y-0.5">
            {items.map(m => (
              <button
                key={m.id}
                onClick={() => onSelect(m)}
                className={cn(
                  'w-full text-left flex items-center justify-between px-2.5 py-2 rounded-xl text-xs transition-all duration-200 border border-transparent',
                  selected?.id === m.id
                    ? 'bg-primary/10 text-primary border-primary/15 font-bold'
                    : 'text-muted-foreground hover:text-foreground hover:bg-accent/40'
                )}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <BookOpen className="w-3.5 h-3.5 shrink-0 text-primary/75" />
                  <span className="truncate font-medium">{m.label}</span>
                </div>
                <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-accent/35 text-muted-foreground/80">{m.pages}p</span>
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

import { WikiTree } from './WikiTree'

function PageList({ pages, selected, onSelect }: {
  pages: WikiPageMeta[]; selected: string | null; onSelect: (path: string) => void
}) {
  const grouped = useMemo(() => {
    const map: Record<string, WikiPageMeta[]> = { Special: [], Community: [], Entities: [] }
    pages.forEach(p => {
      if (p.type === 'index' || p.type === 'log') map.Special.push(p)
      else if (p.type === 'community' || p.type === 'god-node') map.Community.push(p)
      else map.Entities.push(p)
    })
    return map
  }, [pages])

  const confColor = (c: number) => {
    if (c >= 0.8) return 'bg-emerald-500'
    if (c >= 0.5) return 'bg-amber-500'
    if (c > 0) return 'bg-red-500'
    return 'bg-muted-foreground/20'
  }

  return (
    <div className="flex flex-col gap-1">
      {Object.entries(grouped).map(([group, items]) => items.length > 0 && (
        <div key={group} className="mb-2">
          <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/60 px-2.5 py-1.5">{group}</p>
          {group === 'Entities' ? (
            <WikiTree pages={items} selectedPath={selected} onSelect={onSelect} confColor={confColor} />
          ) : (
            <div className="space-y-0.5">
              {items.map(p => {
                const isSelected = selected === p.path
                return (
                  <button
                    key={p.path}
                    onClick={() => onSelect(p.path)}
                    className={cn(
                      'w-full text-left flex items-center justify-between px-3 py-2 rounded-xl text-xs transition-all duration-200 group border border-transparent',
                      isSelected
                        ? 'bg-primary/10 text-primary border-primary/20 font-bold shadow-sm'
                        : 'text-muted-foreground hover:text-foreground hover:bg-accent/40'
                    )}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="shrink-0 transition-transform group-hover:scale-105 text-primary/75">
                        {(TYPE_META[p.type] ?? TYPE_META.other).icon}
                      </span>
                      <span className="truncate font-medium">{p.title}</span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0 ml-2">
                      {p.confidence > 0 && (
                        <span
                          title={`Confidence: ${Math.round(p.confidence * 100)}%`}
                          className={cn('w-2 h-2 rounded-full shrink-0 border border-black/10 shadow-sm', confColor(p.confidence))}
                        />
                      )}
                      <span className="text-[9px] font-bold text-muted-foreground/45 group-hover:text-muted-foreground/75 font-mono">
                        {p.wordCount}
                      </span>
                    </div>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

function ImageLightbox({ src, onClose }: { src: string; onClose: () => void }) {
  const [scale, setScale] = useState(1)
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const [isDragging, setIsDragging] = useState(false)
  const dragging = useRef(false)
  const didDrag = useRef(false)
  const lastPos = useRef({ x: 0, y: 0 })

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault()
    setScale(s => Math.max(0.5, Math.min(5, s - e.deltaY * 0.002)))
  }, [])

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return
    dragging.current = true
    setIsDragging(true)
    didDrag.current = false
    lastPos.current = { x: e.clientX, y: e.clientY }
    e.preventDefault()
  }, [])

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return
    const dx = e.clientX - lastPos.current.x
    const dy = e.clientY - lastPos.current.y
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) didDrag.current = true
    setPan(p => ({ x: p.x + dx, y: p.y + dy }))
    lastPos.current = { x: e.clientX, y: e.clientY }
  }, [])

  const handleMouseUp = useCallback(() => { dragging.current = false; setIsDragging(false) }, [])

  const handleOverlayClick = useCallback(() => {
    if (!didDrag.current) onClose()
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-[9999] bg-black/85 backdrop-blur-md flex items-center justify-center animate-in fade-in duration-200"
      style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
      onClick={handleOverlayClick}
      onWheel={handleWheel}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    >
      <div className="absolute top-4 right-4 flex items-center gap-2 z-10 bg-black/40 backdrop-blur-md border border-white/10 p-1.5 rounded-xl">
        <button
          onClick={(e) => { e.stopPropagation(); setScale(s => Math.min(5, s + 0.25)) }}
          className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
        >
          +
        </button>
        <span className="text-white/80 text-xs font-mono min-w-[5ch] text-center">{Math.round(scale * 100)}%</span>
        <button
          onClick={(e) => { e.stopPropagation(); setScale(s => Math.max(0.5, s - 0.25)) }}
          className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white font-bold text-sm transition-colors"
        >
          −
        </button>
        <button
          onClick={(e) => { e.stopPropagation(); setScale(1); setPan({ x: 0, y: 0 }) }}
          className="px-3 py-1.5 rounded-lg bg-white/10 hover:bg-white/20 text-white text-xs font-semibold transition-colors"
        >
          Reset
        </button>
        <div className="w-px h-5 bg-white/10 mx-1" />
        <button
          onClick={(e) => { e.stopPropagation(); onClose() }}
          className="w-8 h-8 rounded-lg bg-white/10 hover:bg-white/20 text-white flex items-center justify-center transition-colors"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
      <img
        src={src}
        alt="Zoomed view"
        className="max-w-[92vw] max-h-[92vh] bg-white rounded-2xl shadow-2xl select-none pointer-events-none transition-transform duration-75"
        style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})` }}
        draggable={false}
      />
    </div>
  )
}

function StatsBar({ pages }: { pages: WikiPageMeta[] }) {
  const total = pages.length
  const entities = pages.filter(p => p.type === 'entity').length
  const communities = pages.filter(p => p.type === 'community').length
  const godNodes = pages.filter(p => p.type === 'god-node').length
  const uniqueLinks = new Set<string>()
  pages.forEach(p => {
    (p.links ?? []).forEach(l => uniqueLinks.add(l))
  })
  const totalLinks = uniqueLinks.size
  const stats = [
    { label: 'Pages', value: total, icon: <FileText className="w-4 h-4" />, gradient: "from-blue-500/10 to-blue-500/5 text-blue-500" },
    { label: 'Entities', value: entities, icon: <Hash className="w-4 h-4" />, gradient: "from-purple-500/10 to-purple-500/5 text-purple-500" },
    { label: 'Communities', value: communities, icon: <GitBranch className="w-4 h-4" />, gradient: "from-emerald-500/10 to-emerald-500/5 text-emerald-500" },
    { label: 'God Nodes', value: godNodes, icon: <Zap className="w-4 h-4" />, gradient: "from-amber-500/10 to-amber-500/5 text-amber-500" },
    { label: 'Links', value: totalLinks, icon: <ExternalLink className="w-4 h-4" />, gradient: "from-indigo-500/10 to-indigo-500/5 text-indigo-500" },
  ]
  return (
    <div className="grid grid-cols-2 sm:grid-cols-5 gap-4 mb-6">
      {stats.map(s => (
        <div
          key={s.label}
          className="relative group bg-card/60 border border-border/40 rounded-2xl p-4 flex flex-col items-center justify-center text-center gap-1.5 transition-all duration-300 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30"
        >
          <div className={cn("p-2.5 rounded-xl border border-transparent shadow-inner flex items-center justify-center", s.gradient)}>
            {s.icon}
          </div>
          <span className="text-2xl font-black font-heading text-foreground tracking-tight mt-1">{s.value}</span>
          <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/70">{s.label}</span>
        </div>
      ))}
    </div>
  )
}

interface WikiExplorerProps {
  moduleFilter?: string
  autoSelectProject?: boolean
}

export default function WikiExplorerPage({ moduleFilter, autoSelectProject }: WikiExplorerProps = {}) {
  const { moduleId: moduleIdFromURL, sessionId } = useParams<{ moduleId?: string; sessionId?: string }>()
  const navigate = useNavigate()
  const location = useLocation()
  const externalState = location.state as {
    aiResponse?: AISearchResponse; searchQuery?: string; sessionId?: string;
    keywordResults?: MultiKeywordResult[];
    history?: Array<{ role: string; content: string }>;
  } | null
  const cameFromExternalSearch = useRef(false)
  useEffect(() => {
    if (externalState?.aiResponse || externalState?.keywordResults) {
      cameFromExternalSearch.current = true
    }
  }, [externalState])
  const { activeProjectDir } = useAppStore()
  const [modules, setModules] = useState<WikiModule[]>([])
  const [selectedModule, setSelectedModule] = useState<WikiModule | null>(null)
  const [pages, setPages] = useState<WikiPageMeta[]>([])
  const [allModulePages, setAllModulePages] = useState<Record<string, WikiPageMeta[]>>({})
  const [selectedPage, setSelectedPage] = useState<string | null>(null)
  const [pageContent, setPageContent] = useState<WikiPageContent | null>(null)
  const [searchQ, setSearchQ] = useState('')
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(null)
  const [aiResponse, setAiResponse] = useState<AISearchResponse | null>(null)
  const [aiSessionId, setAiSessionId] = useState<string | null>(null)
  const [aiHistory, setAiHistory] = useState<Array<{ role: string; content: string }>>([])
  const [loading, setLoading] = useState(false)
  const [aiLoading, setAiLoading] = useState(false)
  const [view, setView] = useState<'browse' | 'search' | 'ai-search'>('browse')
  const [searchMode, setSearchMode] = useState<'keyword' | 'ai'>('keyword')
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null)
  const [navHistory, setNavHistory] = useState<{ path: string; title: string }[]>([])
  const [navIndex, setNavIndex] = useState(-1)

  const { processedMarkdown, hasH1 } = useMemo(() => {
    if (!pageContent) return { processedMarkdown: '', hasH1: false }
    const processed = preprocessContent(pageContent.content, pageContent.title, pageContent.type)
    const hasH1 = processed.trim().startsWith('# ')
    return { processedMarkdown: processed, hasH1 }
  }, [pageContent])
  
  const left = useResizable(280, 200, 480, 'right', 'graphit_wiki_left_width')
  const [leftCollapsed, setLeftCollapsed] = useState(
    () => LS.get<boolean>('graphit_wiki_left_collapsed', false)
  )

  const explorerBase = moduleFilter === 'knowledge'
    ? '/knowledge/explorer'
    : moduleFilter === 'memory'
      ? '/memory/explorer'
      : '/wiki'

  useEffect(() => { LS.set('graphit_wiki_left_collapsed', leftCollapsed) }, [leftCollapsed])

  useEffect(() => {
    const handler = (e: Event) => setLightboxSrc((e as CustomEvent).detail)
    document.addEventListener('wiki-lightbox', handler)
    return () => document.removeEventListener('wiki-lightbox', handler)
  }, [])

  useEffect(() => {
    const applyExternalState = () => {
      if (externalState?.aiResponse) {
        setAiResponse(externalState.aiResponse)
        setSearchQ(externalState.searchQuery ?? '')
        setSearchMode('ai')
        setView('ai-search')
        setNavHistory([{ path: '__search__', title: `AI: ${externalState.searchQuery ?? ''}` }])
        setNavIndex(0)
        setSelectedPage(null)
        if (externalState.sessionId) setAiSessionId(externalState.sessionId)
        if (externalState.history) setAiHistory(externalState.history)
        navigate(location.pathname, { replace: true, state: null })
      } else if (externalState?.keywordResults) {
        const results: SearchResult[] = externalState.keywordResults.map(r => ({
          path: r.path,
          title: `[${r.source_label}] ${r.title}`,
          snippet: r.snippet,
          score: r.score,
        }))
        setSearchResults(results)
        setSearchQ(externalState.searchQuery ?? '')
        setSearchMode('keyword')
        setView('search')
        setNavHistory([{ path: '__search__', title: `Search: ${externalState.searchQuery ?? ''}` }])
        setNavIndex(0)
        setSelectedPage(null)
        navigate(location.pathname, { replace: true, state: null })
      }
    }
    applyExternalState()
  }, [externalState])  // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!sessionId || externalState?.aiResponse) return
    
    const loadSession = async () => {
      setAiLoading(true)
      setView('ai-search')
      setSearchMode('ai')
      setSelectedPage(null)
      
      try {
        const messages = await loadSessionMessages(sessionId)
        const firstAssistantIdx = messages.findIndex(m => m.role === 'assistant')
        
        let history: Array<{ role: string; content: string }> = []
        let queryText = 'AI Query'
        let answer: string
        
        if (firstAssistantIdx !== -1) {
          answer = messages[firstAssistantIdx].content
          history = messages.slice(firstAssistantIdx + 1).map(m => ({
            role: m.role,
            content: m.content
          }))
          if (firstAssistantIdx > 0 && messages[firstAssistantIdx - 1].role === 'user') {
            queryText = messages[firstAssistantIdx - 1].content
          }
        } else {
          answer = `Session loaded.\n\nSend a follow-up to continue.`
        }

        const wikiLinkRegex = /\[\[([^\]]+)\]\]/g
        const refs: Array<{ path: string; title: string; relevance: string; score: number }> = []
        const seen = new Set<string>()
        let match: RegExpExecArray | null
        while ((match = wikiLinkRegex.exec(answer)) !== null) {
          const title = match[1]
          if (!seen.has(title)) {
            seen.add(title)
            refs.push({ path: title.replace(/\s+/g, '_') + '.md', title, relevance: 'Referenced in answer', score: 80 })
          }
        }

        setAiResponse({
          answer,
          results: refs,
          session_id: sessionId,
        })
        setSearchQ(queryText)
        setAiSessionId(sessionId)
        setAiHistory(history)
        setNavHistory([{ path: '__search__', title: `AI: ${queryText}` }])
        setNavIndex(0)
      } catch (_err) {
        setAiResponse({
          answer: '',
          results: [],
          error: 'Failed to load session history.',
        })
      } finally {
        setAiLoading(false)
      }
    }
    loadSession()
  }, [sessionId, externalState])

  useEffect(() => {
    fetchModules(activeProjectDir || undefined).then(raw => {
      const allModules = raw ?? []
      let filtered = allModules
      if (moduleFilter) {
        filtered = allModules.filter(m =>
          m.id === moduleFilter ||
          m.id.startsWith(moduleFilter + '/') ||
          m.id.startsWith(moduleFilter + '-')
        )
      }
      setModules(filtered)
      
      filtered.forEach(m => {
        fetchPages(m.path).then(ps => {
          setAllModulePages(prev => ({ ...prev, [m.id]: ps ?? [] }))
        }).catch(console.error)
      })

      if (filtered.length > 0) {
        if (moduleIdFromURL) {
          const target = filtered.find(m => m.context === moduleIdFromURL) ??
                         filtered.find(m => m.id === moduleIdFromURL)
          setSelectedModule(target ?? filtered[0])
        } else if (autoSelectProject) {
          const project = filtered.find(m => m.context === 'project')
          setSelectedModule(project ?? filtered[0])
        } else {
          setSelectedModule(filtered[0])
        }
      }
    }).catch(console.error)
  }, [moduleFilter, autoSelectProject, moduleIdFromURL, activeProjectDir])

  useEffect(() => {
    if (!selectedModule) return
    const loadPages = async () => {
      setPages([]); setSelectedPage(null); setPageContent(null)
      try {
        const raw = await fetchPages(selectedModule.path)
        const ps = raw ?? []
        setPages(ps)
        const idx = ps.find(p => p.path === 'index.md')
        if (idx) setSelectedPage(idx.path)
      } catch (e) {
        console.error(e)
      }
    }
    loadPages()
  }, [selectedModule])

  useEffect(() => {
    if (!selectedModule || !selectedPage) return
    const loadPage = async () => {
      setLoading(true)
      try {
        const c = await fetchPage(selectedModule.path, selectedPage)
        setPageContent(c)
      } finally {
        setLoading(false)
      }
    }
    loadPage()
  }, [selectedModule, selectedPage])

  const runSearch = useCallback(async () => {
    if (!selectedModule || !searchQ.trim()) return
    const searchTitle = searchMode === 'ai' ? `AI: ${searchQ}` : `Search: ${searchQ}`
    setNavHistory([{ path: '__search__', title: searchTitle }])
    setNavIndex(0)
    setSelectedPage(null)
    setAiHistory([])
    if (searchMode === 'ai') {
      setAiLoading(true)
      setView('ai-search')
      try {
        const resp = await aiSearchWiki(selectedModule.path, searchQ)
        setAiResponse(resp)
        if (resp.session_id) setAiSessionId(resp.session_id)
      } catch {
        setAiResponse({ answer: '', results: [], error: 'AI search failed. Check AI configuration.' })
      } finally {
        setAiLoading(false)
      }
    } else {
      const rs = await searchWiki(selectedModule.path, searchQ)
      setSearchResults(rs); setView('search')
    }
  }, [selectedModule, searchQ, searchMode, setAiHistory])

  const handleRefresh = useCallback(async () => {
    if (!selectedModule) return
    
    try {
      const ps = await fetchPages(selectedModule.path)
      setPages(ps ?? [])
    } catch (e) {
      console.error('Failed to refresh pages list:', e)
    }

    if (view === 'browse' && selectedPage) {
      setLoading(true)
      try {
        const c = await fetchPage(selectedModule.path, selectedPage)
        setPageContent(c)
      } catch (e) {
        console.error('Failed to refresh page content:', e)
      } finally {
        setLoading(false)
      }
    } else if (view === 'ai-search' && sessionId) {
      setAiLoading(true)
      try {
        const messages = await loadSessionMessages(sessionId)
        const firstAssistantIdx = messages.findIndex(m => m.role === 'assistant')
        
        let answer = ''
        let history: Array<{ role: string; content: string }> = []
        let queryText = 'AI Query'
        
        if (firstAssistantIdx !== -1) {
          answer = messages[firstAssistantIdx].content
          history = messages.slice(firstAssistantIdx + 1).map(m => ({
            role: m.role,
            content: m.content
          }))
          if (firstAssistantIdx > 0 && messages[firstAssistantIdx - 1].role === 'user') {
            queryText = messages[firstAssistantIdx - 1].content
          }
        } else {
          answer = `Session loaded.\n\nSend a follow-up to continue.`
        }

        const wikiLinkRegex = /\[\[([^\]]+)\]\]/g
        const refs: Array<{ path: string; title: string; relevance: string; score: number }> = []
        const seen = new Set<string>()
        let match: RegExpExecArray | null
        while ((match = wikiLinkRegex.exec(answer)) !== null) {
          const title = match[1]
          if (!seen.has(title)) {
            seen.add(title)
            refs.push({ path: title.replace(/\s+/g, '_') + '.md', title, relevance: 'Referenced in answer', score: 80 })
          }
        }

        setAiResponse({
          answer,
          results: refs,
          session_id: sessionId,
        })
        setSearchQ(queryText)
        setAiSessionId(sessionId)
        setAiHistory(history)
      } catch (e) {
        console.error('Failed to refresh session:', e)
      } finally {
        setAiLoading(false)
      }
    } else if ((view === 'search' || view === 'ai-search') && searchQ.trim()) {
      runSearch()
    }
  }, [selectedModule, view, selectedPage, searchQ, sessionId, runSearch])

  const navigateTo = useCallback((path: string, title: string, resetHistory = false) => {
    if (resetHistory) {
      setNavHistory([{ path, title }])
      setNavIndex(0)
    } else {
      setNavHistory(prev => {
        const next = [...prev.slice(0, navIndex + 1), { path, title }]
        return next
      })
      setNavIndex(prev => prev + 1)
    }
    setSelectedPage(path)
    setView('browse')
  }, [navIndex])

  const navBack = useCallback(() => {
    if (navIndex <= 0) return
    const prev = navHistory[navIndex - 1]
    if (prev.path === '__search__') {
      setView(prev.title.startsWith('AI:') ? 'ai-search' : 'search')
      setSelectedPage(null)
      setNavIndex(i => i - 1)
    } else {
      setSelectedPage(prev.path)
      setView('browse')
      setNavIndex(i => i - 1)
    }
  }, [navHistory, navIndex])

  const navForward = useCallback(() => {
    if (navIndex >= navHistory.length - 1) return
    const next = navHistory[navIndex + 1]
    if (next.path === '__search__') {
      setView(next.title.startsWith('AI:') ? 'ai-search' : 'search')
      setSelectedPage(null)
      setNavIndex(i => i + 1)
    } else {
      setSelectedPage(next.path)
      setView('browse')
      setNavIndex(i => i + 1)
    }
  }, [navHistory, navIndex])

  const onWikiLink = useCallback(async (target: string) => {
    let moduleContext: string | null = null
    let pageTarget = target

    if (target.includes('/')) {
      const parts = target.split('/')
      moduleContext = parts[0]
      pageTarget = parts.slice(1).join('/')
    }

    const normTarget = pageTarget.toLowerCase().replace(/[^a-z0-9]/g, '')

    const findPage = (pageList: WikiPageMeta[]) => {
      if (!normTarget) return undefined
      return pageList.find(p => {
        const normTitle = p.title.toLowerCase().replace(/[^a-z0-9]/g, '')
        const normPath = p.path.toLowerCase().replace(/[^a-z0-9]/g, '')
        const normSource = (p.source ?? '').toLowerCase().replace(/[^a-z0-9]/g, '')
        return (
          (normTitle && normTitle === normTarget) ||
          (normPath && normPath.includes(normTarget)) ||
          (normSource && normSource.includes(normTarget))
        )
      })
    }

    if (moduleContext) {
      const targetModule = modules.find(m => m.context === moduleContext || m.id === moduleContext)
      if (targetModule) {
        if (!selectedModule || selectedModule.id !== targetModule.id) {
          setSelectedModule(targetModule)
          setView('browse')
          navigate(`${explorerBase}/${encodeURIComponent(targetModule.context)}`, { replace: true })
          try {
            const ps = allModulePages[targetModule.id] ?? await fetchPages(targetModule.path)
            setPages(ps)
            const found = findPage(ps)
            if (found) navigateTo(found.path, found.title)
          } catch (e) {
            console.error('Failed to load target module pages:', e)
          }
          return
        }
      }
    }

    const found = findPage(pages)
    if (found) {
      navigateTo(found.path, found.title)
      return
    }

    for (const [modId, modPages] of Object.entries(allModulePages)) {
      if (selectedModule && modId === selectedModule.id) continue
      const matched = findPage(modPages)
      if (matched) {
        const targetModule = modules.find(m => m.id === modId)
        if (targetModule) {
          setSelectedModule(targetModule)
          setPages(modPages)
          setView('browse')
          navigateTo(matched.path, matched.title)
          navigate(`${explorerBase}/${encodeURIComponent(targetModule.context)}`, { replace: true })
          return
        }
      }
    }
  }, [pages, modules, selectedModule, allModulePages, explorerBase, navigate, navigateTo])

  const hasWiki = modules.length > 0

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden animate-in fade-in duration-200">
      <div
        className={cn(
          'flex flex-col border-r border-border/40 bg-card/45 backdrop-blur-2xl shrink-0 relative transition-all duration-300 ease-in-out z-20',
          leftCollapsed ? 'w-[52px]' : '',
        )}
        style={leftCollapsed ? undefined : { width: left.size }}
      >
        <div className="flex items-center justify-between px-3.5 py-4 border-b border-border/40 gap-2 shrink-0">
          {!leftCollapsed ? (
            <button
              onClick={() => {
                if (cameFromExternalSearch.current) navigate('/wiki/search')
                else if (moduleFilter === 'knowledge') navigate('/knowledge/contexts')
                else if (moduleFilter === 'memory') navigate(-1)
                else navigate('/wiki/search')
              }}
              className="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground transition-colors group"
            >
              <ArrowLeft className="w-3.5 h-3.5 group-hover:-translate-x-0.5 transition-transform" />
              {moduleFilter === 'memory' ? 'Back' : 'Contexts'}
            </button>
          ) : (
            <button
              onClick={() => {
                if (cameFromExternalSearch.current) navigate('/wiki/search')
                else if (moduleFilter === 'knowledge') navigate('/knowledge/contexts')
                else if (moduleFilter === 'memory') navigate(-1)
                else navigate('/wiki/search')
              }}
              className="p-1.5 rounded-lg border border-border/30 hover:bg-accent/50 text-muted-foreground hover:text-foreground transition-all flex items-center justify-center mx-auto"
              title={moduleFilter === 'memory' ? 'Back' : 'Back to Contexts'}
            >
              <ArrowLeft className="w-3.5 h-3.5" />
            </button>
          )}

          <button
            onClick={() => setLeftCollapsed((v) => !v)}
            className={cn('p-1.5 rounded-lg border border-border/30 bg-background/50 hover:bg-accent text-muted-foreground hover:text-foreground transition-all', leftCollapsed && 'mx-auto mt-1')}
          >
            {leftCollapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-2 py-3 space-y-4">
          {!leftCollapsed && modules.length > 1 && (
            <div>
              <ModuleSelector modules={modules} selected={selectedModule} onSelect={m => {
                setSelectedModule(m); setView('browse')
                navigate(`${explorerBase}/${encodeURIComponent(m.context)}`, { replace: true })
              }} />
            </div>
          )}

          {!leftCollapsed && modules.length <= 1 && selectedModule && (
            <div className="px-3.5 py-2.5 bg-accent/20 border border-border/20 rounded-2xl mx-1">
              <p className="text-xs font-bold text-foreground truncate" title={selectedModule.label}>
                {selectedModule.label}
              </p>
              <p className="text-[9px] font-bold text-muted-foreground uppercase tracking-widest mt-1">
                {moduleFilter === 'knowledge' ? 'Knowledge base' : 'Wiki docs'}
              </p>
            </div>
          )}

          {!leftCollapsed && pages.length > 0 && (
            <div className="animate-in fade-in duration-200">
              <div className="flex items-center justify-between px-2.5 py-1.5 mb-1.5">
                <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/60">Wiki Index</p>
                <span className="text-[10px] font-bold text-muted-foreground/45 bg-accent/20 px-2 py-0.5 rounded-full">{pages.length}</span>
              </div>
              <PageList pages={pages} selected={selectedPage} onSelect={p => {
                const pg = pages.find(x => x.path === p)
                navigateTo(p, pg?.title ?? p, true)
              }} />
            </div>
          )}
        </div>

        {!leftCollapsed && (
          <div
            className="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-primary/50 transition-colors z-20"
            onMouseDown={left.onMouseDown}
          />
        )}
      </div>

      <div className="flex-1 flex flex-col overflow-hidden bg-background">
        <div className="flex items-center gap-3 px-4 py-3 border-b border-border/40 bg-card/35 backdrop-blur-md shrink-0 z-10 justify-between">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex items-center gap-1 shrink-0 bg-accent/25 border border-border/30 p-0.5 rounded-xl">
              <button
                onClick={navBack}
                disabled={navIndex <= 0}
                className="p-1.5 rounded-lg hover:bg-background/80 disabled:opacity-25 text-muted-foreground hover:text-foreground transition-all disabled:pointer-events-none active:scale-95"
                title="Navigate Back"
              >
                <ArrowLeft className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={navForward}
                disabled={navIndex >= navHistory.length - 1}
                className="p-1.5 rounded-lg hover:bg-background/80 disabled:opacity-25 text-muted-foreground hover:text-foreground transition-all disabled:pointer-events-none active:scale-95"
                title="Navigate Forward"
              >
                <ArrowRight className="w-3.5 h-3.5" />
              </button>
            </div>

            <div className="flex items-center gap-1.5 text-xs text-muted-foreground/80 font-medium overflow-x-auto whitespace-nowrap min-w-0 pr-2">
              <button
                onClick={() => {
                  const idx = pages.find(p => p.type === 'index')
                  if (idx) navigateTo(idx.path, idx.title, true)
                }}
                className="hover:text-foreground transition-colors font-bold uppercase tracking-wider text-[11px]"
              >
                {selectedModule?.label ?? 'Wiki'}
              </button>
              {navHistory.slice(0, navIndex + 1).map((entry, idx) => (
                <React.Fragment key={idx}>
                  <ChevronRight className="w-3.5 h-3.5 shrink-0 opacity-40" />
                  {entry.path === '__search__' ? (
                    <button
                      onClick={() => { setNavIndex(idx); setView(entry.title.startsWith('AI:') ? 'ai-search' : 'search'); setSelectedPage(null) }}
                      className={cn(
                        'shrink-0 flex items-center gap-1 px-2 py-0.5 rounded-md hover:bg-accent/40 transition-colors',
                        idx === navIndex ? 'text-primary font-bold bg-primary/10' : 'text-muted-foreground hover:text-foreground'
                      )}
                    >
                      <Search className="w-3 h-3" />
                      <span className="truncate max-w-[120px]">{entry.title.replace(/^(AI:|Search:)\s*/, '')}</span>
                    </button>
                  ) : (
                    <button
                      onClick={() => { setNavIndex(idx); setSelectedPage(entry.path); setView('browse') }}
                      className={cn(
                        'shrink-0 truncate max-w-[200px] px-2 py-0.5 rounded-md hover:bg-accent/40 transition-all font-semibold',
                        idx === navIndex ? 'text-primary font-extrabold bg-primary/10' : 'text-muted-foreground hover:text-foreground'
                      )}
                    >
                      {entry.title}
                    </button>
                  )}
                </React.Fragment>
              ))}
              {pageContent && navIndex >= 0 && navHistory[navIndex]?.path !== '__search__' && (
                <div className="ml-2 shrink-0">
                  <TypeBadge type={pageContent.type} />
                </div>
              )}
            </div>
          </div>

          <button
            onClick={handleRefresh}
            className="p-2 rounded-xl border border-border/40 bg-card/65 hover:bg-accent hover:text-foreground transition-all hover:scale-[1.02] text-muted-foreground flex items-center justify-center shrink-0 shadow-sm"
            title="Refresh Content & Pages Index"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
        </div>

        <div className="border-b border-border/30 bg-card/20 shrink-0">
          <div className="flex flex-col gap-3 px-6 py-4">
            <div className="flex items-center gap-1 bg-accent/25 border border-border/30 rounded-xl p-1 w-fit">
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
              <button
                onClick={() => setSearchMode('ai')}
                className={cn(
                  'flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-semibold transition-all',
                  searchMode === 'ai'
                    ? 'bg-background shadow-sm text-foreground font-bold border border-border/20 scale-[1.02]'
                    : 'text-muted-foreground hover:text-foreground'
                )}
              >
                <Wand2 className="w-3.5 h-3.5 text-primary" /> AI Insights Search
              </button>
            </div>

            <div className="relative group/input">
              <textarea
                id="wiki-search-input"
                value={searchQ}
                onChange={e => setSearchQ(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); runSearch() } }}
                placeholder={searchMode === 'ai'
                  ? 'Ask anything about the system architecture, ADRs, or wiki content...'
                  : 'Type keywords to locate pages...'}
                rows={searchMode === 'ai' ? 2 : 1}
                className={cn(
                  'w-full pl-4 pr-24 py-3 rounded-2xl border border-border/40 bg-card/60 text-sm outline-none transition-all focus:border-primary/45 focus:ring-1 focus:ring-primary/20 resize-none shadow-sm',
                  searchMode === 'ai' ? 'font-sans' : 'font-mono',
                )}
              />
              <div className="absolute right-3 bottom-3 flex items-center gap-2">
                {(view === 'search' || view === 'ai-search') && (
                  <button
                    onClick={() => { setView('browse'); setSearchResults(null); setAiResponse(null); setAiHistory([]); setSearchQ(''); setNavHistory([]); setNavIndex(-1) }}
                    className="text-xs font-semibold px-2.5 py-1.5 rounded-xl bg-accent hover:bg-accent/80 text-muted-foreground hover:text-foreground transition-all"
                  >
                    Clear
                  </button>
                )}
                <button
                  onClick={runSearch}
                  disabled={aiLoading || !searchQ.trim()}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-primary text-primary-foreground text-xs font-bold hover:bg-primary/95 disabled:opacity-40 transition-all shadow-md active:scale-95 disabled:pointer-events-none"
                >
                  {aiLoading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
                  {searchMode === 'ai' ? 'Query' : 'Find'}
                </button>
              </div>
            </div>
            {searchMode === 'ai' && (
              <p className="text-[10px] font-bold text-muted-foreground/60 tracking-wide flex items-center gap-1">
                <span className="w-1 h-1 rounded-full bg-primary" />
                Press <kbd className="px-1 py-0.5 rounded bg-accent/40 font-mono text-[9px]">Ctrl+Enter</kbd> to run AI analysis on the current wiki.
              </p>
            )}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto">
          {!hasWiki ? (
            <div className="max-w-md mx-auto py-16 px-4">
              <EmptyState />
            </div>
          ) : view === 'ai-search' ? (
            <AISearchResultsView
              response={aiResponse}
              loading={aiLoading}
              sessionId={aiSessionId}
              initialHistory={aiHistory}
              onChat={async (msg) => {
                if (!aiSessionId) return
                const resp = await chatWiki(aiSessionId, msg)
                if (resp.error) return resp
                return resp
              }}
              onSelect={onWikiLink}
              onWikiLink={onWikiLink}
              pages={pages}
              allModulePages={allModulePages}
              selectedModule={selectedModule}
            />
          ) : view === 'search' && searchResults ? (
            <SearchResultsView
              results={searchResults}
              onSelect={onWikiLink}
            />
          ) : (
            <div className="max-w-4xl mx-auto px-8 py-8 animate-in fade-in duration-300">
              {pages.length > 0 && view === 'browse' && !selectedPage && (
                <div className="space-y-6">
                  <div className="flex flex-col gap-1.5 mb-6">
                    <h2 className="text-xl font-heading font-extrabold text-foreground">Wiki Hub Dashboard</h2>
                    <p className="text-xs text-muted-foreground">General statistics, indexed files distribution and structure summary.</p>
                  </div>
                  <StatsBar pages={pages} />
                </div>
              )}
              {loading && (
                <div className="flex items-center justify-center py-32">
                  <div className="flex flex-col items-center gap-3">
                    <Loader2 className="w-8 h-8 text-primary animate-spin" />
                    <span className="text-xs font-bold text-muted-foreground/80">Fetching Document Contents...</span>
                  </div>
                </div>
              )}
              {!loading && pageContent && (
                <article className="prose-wiki animate-in fade-in slide-in-from-bottom-2 duration-300">
                  <div className="mb-8 pb-4 border-b border-border/40 flex flex-wrap items-center gap-3 text-xs text-muted-foreground/85 font-medium bg-accent/15 px-4 py-2.5 rounded-2xl border border-border/25">
                    <TypeBadge type={pageContent.type} />
                    <span className="w-1 h-1 rounded-full bg-border/80" />
                    <span className="font-mono text-[11px] bg-background/50 px-2 py-0.5 rounded border border-border/20">{pageContent.wordCount} words</span>
                    {(pageContent.links ?? []).length > 0 && (
                      <>
                        <span className="w-1 h-1 rounded-full bg-border/80" />
                        <span className="bg-background/50 px-2 py-0.5 rounded border border-border/20">{(pageContent.links ?? []).length} outbound links</span>
                      </>
                    )}
                    {pageContent.confidence > 0 && (
                      <>
                        <span className="w-1 h-1 rounded-full bg-border/80" />
                        <span>Confidence: <strong className="text-foreground">{Math.round(pageContent.confidence * 100)}%</strong></span>
                      </>
                    )}
                    {pageContent.source && (
                      <>
                        <span className="w-1 h-1 rounded-full bg-border/80" />
                        <span className="font-mono text-[10px] truncate max-w-[200px]" title={pageContent.source}>{pageContent.source}</span>
                      </>
                    )}
                    {(pageContent.tags ?? []).length > 0 && (
                      <div className="flex flex-wrap gap-1 ml-auto">
                        {(pageContent.tags ?? []).map(t => (
                          <span key={t} className="bg-primary/5 text-primary border border-primary/10 px-2 py-0.5 rounded-lg text-[9px] font-bold uppercase tracking-wider">{t}</span>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="min-h-[250px] pb-10">
                    {!hasH1 && (
                      <h1 className="text-2xl font-heading font-extrabold tracking-tight text-foreground mt-0 mb-6 pb-2 border-b border-border/40 leading-snug">
                        {pageContent.title}
                      </h1>
                    )}
                    <WikiMarkdown content={processedMarkdown} onLink={onWikiLink} />
                  </div>

                  {(pageContent.links ?? []).length > 0 && (
                    <div className="mt-10 p-5 bg-card/50 backdrop-blur-sm border border-border/40 rounded-2xl shadow-sm">
                      <h3 className="text-[11px] font-extrabold uppercase tracking-widest text-muted-foreground/80 mb-3.5 flex items-center gap-1.5">
                        <ExternalLink className="w-3.5 h-3.5 text-primary" />
                        Outbound Document References ({[...new Set(pageContent.links ?? [])].length})
                      </h3>
                      <div className="flex flex-wrap gap-2">
                        {[...new Set(pageContent.links ?? [])].map(link => (
                          <button
                            key={link}
                            onClick={() => onWikiLink(link)}
                            className="text-xs bg-background/50 hover:bg-accent/40 border border-border/30 hover:border-primary/30 px-3 py-1.5 rounded-xl transition-all text-foreground/75 hover:text-foreground font-semibold flex items-center gap-1 group/btn"
                          >
                            <span className="truncate">{wikiLinkFriendlyName(link)}</span>
                            <ChevronRight className="w-3 h-3 text-muted-foreground/60 group-hover/btn:translate-x-0.5 transition-transform" />
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </article>
              )}
              {!loading && !pageContent && pages.length > 0 && (
                <div className="py-6" />
              )}
            </div>
          )}
        </div>
      </div>

      {lightboxSrc && <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />}
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center gap-5 bg-card/35 border border-border/30 backdrop-blur-md rounded-2xl p-8">
      <div className="w-14 h-14 rounded-2xl bg-primary/10 flex items-center justify-center border border-primary/20">
        <BookOpen className="w-7 h-7 text-primary" />
      </div>
      <div>
        <h2 className="text-lg font-bold text-foreground mb-1.5">No indexed wikis found</h2>
        <p className="text-xs text-muted-foreground max-w-xs mx-auto leading-relaxed">
          Index project documentation first with knowledge CLI tool to construct relationships and links.
        </p>
      </div>
      <div className="flex flex-col gap-2 text-left mt-3 w-full max-w-sm">
        {[
          'graphit knowledge index docs/',
          'graphit memory index',
        ].map(cmd => (
          <code key={cmd} className="text-[11px] bg-accent/35 border border-border/20 rounded-xl px-4 py-2 font-mono text-foreground/80 flex justify-between items-center group">
            {cmd}
            <button
              onClick={() => navigator.clipboard.writeText(cmd)}
              className="opacity-0 group-hover:opacity-100 transition-opacity text-[10px] font-bold text-primary hover:underline"
            >
              Copy
            </button>
          </code>
        ))}
      </div>
    </div>
  )
}

function SearchResultsView({ results, onSelect }: { results: SearchResult[]; onSelect: (path: string) => void }) {
  return (
    <div className="max-w-4xl mx-auto px-8 py-8 animate-in fade-in duration-200">
      <div className="flex items-center gap-2 mb-6 pb-2 border-b border-border/30">
        <BarChart3 className="w-4 h-4 text-primary" />
        <span className="text-xs font-extrabold uppercase tracking-wider text-muted-foreground">{results.length} search hits found</span>
      </div>
      {results.length === 0 ? (
        <div className="bg-card/45 border border-border/35 rounded-2xl p-8 text-center text-sm text-muted-foreground">
          No matches found for the requested keywords.
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {results.map(r => (
            <button
              key={r.path}
              onClick={() => onSelect(r.path)}
              className="text-left bg-card/50 hover:bg-card/95 border border-border/40 rounded-2xl p-5 hover:border-primary/30 transition-all hover:-translate-y-0.5 shadow-sm group"
            >
              <div className="flex items-center gap-2 mb-2">
                <span className="text-[15px] font-bold text-foreground group-hover:text-primary transition-colors">{r.title}</span>
                <span className="text-[10px] font-bold bg-primary/10 text-primary border border-primary/15 px-2 py-0.5 rounded-full ml-auto">{r.score} hits</span>
              </div>
              <p className="text-xs text-muted-foreground/85 font-sans leading-relaxed line-clamp-3 bg-accent/20 px-3 py-2 rounded-xl border border-border/10 mb-2">
                {r.snippet}
              </p>
              <p className="text-[10px] text-muted-foreground/45 font-mono truncate">{r.path}</p>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function extractConsultedDocs(
  content: string,
  allModulePages: Record<string, WikiPageMeta[]>,
  currentModule: WikiModule | null
): Array<{ path: string; title: string; target: string; score?: number }> {
  const seen = new Set<string>()
  const docs: Array<{ path: string; title: string; target: string; score?: number }> = []

  const currentPages = currentModule ? (allModulePages[currentModule.id] ?? []) : []

  const findPageInList = (pageList: WikiPageMeta[], normTarget: string) => {
    if (!normTarget) return undefined
    return pageList.find(p => {
      const normTitle = p.title.toLowerCase().replace(/[^a-z0-9]/g, '')
      const normPath = p.path.toLowerCase().replace(/[^a-z0-9]/g, '')
      const normSource = (p.source ?? '').toLowerCase().replace(/[^a-z0-9]/g, '')
      return (
        (normTitle && normTitle === normTarget) ||
        (normPath && normPath.includes(normTarget)) ||
        (normSource && normSource.includes(normTarget))
      )
    })
  }

  const resolveTarget = (targetName: string) => {
    const normTarget = targetName.toLowerCase().replace(/[^a-z0-9]/g, '')
    
    if (currentModule) {
      const found = findPageInList(currentPages, normTarget)
      if (found) {
        return {
          path: found.path,
          title: found.title,
          target: found.path,
          score: found.confidence ? Math.round(found.confidence * 100) : 80
        }
      }
    }

    for (const [modId, modPages] of Object.entries(allModulePages)) {
      if (currentModule && modId === currentModule.id) continue
      const found = findPageInList(modPages, normTarget)
      if (found) {
        return {
          path: found.path,
          title: found.title,
          target: modId + '/' + found.path,
          score: found.confidence ? Math.round(found.confidence * 100) : 80
        }
      }
    }

    return {
      path: targetName.replace(/\s+/g, '_') + '.md',
      title: wikiLinkFriendlyName(targetName),
      target: targetName,
      score: 80
    }
  }

  const wikiLinkRegex = /\[\[([^\]]+)\]\]/g
  let match: RegExpExecArray | null
  while ((match = wikiLinkRegex.exec(content)) !== null) {
    const target = match[1]
    const resolved = resolveTarget(target)
    const key = resolved.target
    if (!seen.has(key)) {
      seen.add(key)
      docs.push(resolved)
    }
  }

  const wikiUrlRegex = /wiki:\/\/([^\s)]+)/g;
  wikiUrlRegex.lastIndex = 0;
  while ((match = wikiUrlRegex.exec(content)) !== null) {
    let target = decodeURIComponent(match[1])
    if (target.includes('/')) {
      const parts = target.split('/')
      target = parts.slice(1).join('/')
    }
    const resolved = resolveTarget(target)
    const key = resolved.target
    if (!seen.has(key)) {
      seen.add(key)
      docs.push(resolved)
    }
  }

  return docs
}

function AISearchResultsView({ response, loading, sessionId, onChat, onSelect, onWikiLink, initialHistory, pages, allModulePages, selectedModule }: {
  response: AISearchResponse | null; loading: boolean
  sessionId?: string | null
  onChat?: (message: string) => Promise<{ answer: string; error?: string } | undefined>
  onSelect: (path: string) => void; onWikiLink: (target: string) => void
  initialHistory?: Array<{ role: string; content: string }>
  pages: WikiPageMeta[]
  allModulePages: Record<string, WikiPageMeta[]>
  selectedModule: WikiModule | null
}) {
  const [chatMessages, setChatMessages] = useState<Array<{ role: string; content: string }>>(() => initialHistory ?? [])

  useEffect(() => {
    setChatMessages(initialHistory ?? [])
  }, [initialHistory])
  const [chatInput, setChatInput] = useState('')
  const [chatLoading, setChatLoading] = useState(false)
  const chatEndRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [chatMessages, chatLoading])

  const handleSendChat = useCallback(async () => {
    if (!chatInput.trim() || !onChat || chatLoading) return
    const msg = chatInput.trim()
    setChatInput('')
    setChatMessages(prev => [...prev, { role: 'user', content: msg }])
    setChatLoading(true)
    try {
      const resp = await onChat(msg)
      if (resp?.answer) {
        setChatMessages(prev => [...prev, { role: 'assistant', content: resp.answer }])
      } else if (resp?.error) {
        setChatMessages(prev => [...prev, { role: 'error', content: resp.error! }])
      }
    } catch (e: unknown) {
      setChatMessages(prev => [...prev, { role: 'error', content: e instanceof Error ? e.message : String(e) }])
    } finally {
      setChatLoading(false)
    }
  }, [chatInput, onChat, chatLoading])

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-32 gap-4 animate-in fade-in duration-300">
        <div className="relative">
          <div className="w-14 h-14 border-2 border-primary/20 border-t-primary rounded-full animate-spin" />
          <Wand2 className="w-6 h-6 text-primary absolute inset-0 m-auto animate-pulse" />
        </div>
        <p className="text-xs font-bold text-muted-foreground animate-pulse tracking-wide">AI Engine is assembling knowledge details...</p>
      </div>
    )
  }

  if (!response) return null

  if (response.error) {
    return (
      <div className="max-w-4xl mx-auto px-8 py-8">
        <div className="bg-red-500/10 border border-red-500/25 rounded-2xl p-5 text-xs font-bold text-red-500 shadow-sm leading-relaxed">
          {response.error}
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-4xl mx-auto px-8 py-8 animate-in fade-in duration-300">
          
          {response.answer && (
            <article className="prose-wiki mb-8">
              <div className="flex items-center gap-3 mb-6 pb-4 border-b border-primary/20">
                <div className="p-2.5 bg-gradient-to-br from-primary/15 to-accent/15 rounded-xl border border-primary/10">
                  <Wand2 className="w-5 h-5 text-primary" />
                </div>
                <div>
                  <h2 className="text-base font-heading font-extrabold text-foreground leading-none">AI Synthesized Insights</h2>
                  <p className="text-[10px] text-muted-foreground/75 font-semibold mt-1">Compiled across {(response.results ?? []).length} relevant pages</p>
                </div>
              </div>
              <div className="min-h-[150px] pb-6">
                <WikiMarkdown content={response.answer} onLink={onWikiLink} />
              </div>
            </article>
          )}

          
          {(response.results ?? []).length > 0 && (
            <div className="mt-8 pt-6 border-t border-border/40">
              <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/60 mb-3">
                Source documents consulted ({response.results.length})
              </p>
              <div className="flex flex-wrap gap-1.5">
                {response.results.map(r => (
                  <button key={r.path} onClick={() => onSelect(r.path)}
                    title={r.relevance}
                    className="inline-flex items-center gap-1.5 text-xs bg-muted/40 hover:bg-accent/40 border border-border hover:border-accent/60 px-2.5 py-1.5 rounded-lg transition-colors text-foreground/70 hover:text-foreground group">
                    <FileText className="w-3 h-3 shrink-0 text-muted-foreground group-hover:text-accent" />
                    <span className="truncate max-w-[200px]">{r.title}</span>
                    <span className="text-[9px] bg-primary/10 text-primary px-1.5 py-0.5 rounded-full font-semibold">{r.score}%</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          
          {chatMessages.length > 0 && (
            <div className="mt-8 pt-6 border-t border-border/40 space-y-6">
              {chatMessages.map((msg, i) => (
                <div key={i} className={cn(
                  'animate-in fade-in slide-in-from-bottom-2 duration-300',
                  msg.role === 'user' ? 'flex justify-end' : ''
                )}>
                  {msg.role === 'user' ? (
                    <div className="max-w-[80%] bg-primary/10 border border-primary/20 rounded-2xl rounded-tr-md px-4 py-3">
                      <p className="text-sm text-foreground whitespace-pre-wrap">{msg.content}</p>
                    </div>
                  ) : msg.role === 'error' ? (
                    <div className="bg-red-500/10 border border-red-500/25 rounded-2xl p-4">
                      <p className="text-xs text-red-500">{msg.content}</p>
                    </div>
                  ) : (
                    <article className="prose-wiki">
                      <div className="flex items-center gap-2 mb-3">
                        <Wand2 className="w-4 h-4 text-primary" />
                        <span className="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-wider">Follow-up Response</span>
                      </div>
                      <WikiMarkdown content={msg.content} onLink={onWikiLink} />
                      {(() => {
                        const consulted = extractConsultedDocs(msg.content, allModulePages, selectedModule)
                        if (consulted.length === 0) return null
                        return (
                          <div className="mt-4 pt-4 border-t border-border/20">
                            <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground/60 mb-3">
                              Source documents consulted ({consulted.length})
                            </p>
                            <div className="flex flex-wrap gap-1.5">
                              {consulted.map(r => (
                                <button key={r.path} onClick={() => onWikiLink(r.target)}
                                  title="Referenced in follow-up answer"
                                  className="inline-flex items-center gap-1.5 text-xs bg-muted/40 hover:bg-accent/40 border border-border hover:border-accent/60 px-2.5 py-1.5 rounded-lg transition-colors text-foreground/70 hover:text-foreground group">
                                  <FileText className="w-3 h-3 shrink-0 text-muted-foreground group-hover:text-accent" />
                                  <span className="truncate max-w-[200px]">{r.title}</span>
                                  {r.score !== undefined && (
                                    <span className="text-[9px] bg-primary/10 text-primary px-1.5 py-0.5 rounded-full font-semibold">{r.score}%</span>
                                  )}
                                </button>
                              ))}
                            </div>
                          </div>
                        )
                      })()}
                    </article>
                  )}
                </div>
              ))}

              
              {chatLoading && (
                <div className="flex items-center gap-3 py-4 animate-in fade-in duration-200">
                  <div className="w-5 h-5 border-2 border-primary/20 border-t-primary rounded-full animate-spin" />
                  <p className="text-xs font-semibold text-muted-foreground animate-pulse">Thinking...</p>
                </div>
              )}
            </div>
          )}

          
          {chatMessages.length === 0 && chatLoading && (
            <div className="mt-8 pt-6 border-t border-border/40">
              <div className="flex items-center gap-3 py-4 animate-in fade-in duration-200">
                <div className="w-5 h-5 border-2 border-primary/20 border-t-primary rounded-full animate-spin" />
                <p className="text-xs font-semibold text-muted-foreground animate-pulse">Thinking...</p>
              </div>
            </div>
          )}

          {(response.results ?? []).length === 0 && !response.answer && (
            <p className="text-sm text-muted-foreground text-center py-8">No relevant pages found.</p>
          )}

          <div ref={chatEndRef} />
        </div>
      </div>

      
      {sessionId && onChat && (
        <div className="shrink-0 border-t border-border/40 bg-card/60 backdrop-blur-xl px-8 py-4">
          <div className="max-w-4xl mx-auto">
            <div className="relative">
              <textarea
                ref={inputRef}
                value={chatInput}
                onChange={e => setChatInput(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault()
                    handleSendChat()
                  }
                }}
                placeholder="Ask a follow-up question..."
                rows={1}
                className="w-full pl-4 pr-14 py-3 rounded-2xl border border-border/40 bg-background text-sm outline-none transition-all focus:border-primary/45 focus:ring-1 focus:ring-primary/20 resize-none"
              />
              <button
                onClick={handleSendChat}
                disabled={!chatInput.trim() || chatLoading}
                className={cn(
                  "absolute right-2 bottom-2 p-2 rounded-xl transition-all",
                  chatInput.trim() && !chatLoading
                    ? "bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm"
                    : "bg-accent text-muted-foreground cursor-not-allowed"
                )}
              >
                {chatLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
              </button>
            </div>
            <p className="text-[10px] text-muted-foreground/50 mt-1.5 flex items-center gap-1">
              <kbd className="px-1 py-0.5 rounded bg-accent/40 font-mono text-[9px]">Ctrl+Enter</kbd> to send
            </p>
          </div>
        </div>
      )}
    </div>
  )
}
