import { useEffect, useRef, useState } from 'react'
import type { NativeKnowledgePage } from '@/api/nativeKnowledge'
import { stringifyNativeJson } from '@/api/nativeJson'
import { FactList, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { MarkdownContent } from './WikiMarkdown'

export function NativeKnowledgeDocument({ publication, openPage }: { publication: Extract<NativeKnowledgePage, { state: 'published' }>; openPage: (slug: string) => void }) {
  const document = publication.page.state === 'found' ? publication.page.document : null
  const [raw, setRaw] = useState(false)
  const [copyError, setCopyError] = useState('')
  const title = useRef<HTMLHeadingElement>(null)
  useEffect(() => { title.current?.focus({ preventScroll: true }) }, [document])
  if (!document) return null
  const copy = async () => {
    setCopyError('')
    try { await navigator.clipboard.writeText(document.body) }
    catch (cause) { setCopyError(cause instanceof Error ? cause.message : 'Could not copy the document.') }
  }
  return <>
    <h1 tabIndex={-1} ref={title}>{document.title || document.slug}</h1>
    <div className="work-actions"><button className="work-button" aria-pressed={raw} onClick={() => setRaw(value => !value)}>{raw ? 'Rendered document' : 'Raw body'}</button><button className="work-button" onClick={() => void copy()}>Copy Markdown</button></div>
    {copyError && <WorkNotice title="Unable to copy document" tone="error">{copyError}</WorkNotice>}
    {(document.previous || document.next) && <nav className="knowledge-page-sequence" aria-label="Document sequence">
      {([['Previous page', document.previous], ['Next page', document.next]] as const).map(([label, link]) => link && <div key={label}>
        <small>{label}</small>
        {link.slug !== null ? <button className="work-button" aria-label={`${label}: ${link.title || link.slug}`} onClick={() => openPage(link.slug!)}>{link.title || link.slug}</button>
          : <WorkNotice title={`${label} unavailable`} tone="error">The target “{link.target}” is not in this publication.</WorkNotice>}
      </div>)}
    </nav>}
    <WorkSection title="Document body">{raw ? <pre className="work-code">{document.body}</pre> : <MarkdownContent content={document.body} frontmatter displayedTitle={document.title || document.slug} />}</WorkSection>
    <details className="work-disclosure"><summary>Document metadata</summary><FactList items={[
      ['Slug', document.slug], ['Kind', document.kind], ['Summary', document.summary], ['Summary truncated', String(document.summary_truncated)], ['Description', document.description], ['Source path', document.source_path],
      ['Recipe', document.recipe], ['Fallback title', document.recipe_parameters.fallback_title], ['Kind on absent', document.recipe_parameters.kind_on_absent], ['Maximum summary bytes', document.recipe_parameters.max_summary_bytes],
      ['Updated at (nanoseconds)', document.updated_at], ['Stale since (nanoseconds)', document.stale_since], ['Stale after', document.stale_after ? stringifyNativeJson(document.stale_after) : null],
      ['Physical digest', stringifyNativeJson(document.physical_digest)], ['Compilation digest', stringifyNativeJson(document.compilation_digest)],
    ]} /></details>
    <details className="work-disclosure"><summary>Linked page identifiers</summary>{document.resolved_wiki_targets.length === 0 ? <p>No resolved targets were recorded.</p> : <ol>{document.resolved_wiki_targets.map((target, index) => <li key={index}><button className="work-button" onClick={() => openPage(target)}>{target}</button></li>)}</ol>}</details>
    <details className="work-disclosure"><summary>Complete publication document</summary><pre className="work-code">{stringifyNativeJson(publication, 2)}</pre></details>
  </>
}
