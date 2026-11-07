import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from 'react'
import { ChevronDown, ChevronRight } from 'lucide-react'
import type { NativeKnowledgeHead } from '@/api/nativeKnowledge'
import './knowledgeCatalogue.css'

/** The URL owns the single expanded page, including pages outside the head list. */
export function KnowledgePageCatalogue({ heads, slug, onSelect, label, readerLabel, children, renderMetadata, showUpdatedAt = true, restoreFocus = true }: {
  heads: NativeKnowledgeHead[] | null
  slug: string | null
  onSelect: (slug: string | null) => void
  label: string
  readerLabel: string
  children: ReactNode
  renderMetadata?: (head: NativeKnowledgeHead) => ReactNode
  showUpdatedAt?: boolean
  restoreFocus?: boolean
}) {
  const id = useId()
  const triggers = useRef(new Map<string, HTMLButtonElement>())
  const catalogue = useRef<HTMLElement>(null)
  const previous = useRef(slug)
  const reader = useRef<HTMLElement>(null)
  const pages = heads ?? []
  const requested = slug !== null && !pages.some(page => page.slug === slug)
  const entries = requested ? [...pages, { slug, title: slug, updated_at: null }] : pages
  useLayoutEffect(() => {
    const panel = reader.current
    if (slug !== null && panel) {
      const viewport = panel.closest<HTMLElement>('.work-page')
      // A final short/loading reader also needs enough space to reach the top.
      if (viewport && viewport.clientHeight > 0) panel.style.minHeight = `${viewport.clientHeight}px`
      panel.focus({preventScroll:true})
      if (viewport) {
        const header = panel.closest('.workspace-shell')?.querySelector<HTMLElement>('.workspace-header')
        const visibleTop = Math.max(viewport.getBoundingClientRect().top + viewport.clientTop, header?.getBoundingClientRect().bottom ?? 0)
        const margin = Number.parseFloat(getComputedStyle(panel).scrollMarginTop) || 0
        viewport.scrollTo({top:viewport.scrollTop + panel.getBoundingClientRect().top - visibleTop - margin,behavior:'instant'})
      }
    }
  }, [slug, heads])
  useEffect(() => {
    if (restoreFocus && previous.current !== null && slug === null) (triggers.current.get(previous.current) ?? catalogue.current)?.focus()
    previous.current = slug
  }, [slug, restoreFocus])
  return <nav ref={catalogue} tabIndex={-1} aria-label={label} className="knowledge-page-catalogue">
    {entries.map((head, index) => {
      const expanded = slug === head.slug
      const triggerId = `${id}-trigger-${index}`, panelId = `${id}-panel-${index}`
      return <div className="knowledge-page-item" key={head.slug}>
        <h3>
          <button type="button" id={triggerId}
            ref={button => { if (button) triggers.current.set(head.slug, button); else triggers.current.delete(head.slug) }}
            className={`record-link knowledge-page-trigger${expanded ? ' selected' : ''}`}
            aria-label={head.title || head.slug} aria-expanded={expanded} aria-controls={panelId}
            onClick={() => onSelect(expanded ? null : head.slug)}
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
            <span><strong>{head.title || head.slug}</strong><small>{head.slug}</small></span>
            {requested && head.slug === slug ? <span className="knowledge-page-date">Requested page</span>
              : showUpdatedAt && <span className="knowledge-page-date">{head.updated_at ?? '—'}</span>}
            {expanded ? <ChevronDown aria-hidden="true" size={16} /> : <ChevronRight aria-hidden="true" size={16} />}
          </button>
        </h3>
        {renderMetadata?.(head)}
        <div id={panelId} hidden={!expanded}>
          {expanded && <article ref={reader} tabIndex={-1} className="knowledge-reader knowledge-inline-reader" aria-label={readerLabel} aria-describedby={triggerId}>{children}</article>}
        </div>
      </div>
    })}
  </nav>
}
