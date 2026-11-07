import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { nativeKnowledgeApi, type ImportedKnowledgeSource, type NativeKnowledgeSearch as SearchReply, type NativeKnowledgeSearchInput, type NativeKnowledgeSearchMetric, type NativeKnowledgeSearchMode } from '@/api/nativeKnowledge'
import { importedSourceKey, nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { parseNativeJson, stringifyNativeJson, type NativeInteger } from '@/api/nativeJson'
import type { TaskContext } from '@/api/task'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { FactList, WorkEmpty, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { KnowledgePageCatalogue } from './KnowledgePageCatalogue'

type Scope = { kind: 'project'; selection: TaskContext } | { kind: 'imported'; source: ImportedKnowledgeSource }

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
  const [reply, setReply] = useState<{ binding: string; value: SearchReply } | null>(null)
  const [error, setError] = useState<{ binding: string; message: string } | null>(null)
  const [loading, setLoading] = useState<string | null>(null)
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
  usePageRefresh(async () => { clear() })

  const run = async (input: NativeKnowledgeSearchInput) => {
    controller.current?.abort()
    const current = new AbortController(); controller.current = current
    const id = ++request.current
    setCaptured({ binding, input })
    setReply(null); setError(null); setLoading(binding)
    try {
      const result = scope.kind === 'project'
        ? await nativeKnowledgeApi.search(scope.selection, input, current.signal)
        : await nativeKnowledgeApi.importedSearch(scope.source, input, current.signal)
      if (id === request.current && !current.signal.aborted) setReply({ binding, value: result.native })
    } catch (cause) {
      if (id === request.current && !current.signal.aborted) setError({ binding, message: cause instanceof Error ? cause.message : String(cause) })
    } finally {
      if (id === request.current) setLoading(null)
    }
  }
  const search = () => {
    if (!/^(0|[1-9]\d*)$/.test(limit)) {
      clear(); setError({ binding, message: 'Use a nonnegative whole number for the result limit.' }); return
    }
    void run({ query, mode, metric, top_k: parseNativeJson<NativeInteger>(limit), page_size: pageSize, preview, ...(rerank === 'configuration' ? {} : { rerank: rerank === 'on' }) })
  }
  const cancel = () => {
    request.current += 1; controller.current?.abort()
    setReply(null); setLoading(null); setError({ binding, message: 'Search request stopped.' })
  }
  const current = reply?.binding === binding ? reply.value : null
  const failure = error?.binding === binding ? error.message : ''
  const pending = loading === binding
  const page = current && 'generation' in current ? current : null
  const resultHeads = useMemo(() => page?.hits.map(hit => ({slug:hit.focused_source.slug,title:hit.focused_source.title,updated_at:null})) ?? null, [page])

  return <WorkSection title="Search Knowledge">
    <form className="work-actions" onSubmit={event => { event.preventDefault(); search() }}>
      <label className="work-field">Search pages<input aria-label="Search Knowledge pages" value={query} onChange={event => { clear(); setQuery(event.target.value) }} /></label>
      <label className="work-field">Search mode<StyledSelect aria-label="Knowledge search mode" value={mode} onChange={event => { clear(); setMode(event.target.value as NativeKnowledgeSearchMode) }}><option value="fts">fts</option><option value="semantic">semantic</option><option value="hybrid">hybrid</option></StyledSelect></label>
      <label className="work-field">Distance metric<StyledSelect aria-label="Knowledge distance metric" value={metric} onChange={event => { clear(); setMetric(event.target.value as NativeKnowledgeSearchMetric) }}><option value="cosine">cosine</option><option value="l2">l2</option><option value="dot">dot</option></StyledSelect></label>
      <label className="work-field">Reranking<StyledSelect aria-label="Knowledge reranking" value={rerank} onChange={event => { clear(); setRerank(event.target.value as typeof rerank) }}><option value="configuration">Use configuration</option><option value="off">Off</option><option value="on">On</option></StyledSelect></label>
      <label className="work-field">Maximum results<input aria-label="Knowledge result limit" inputMode="numeric" value={limit} onChange={event => { clear(); setLimit(event.target.value) }} /></label>
      <label className="work-field">Results per page<StyledSelect aria-label="Knowledge results per page" value={String(pageSize)} onChange={event => { clear(); setPageSize(Number(event.target.value)) }}><option value={20}>20</option><option value={50}>50</option><option value={100}>100</option></StyledSelect></label>
      <label><input type="checkbox" checked={preview} onChange={event => { clear(); setPreview(event.target.checked) }} /> Show previews</label>
      <button className="work-button primary" type="submit" disabled={!query.trim() || pending}>{pending ? 'Searching…' : 'Search pages'}</button>
      {pending && <button className="work-button" type="button" onClick={cancel}>Cancel search</button>}
    </form>
    <p className="work-help">A maximum of zero keeps all matches available through pagination.</p>
    {failure && <WorkNotice title="Unable to complete Knowledge search" tone="error">{failure}{captured?.binding === binding && <button className="work-button" onClick={() => void run(captured.input)}>Retry search</button>}</WorkNotice>}
    {current?.state === 'no_corpus' && <WorkEmpty title="No Knowledge corpus has been published" />}
    {current?.state === 'registry_absent' && <WorkEmpty title="Knowledge artifact is not registered" />}
    {page && <>
      <FactList items={[["Source publication", page.generation.version], ["Total matches", page.total], ["Semantic publication", page.semantic_generation?.version]]} />
      {page.state === 'published_empty' && <WorkEmpty title="The Knowledge publication is empty" />}
      {page.state === 'no_hits' && <WorkEmpty title="No Knowledge matches" />}
      {page.state === 'unavailable' && <WorkNotice title="Knowledge search unavailable">{page.reason}</WorkNotice>}
      {page.state === 'partial' && <WorkNotice title="Partial Knowledge search results">The server returned the available part of this search. {page.reason}</WorkNotice>}
    </>}
    <KnowledgePageCatalogue heads={resultHeads}
      slug={reader?.slug ?? null} label="Knowledge search results" readerLabel={reader?.label ?? 'Knowledge document'} showUpdatedAt={false} restoreFocus={reader !== undefined}
      onSelect={slug => slug === null ? reader?.close() : openPage(slug)}
      renderMetadata={head => {
        const hit = page?.hits.find(hit => hit.focused_source.slug === head.slug)
        return hit ? <>
          {hit.preview && <><p>{hit.preview.text}</p>{hit.preview.truncated && <small>Preview truncated</small>}</>}
          <FactList items={[["Lexical score", hit.lexical_score], ["Semantic distance", hit.semantic_distance], ["Combined score", hit.rrf_score], ...(hit.rerank_score !== undefined ? [["Rerank score", hit.rerank_score] as [string, number]] : [])]} />
        </> : <p className="work-help">The opened page is outside these search results.</p>
      }}>{reader?.content}</KnowledgePageCatalogue>
    {page && page.next_cursor !== null && captured?.binding === binding && <button className="work-button" onClick={() => void run({ ...captured.input, cursor: page.next_cursor })}>More search results</button>}
    {current && <details className="work-disclosure"><summary>Search publication details</summary><pre className="work-code">{stringifyNativeJson(current, 2)}</pre></details>}
  </WorkSection>
}
