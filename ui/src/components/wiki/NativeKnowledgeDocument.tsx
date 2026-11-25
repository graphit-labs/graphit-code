import { ArrowLeft, ArrowRight, BookOpen, FileText, Route } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import type { NativeKnowledgeHead, NativeKnowledgePage } from '@/api/nativeKnowledge'
import { WorkNotice } from '@/components/shared/EngineeringUI'
import { MarkdownContent } from './WikiMarkdown'

function relativeMarkdownPath(href: string): { path: string; hash: string } | null {
  let path: string
  try { path = decodeURIComponent(href.split(/[?#]/, 1)[0]) } catch { return null }
  if (!path.endsWith('.md') || path.startsWith('/') || /^[a-z][a-z0-9+.-]*:/i.test(path) || /[\\\p{Cc}]/u.test(path)) return null
  const fragment = href.indexOf('#')
  return { path, hash: fragment < 0 ? '' : href.slice(fragment) }
}

export function NativeKnowledgeDocument({ publication, openPage, browseIndex, heads, pageHref, readerOrigin, fragment }: {
  publication: Extract<NativeKnowledgePage, { state: 'published' }>
  openPage: (slug: string) => void
  browseIndex: () => void
  heads?: NativeKnowledgeHead[]
  pageHref?: (slug: string) => string
  readerOrigin?: 'search' | 'catalogue'
  fragment?: string
}) {
  const document = publication.page.state === 'found' ? publication.page.document : null
  const [raw, setRaw] = useState(false)
  const [copyError, setCopyError] = useState('')
  const title = useRef<HTMLHeadingElement>(null)
  const body = useRef<HTMLElement>(null)
  useEffect(() => {
    if (fragment && body.current) {
      let id = fragment.slice(1)
      try { id = decodeURIComponent(id) } catch { /* An invalid escape cannot match a heading. */ }
      const heading = Array.from(body.current.querySelectorAll<HTMLElement>('h1[id], h2[id], h3[id], h4[id]')).find(element => element.id === id)
      if (heading) {
        heading.tabIndex = -1
        heading.focus({ preventScroll: true })
        heading.scrollIntoView?.({ block: 'start' })
        return
      }
    }
    title.current?.focus({ preventScroll: true })
  }, [document?.slug, fragment])
  if (!document) return null
  const neighbors = [
    { direction: 'previous', label: 'Previous page', shortcut: 'Earlier', link: document.previous, Icon: ArrowLeft },
    { direction: 'next', label: 'Next page', shortcut: 'Up next', link: document.next, Icon: ArrowRight },
  ]
  const hasSequence = Boolean(document.previous || document.next)
  const sequence = new Set([document.slug, document.previous?.slug, document.next?.slug])
  const pagesBySlug = new Map(heads?.map(head => [head.slug, head]))
  const renderLink = (href: string, label: ReactNode) => {
    const link = relativeMarkdownPath(href)
    if (!link) return undefined
    const unavailable = <span title="This document is unavailable in the current publication">{label}</span>
    if (!document.source_path || !pageHref) return unavailable
    const parts = document.source_path.split('/').slice(0, -1)
    for (const part of link.path.split('/')) {
      if (part === '..') {
        if (!parts.pop()) return unavailable
      } else if (part && part !== '.') {
        parts.push(part)
      }
    }
    const path = parts.join('/')
    const destinations = document.resolved_wiki_targets.filter(slug => pagesBySlug.get(slug)?.source_path === path)
    const matches = new Set(path === document.source_path ? [document.slug, ...destinations] : destinations)
    if (matches.size !== 1) return unavailable
    const slug = [...matches][0]
    return <Link to={pageHref(slug) + link.hash} state={{ knowledgeReaderOrigin: readerOrigin }} className="text-primary hover:underline underline-offset-2">{label}</Link>
  }
  const related = [...new Set(document.resolved_wiki_targets)].flatMap(slug => {
    const head = pagesBySlug.get(slug)
    return head && !sequence.has(slug) ? [head] : []
  })
  const copy = async () => {
    setCopyError('')
    try { await navigator.clipboard.writeText(document.body) }
    catch (cause) { setCopyError(cause instanceof Error ? cause.message : 'Could not copy the document.') }
  }
  return <>
    <header className="knowledge-document-header">
      <h1 className="knowledge-document-title" tabIndex={-1} ref={title}>{document.title || document.slug}</h1>
      <div className="knowledge-reading-path">
        <span>{hasSequence ? <><Route size={16} aria-hidden="true" />Reading path</> : <><FileText size={16} aria-hidden="true" />Independent page</>}</span>
        {neighbors.map(({ direction, shortcut, link }) => {
          const slug = link?.slug
          return slug ? <button key={direction} type="button" onClick={() => openPage(slug)}>
            <small>{shortcut}</small><strong>{link.title || slug}</strong>
          </button> : null
        })}
      </div>
      <div className="work-actions knowledge-document-tools">
        <button className="work-button" aria-pressed={raw} onClick={() => setRaw(value => !value)}>{raw ? 'Rendered document' : 'Raw Markdown'}</button>
        <button className="work-button" onClick={() => void copy()}>Copy Markdown</button>
      </div>
    </header>
    {copyError && <WorkNotice title="Unable to copy document" tone="error">{copyError}</WorkNotice>}
    <section ref={body} className="knowledge-document-body" aria-label="Document body">
      {raw ? <pre className="work-code">{document.body}</pre> : <MarkdownContent content={document.body} frontmatter displayedTitle={document.title || document.slug} outline renderLink={renderLink} />}
    </section>
    <footer className="knowledge-document-footer">
      {hasSequence && <nav className="knowledge-page-sequence" aria-label="Document sequence">
        {neighbors.map(({ direction, label, link, Icon }) => {
          if (!link) return null
          const { slug, title } = link
          return <div key={direction} data-direction={direction}>
            {slug !== null ? <button type="button" aria-label={`${label}: ${title || slug}`} onClick={() => openPage(slug)}>
              <Icon size={18} aria-hidden="true" /><span><small>{label}</small><strong>{title || slug}</strong></span>
            </button> : <WorkNotice title={`${label} unavailable`} tone="error">The target “{link.target}” is not in this publication.</WorkNotice>}
          </div>
        })}
      </nav>}
      <button type="button" className="work-button knowledge-browse-index" onClick={browseIndex}><BookOpen size={16} aria-hidden="true" />Browse page index</button>
      {related.length > 0 && <nav className="knowledge-related-pages" aria-label="Related pages">
        <h2>Related pages</h2>
        <ul>{related.map(head => <li key={head.slug}><button type="button" onClick={() => openPage(head.slug)}>{head.title || head.slug}<ArrowRight size={15} aria-hidden="true" /></button></li>)}</ul>
      </nav>}
    </footer>
  </>
}
