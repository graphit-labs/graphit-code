import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { nativeKnowledgeApi, type ImportedKnowledgeSource, type NativeKnowledgeSearch as SearchReply, type NativeKnowledgeSearchInput, type NativeKnowledgeSearchMetric, type NativeKnowledgeSearchMode } from '@/api/nativeKnowledge'
import { importedSourceKey, nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { parseNativeJson, type NativeInteger } from '@/api/nativeJson'
import type { TaskContext } from '@/api/task'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { WorkEmpty, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { KnowledgePageCatalogue } from './KnowledgePageCatalogue'

type Scope = { kind: 'project'; selection: TaskContext } | { kind: 'imported'; source: ImportedKnowledgeSource }
type SearchPage = Extract<SearchReply, { hits: unknown[] }>
type SearchAttempt = { input: NativeKnowledgeSearchInput; append?: boolean; pages?: number }

function combinePages(earlier: SearchPage, later: SearchReply): SearchPage {
  if (!('hits' in later) || later.state === 'unavailable' || later.state === 'published_empty') {
    throw new Error('reason' in later && later.reason ? later.reason : 'The selected search continuation is unavailable.')
  }
  const hitsBySlug = new Map(earlier.hits.map(hit => [hit.slug, hit]))
  for (const hit of later.hits) {
    if (!hitsBySlug.has(hit.slug)) hitsBySlug.set(hit.slug, hit)
  }
  const hits = [...hitsBySlug.values()]
  return {
    ...later,
    hits,
    state: later.state === 'partial' || earlier.state === 'partial' ? 'partial' : hits.length ? 'hits' : 'no_hits',
    reason: later.reason ?? earlier.reason,
  }
}

export function NativeKnowledgeSearch({ scope, openPage, reader }: {
  scope: Scope; openPage: (slug: string) => void
  reader?: { slug: string | null; content: ReactNode; label: string; close: () => void }
}) {
  const binding = scope.kind === 'project' ? nativeSelectionKey(scope.selection) : importedSourceKey(scope.source)
  const [query, setQuery] = useState('')
  const [mode, setMode] = useState<NativeKnowledgeSearchMode>('fts')
  const [metric, setMetric] = useState<NativeKnowledgeSearchMetric>('cosine')
  const [rerank, setRerank] = useState<'configuration' | 'off' | 'on'>('configuration')
  const [limit, setLimit] = useState('0')
  const [pageSize, setPageSize] = useState(20)
  const [preview, setPreview] = useState(true)
  const [reply, setReply] = useState<{ binding: string; value: SearchReply; pages: number } | null>(null)
  const [error, setError] = useState<{ binding: string; message: string; attempt?: SearchAttempt } | null>(null)
  const [loading, setLoading] = useState<{ binding: string; attempt: SearchAttempt } | null>(null)
  const controller = useRef<AbortController | null>(null)
  const request = useRef(0)
  const [captured, setCaptured] = useState<{ binding: string; input: NativeKnowledgeSearchInput } | null>(null)
  const clear = () => {
    request.current += 1
    controller.current?.abort()
    setCaptured(null)
    setReply(null); setError(null); setLoading(null)
  }
  useEffect(() => () => { request.current += 1; controller.current?.abort() }, [binding])
  const result = reply?.binding === binding ? reply : null
  const read = async (input: NativeKnowledgeSearchInput, signal: AbortSignal) => scope.kind === 'project'
    ? (await nativeKnowledgeApi.search(scope.selection, input, signal)).native
    : (await nativeKnowledgeApi.importedSearch(scope.source, input, signal)).native
  const run = async (attempt: SearchAttempt) => {
    controller.current?.abort()
    const current = new AbortController(); controller.current = current
    const id = ++request.current
    const isCurrent = () => id === request.current && !current.signal.aborted
    setError(null); setLoading({ binding, attempt })
    try {
      let value = await read(attempt.input, current.signal)
      if (!isCurrent()) return
      let pages = 1
      if (attempt.append && result && 'hits' in result.value) {
        value = combinePages(result.value, value)
        pages += result.pages
      }
      while (pages < (attempt.pages ?? 1) && 'hits' in value && value.next_cursor !== null) {
        const next = await read({ ...attempt.input, cursor: value.next_cursor }, current.signal)
        if (!isCurrent()) return
        value = combinePages(value, next)
        pages += 1
      }
      if (result && attempt.pages && 'hits' in value && value.state === 'unavailable') {
        throw new Error(value.reason || 'The refreshed search is unavailable.')
      }
      setReply({ binding, value, pages })
    } catch (cause) {
      if (isCurrent()) setError({ binding, attempt, message: cause instanceof Error ? cause.message : String(cause) })
    } finally {
      if (id === request.current) setLoading(null)
    }
  }
  usePageRefresh(async () => {
    if (captured?.binding === binding) await run({ input: captured.input, pages: result?.pages ?? 1 })
  })
  const search = () => {
    if (!/^(0|[1-9]\d*)$/.test(limit)) {
      clear(); setError({ binding, message: 'Use a nonnegative whole number for the result limit.' }); return
    }
    const input = { query, mode, metric, top_k: parseNativeJson<NativeInteger>(limit), page_size: pageSize, preview, ...(rerank === 'configuration' ? {} : { rerank: rerank === 'on' }) }
    setCaptured({ binding, input })
    setReply(null)
    void run({ input })
  }
  const cancel = () => {
    request.current += 1; controller.current?.abort()
    setLoading(null); setError({ binding, attempt: loading?.attempt, message: 'Search request stopped.' })
  }
  const current = result?.value ?? null
  const failure = error?.binding === binding ? error : null
  const retry = failure?.attempt
  const pending = loading?.binding === binding
  const page = current && 'hits' in current ? current : null
  const resultHeads = useMemo(() => page?.hits.map(hit => ({slug:hit.slug,title:hit.title,updated_at:null})) ?? null, [page])

  return <WorkSection title="Search Knowledge">
    <form onSubmit={event => { event.preventDefault(); if (!pending) search() }}>
      <div className="knowledge-search-toolbar">
        <label className="work-field">Search pages<input aria-label="Search Knowledge pages" value={query} onChange={event => { clear(); setQuery(event.target.value) }} /></label>
        <button className="work-button primary" type="submit" disabled={!query.trim() || pending}>Search pages</button>
        {pending && <button className="work-button" type="button" onClick={cancel}>Cancel search</button>}
      </div>
      <details className="work-disclosure">
      <summary>Search options</summary>
      <div className="work-actions">
      <label className="work-field">Search mode<StyledSelect aria-label="Knowledge search mode" value={mode} onChange={event => { clear(); setMode(event.target.value as NativeKnowledgeSearchMode) }}><option value="fts">Keyword</option><option value="semantic">Semantic</option><option value="hybrid">Hybrid</option></StyledSelect></label>
      <label className="work-field">Distance metric<StyledSelect aria-label="Knowledge distance metric" value={metric} onChange={event => { clear(); setMetric(event.target.value as NativeKnowledgeSearchMetric) }}><option value="cosine">cosine</option><option value="l2">l2</option><option value="dot">dot</option></StyledSelect></label>
      <label className="work-field">Reranking<StyledSelect aria-label="Knowledge reranking" value={rerank} onChange={event => { clear(); setRerank(event.target.value as typeof rerank) }}><option value="configuration">Use configuration</option><option value="off">Off</option><option value="on">On</option></StyledSelect></label>
      <label className="work-field">Maximum results<input aria-label="Knowledge result limit" inputMode="numeric" value={limit} onChange={event => { clear(); setLimit(event.target.value) }} /></label>
      <label className="work-field">Results per page<StyledSelect aria-label="Knowledge results per page" value={String(pageSize)} onChange={event => { clear(); setPageSize(Number(event.target.value)) }}><option value={20}>20</option><option value={50}>50</option><option value={100}>100</option></StyledSelect></label>
      <label><input type="checkbox" checked={preview} onChange={event => { clear(); setPreview(event.target.checked) }} /> Show previews</label>
      </div>
      <p className="work-help">A maximum of zero keeps all matches available through pagination.</p>
      </details>
    </form>
    {pending && <p role="status">{loading.attempt.append ? 'Loading more results…' : loading.attempt.pages ? 'Refreshing results…' : 'Searching…'}</p>}
    {failure && <WorkNotice title="Unable to complete Knowledge search" tone="error">{failure.message}{retry && <button className="work-button" onClick={() => void run(retry)}>Retry search</button>}</WorkNotice>}
    {current?.state === 'no_corpus' && <WorkEmpty title="No Knowledge corpus has been published" />}
    {current?.state === 'registry_absent' && <WorkEmpty title="Knowledge artifact is not registered" />}
    {page && <>
      {page.state === 'published_empty' && <WorkEmpty title="The Knowledge publication is empty" />}
      {page.state === 'no_hits' && <WorkEmpty title="No Knowledge matches" />}
      {page.state === 'unavailable' && <WorkNotice title="Knowledge search unavailable">{page.reason}</WorkNotice>}
      {page.state === 'partial' && <WorkNotice title="Partial Knowledge search results">The server returned the available part of this search. {page.reason}</WorkNotice>}
    </>}
    <KnowledgePageCatalogue heads={resultHeads}
      slug={reader?.slug ?? null} label="Knowledge search results" readerLabel={reader?.label ?? 'Knowledge document'} restoreFocus={reader !== undefined}
      onSelect={slug => slug === null ? reader?.close() : openPage(slug)}
      renderMetadata={head => {
        const hit = page?.hits.find(hit => hit.slug === head.slug)
        return hit ? <>
          {hit.preview && <><p>{hit.preview.text}</p>{hit.preview.truncated && <small>Preview truncated</small>}</>}
        </> : <p className="work-help">The opened page is outside these search results.</p>
      }}>{reader?.content}</KnowledgePageCatalogue>
    {page && page.next_cursor !== null && captured?.binding === binding && <button className="work-button" disabled={pending} onClick={() => void run({ input: { ...captured.input, cursor: page.next_cursor }, append: true })}>More search results</button>}
  </WorkSection>
}
