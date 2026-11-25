import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { BookOpen, ChevronDown, ChevronRight, Search } from 'lucide-react'
import type { NativeKnowledgeHead } from '@/api/nativeKnowledge'
import './knowledgeCatalogue.css'

export function KnowledgePageCatalogue({ heads, slug, onSelect, label, readerLabel, children, renderMetadata, restoreFocus = true, focusIndex = false }: {
  heads: NativeKnowledgeHead[] | null
  slug: string | null
  onSelect: (slug: string | null) => void
  label: string
  readerLabel: string
  children: ReactNode
  renderMetadata?: (head: NativeKnowledgeHead) => ReactNode
  restoreFocus?: boolean
  focusIndex?: boolean
}) {
  const id = useId()
  const searchResults = Boolean(renderMetadata)
  const [filter, setFilter] = useState('')
  const [expandedPage, setExpandedPage] = useState<string | null>(null)
  const indexExpanded = slug === null || expandedPage === slug
  const triggers = useRef(new Map<string, HTMLButtonElement>())
  const catalogue = useRef<HTMLElement>(null)
  const indexFilter = useRef<HTMLInputElement>(null)
  const previous = useRef(slug)
  const reader = useRef<HTMLElement>(null)
  const query = filter.trim().toLocaleLowerCase()
  const pages = heads ?? []
  const entries = pages.filter(page => page.slug === slug || !query || [page.title, page.slug].some(text => text.toLocaleLowerCase().includes(query)))
  const requested = slug !== null && !pages.some(page => page.slug === slug)
  if (requested) entries.push({ slug, title: slug, updated_at: null })

  useLayoutEffect(() => {
    const panel = reader.current
    if (slug === null || !panel) return
    const viewport = panel.closest<HTMLElement>('.work-page')
    if (viewport && viewport.clientHeight > 0) panel.style.minHeight = `${viewport.clientHeight}px`
    panel.focus({ preventScroll: true })
    if (viewport && typeof viewport.scrollTo === 'function') {
      const header = panel.closest('.workspace-shell')?.querySelector<HTMLElement>('.workspace-header')
      const visibleTop = Math.max(viewport.getBoundingClientRect().top + viewport.clientTop, header?.getBoundingClientRect().bottom ?? 0)
      const margin = Number.parseFloat(getComputedStyle(panel).scrollMarginTop) || 0
      viewport.scrollTo({ top: viewport.scrollTop + panel.getBoundingClientRect().top - visibleTop - margin, behavior: 'instant' })
    }
  }, [slug, heads])

  useEffect(() => {
    if (focusIndex && slug === null) indexFilter.current?.focus()
    else if (restoreFocus && previous.current !== null && slug === null) (triggers.current.get(previous.current) ?? catalogue.current)?.focus()
    previous.current = slug
  }, [slug, restoreFocus, focusIndex])

  const panel = <article ref={reader} id={id} tabIndex={-1} className="knowledge-reader knowledge-inline-reader" aria-label={readerLabel}>{children}</article>
  const entryList = <ul className={searchResults ? undefined : "knowledge-index-list"}>
        {entries.map((head, index) => <li key={head.slug}>
          <button type="button"
            ref={button => { if (button) triggers.current.set(head.slug, button); else triggers.current.delete(head.slug) }}
            className="knowledge-page-trigger"
            aria-current={!searchResults && slug === head.slug ? 'page' : undefined}
            aria-expanded={searchResults ? slug === head.slug : undefined}
            aria-controls={slug === head.slug ? id : undefined}
            onClick={() => onSelect(slug === head.slug ? null : head.slug)}
            onKeyDown={event => {
              let target: number
              if (event.key === 'ArrowDown') target = (index + 1) % entries.length
              else if (event.key === 'ArrowUp') target = (index + entries.length - 1) % entries.length
              else if (event.key === 'Home') target = 0
              else if (event.key === 'End') target = entries.length - 1
              else return
              event.preventDefault()
              triggers.current.get(entries[target].slug)?.focus()
            }}>
            <strong>{head.title || head.slug}</strong>
            {!searchResults && head.updated_at && <span className="sr-only" aria-hidden="true">{head.updated_at}</span>}
            <ChevronRight aria-hidden="true" size={15} />
          </button>
          {renderMetadata?.(head)}
          {searchResults && slug === head.slug && panel}
        </li>)}
      </ul>
  return <div className={searchResults ? undefined : 'knowledge-guide'}>
    <nav ref={catalogue} tabIndex={-1} aria-label={label} className={searchResults ? "knowledge-search-results" : "knowledge-page-catalogue"}>
      {searchResults ? entryList : <>
        <div className={slug === null ? "knowledge-index-header" : "knowledge-index-header knowledge-index-header-desktop"}><BookOpen size={17} aria-hidden="true" /><strong>Page index</strong><span>{pages.length}</span></div>
        {slug !== null && <button type="button" className="knowledge-index-toggle" aria-expanded={indexExpanded} aria-controls={`${id}-index`} onClick={() => setExpandedPage(indexExpanded ? null : slug)}>
          <BookOpen size={17} aria-hidden="true" /><span>Page index</span><ChevronDown size={16} aria-hidden="true" />
        </button>}
        <div id={`${id}-index`} className="knowledge-index-content" data-expanded={indexExpanded}>
          <label className="knowledge-index-filter"><Search size={15} aria-hidden="true" /><input ref={indexFilter} type="search" value={filter} onChange={event => setFilter(event.target.value)} aria-label="Filter page index" placeholder="Filter by title" /></label>
          {entryList}
          {query && entries.length === 0 && <p className="knowledge-index-empty" role="status">No matching pages.</p>}
        </div>
      </>}
    </nav>
    {!searchResults && (slug !== null
      ? panel
      : pages.length > 0 && <div className="knowledge-reader knowledge-guide-welcome"><BookOpen aria-hidden="true" size={24} /><h3>Find your starting point</h3><p>Choose a topic in the page index or search the library.</p></div>)}
  </div>
}
