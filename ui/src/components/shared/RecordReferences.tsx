import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { nativeKnowledgeApi, type ImportedKnowledgeSource } from '@/api/nativeKnowledge'
import { parseNativeJson, stringifyNativeJson } from '@/api/nativeJson'
import { nativeRecordReferencesApi, type NativeReferences, type ReferenceBinding, type ReferenceDirection, type ReferencePageRequest, type ReferenceRequest, type ReferenceSource } from '@/api/nativeRecordReferences'
import { referenceFocusKey, referenceReaderHref, referenceSourceKey } from '@/api/nativeReferenceNavigation'
import { usePageRefresh } from '../layout/WorkspaceRefresh'
import { WorkSection, WorkNotice } from './EngineeringUI'
import { nativeReferencesView, referenceGenerationKey, referenceSourceLabel, type ReferenceEdgeView, type ReferenceSourceView } from './nativeReferencesView'

export function RecordReferences({ binding, unavailable = 'The reference namespace is unavailable for this record.' }: { binding: ReferenceBinding | null; unavailable?: string }) {
  if (!binding) return <WorkSection title="Record links"><WorkNotice title="References unavailable">{unavailable}</WorkNotice></WorkSection>
  const encoded = stringifyNativeJson(binding)
  return <ScopedRecordReferences key={encoded} encoded={encoded} />
}

function plans(binding: ReferenceBinding, includeProject: boolean, includeUser: boolean, imports: ImportedKnowledgeSource[]): ReferencePageRequest[] {
  const sources: ReferenceSource[] = includeProject && binding.selection ? [{ kind: 'project_task' }, { kind: 'project_memory' }, { kind: 'project_knowledge' }] : []
  if (binding.outgoing_source.kind !== 'user_memory' || includeUser) sources.push(binding.outgoing_source)
  if (includeUser) sources.push({ kind: 'user_memory' })
  sources.push(...imports)
  return [...new Map(sources.map(source => [referenceSourceKey(source), source])).values()].flatMap(source => [
    ...(referenceSourceKey(source) === referenceSourceKey(binding.outgoing_source) ? [{ source, direction: 'outgoing' as const }] : []),
    { source, direction: 'incoming' as const },
  ])
}
interface ReferenceState { key: string; raw: NativeReferences; sources: ReferenceSourceView[]; complete: Record<ReferenceDirection, boolean> }

function ScopedRecordReferences({ encoded }: { encoded: string }) {
  const binding = useMemo(() => parseNativeJson<ReferenceBinding>(encoded), [encoded])
  const [includeProject, setIncludeProject] = useState(false)
  const [includeUser, setIncludeUser] = useState(false)
  const [imports, setImports] = useState<ImportedKnowledgeSource[]>([])
  const [inventory, setInventory] = useState<ImportedKnowledgeSource[] | null>(null)
  const [inventoryError, setInventoryError] = useState('')
  const [inventoryLoading, setInventoryLoading] = useState(false)
  const [data, setData] = useState<ReferenceState | null>(null)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [paging, setPaging] = useState<string | null>(null)
  const controller = useRef<AbortController | null>(null)
  const inventoryController = useRef<AbortController | null>(null)
  const epoch = useRef(0)
  const requestedPages = useMemo(() => plans(binding, includeProject, includeUser, imports), [binding, includeProject, includeUser, imports])
  const request = useMemo<ReferenceRequest>(() => ({
    ...(binding.selection ? { selection: binding.selection } : {}),
    input: { focus: binding.focus, outgoing_source: binding.outgoing_source, page_size: 20, include_user: includeUser, contexts: [...new Map([...(binding.outgoing_source.kind === 'imported_knowledge' ? [binding.outgoing_source] : []), ...imports].map(source => [referenceSourceKey(source), source.artifact])).values()], pages: requestedPages },
  }), [binding, includeUser, imports, requestedPages])
  const requestKey = stringifyNativeJson(request)
  const currentData = data?.key === requestKey ? data : null
  const cancel = useCallback(() => { epoch.current++; controller.current?.abort(); setLoading(false); setPaging(null) }, [])
  const dispose = useCallback(() => { epoch.current++; controller.current?.abort(); inventoryController.current?.abort() }, [])
  const load = useCallback(async () => {
    controller.current?.abort()
    const abort = new AbortController(); controller.current = abort
    const current = ++epoch.current
    setData(null); setError(''); setPaging(null)
    if (!requestedPages.length) { setLoading(false); return }
    setLoading(true)
    try {
      const raw = await nativeRecordReferencesApi.read(request, abort.signal)
      const sources = raw.sources.map(nativeReferencesView)
      if (requestedPages.some(page => !sources.some(source => referenceSourceKey(source.source) === referenceSourceKey(page.source)))) throw new Error('The requested reference source is missing')
      if (!abort.signal.aborted && current === epoch.current) setData({ key: requestKey, raw, sources, complete: { outgoing: raw.outgoing_complete, incoming: raw.incoming_complete } })
    } catch (cause) {
      if (!abort.signal.aborted && current === epoch.current) setError(cause instanceof Error ? cause.message : 'Unable to read record links')
    } finally { if (!abort.signal.aborted && current === epoch.current) setLoading(false) }
  }, [request, requestKey, requestedPages])
  useEffect(() => { let active = true; queueMicrotask(() => { if (active) void load() }); return () => { active = false; cancel() } }, [load, cancel])
  useEffect(() => () => dispose(), [dispose])
  usePageRefresh(load)

  const discover = async () => {
    inventoryController.current?.abort()
    const abort = new AbortController(); inventoryController.current = abort
    setInventoryLoading(true); setInventoryError('')
    try {
      const result = await nativeKnowledgeApi.importedContexts(abort.signal)
      if (!abort.signal.aborted) setInventory(result.contexts)
    } catch (cause) { if (!abort.signal.aborted) setInventoryError(cause instanceof Error ? cause.message : 'Unable to read installed sources') }
    finally { if (!abort.signal.aborted) setInventoryLoading(false) }
  }
  const more = async (source: ReferenceSourceView, direction: ReferenceDirection) => {
    const page = source[direction]
    if (!page || page.native.next_cursor === null || !currentData) return
    controller.current?.abort()
    const abort = new AbortController(); controller.current = abort
    const current = ++epoch.current, sourceKey = referenceSourceKey(source.source), pageKey = sourceKey + ':' + direction
    setPaging(pageKey); setError('')
    try {
      const raw = await nativeRecordReferencesApi.read({ ...request, input: { ...request.input, pages: [{ source: source.source, direction, cursor: page.native.next_cursor }] } }, abort.signal)
      if (abort.signal.aborted || current !== epoch.current) return
      const next = raw.sources.find(value => referenceSourceKey(value.source) === sourceKey)
      if (!next) throw new Error('The requested reference source is missing')
      const view = nativeReferencesView(next), continuation = view[direction]
      if (!continuation || view.state !== source.state || referenceGenerationKey(view) !== referenceGenerationKey(source)) throw new Error('The reference source changed. Reload the record links.')
      const merged = { ...source, raw: next, warnings: view.warnings, coverage: view.coverage, [direction]: { native: continuation.native, edges: [...page.edges, ...continuation.edges] } }
      const sources = currentData.sources.map(value => referenceSourceKey(value.source) === sourceKey ? merged : value)
      const complete = (direction: ReferenceDirection) => requestedPages.filter(page => page.direction === direction).every(page => {
        const source = sources.find(value => referenceSourceKey(value.source) === referenceSourceKey(page.source))
        if (!source) return false
        if (source.state !== 'published') return true
        const result = source[direction]
        return !!result && result.native.complete && result.native.next_cursor === null
      })
      setData({ ...currentData, sources, complete: { outgoing: complete('outgoing'), incoming: complete('incoming') } })
    } catch (cause) { if (!abort.signal.aborted && current === epoch.current) { setData(null); setError(cause instanceof Error ? cause.message : 'Unable to read the next reference page') } }
    finally { if (!abort.signal.aborted && current === epoch.current) setPaging(null) }
  }
  const requested = (source: ReferenceSource, direction: ReferenceDirection) => requestedPages.some(page => referenceSourceKey(page.source) === referenceSourceKey(source) && page.direction === direction)
  const partial = currentData && (!currentData.complete.outgoing || !currentData.complete.incoming || currentData.sources.some(source => source.warnings.length > 0 || (['outgoing', 'incoming'] as const).some(direction => requested(source.source, direction) && source.state === 'published' && (!source[direction] || !source[direction]!.native.complete))))
  return <WorkSection title="Record links" description="Persisted references and backlinks, grouped by requested source." actions={<button className="work-button" disabled={loading || !!paging} onClick={() => void load()}>Reload links</button>}>
    <div className="work-actions">
      {binding.selection && <label><input type="checkbox" checked={includeProject} onChange={event => setIncludeProject(event.target.checked)} /> Include other project sources</label>}
      <label><input type="checkbox" checked={includeUser} onChange={event => setIncludeUser(event.target.checked)} /> Include personal Memory references</label>
      <button className="work-button" disabled={inventoryLoading} onClick={() => void discover()}>{inventoryLoading ? 'Loading artifacts…' : 'Choose imported sources'}</button>
      {(loading || paging) && <button className="work-button" onClick={cancel}>Cancel links</button>}
    </div>
    <p>Other project sources, personal Memory and imported sources are included only when requested. Destination readers check access when opened.</p>
    {inventoryError && <WorkNotice title="Installed sources unavailable" tone="error">{inventoryError}</WorkNotice>}
    {inventory && <div role="group" aria-label="Imported reference sources">{inventory.length === 0 ? <p>No installed Knowledge sources were returned.</p> : inventory.map(source => <label className="record-link" key={referenceSourceKey(source)}><input type="checkbox" checked={imports.some(value => referenceSourceKey(value) === referenceSourceKey(source))} onChange={event => setImports(values => event.target.checked ? [...values, source] : values.filter(value => referenceSourceKey(value) !== referenceSourceKey(source)))} />{referenceSourceLabel(source)}</label>)}</div>}
    {error ? <WorkNotice title="References unavailable" tone="error">{error}<button className="work-button" onClick={() => void load()}>Retry links</button></WorkNotice>
      : loading ? <p role="status">Resolving references…</p>
      : !requestedPages.length ? <WorkNotice title="Personal Memory references are not requested">Enable personal Memory references to read this source.</WorkNotice>
      : !currentData ? <p role="status">Record links are not loaded.</p>
      : <>
        {partial && <WorkNotice title="Partial reference view">Requested sources may be incomplete, unavailable, or have more pages. The returned rows are retained.</WorkNotice>}
        {currentData.raw.warnings.map((warning, index) => <WorkNotice key={index} title="Reference warning">{warning}</WorkNotice>)}
        {currentData.sources.filter(source => source.warnings.length > 0 && requestedPages.some(page => referenceSourceKey(page.source) === referenceSourceKey(source.source))).map(source => <details className="work-disclosure" key={referenceSourceKey(source.source)}>
          <summary>{referenceSourceLabel(source.source)}: {source.warnings.length} projection {source.warnings.length === 1 ? 'warning' : 'warnings'}</summary>
          {source.warnings.map((warning, index) => <pre className="work-code" key={index}>{stringifyNativeJson(warning, 2)}</pre>)}
        </details>)}
        {(['outgoing', 'incoming'] as const).map(direction => <div className="record-references" key={direction}>
          <h3>{direction === 'outgoing' ? 'References' : 'Referenced by'}</h3>
          {currentData.sources.filter(source => requested(source.source, direction)).map(source => {
            const page = source[direction], key = referenceSourceKey(source.source), pageKey = key + ':' + direction
            return <section key={key} aria-label={referenceSourceLabel(source.source) + ' ' + direction}>
              <h4>{referenceSourceLabel(source.source)}</h4>
              {source.state === 'no_corpus' ? <p>This source has no published corpus.</p>
                : source.state === 'registry_absent' ? <p>The Knowledge artifact is not registered.</p>
                : !page ? <WorkNotice title="Requested references unavailable">The native reader did not return this requested direction.</WorkNotice>
                : <>
                  <p>{String(page.native.total)} native references. {page.native.complete ? 'Projection complete.' : 'Projection incomplete.'}{page.native.next_cursor !== null && ' More pages are available.'}</p>
                  {page.edges.map((edge, index) => <ReferenceRow key={index} edge={edge} direction={direction} />)}
                  {page.edges.length === 0 && <p>{page.native.complete && page.native.next_cursor === null ? 'No links in this requested source.' : 'No rows returned on this page; the reference view is incomplete.'}</p>}
                  {page.native.next_cursor !== null && <button className="work-button" disabled={!!paging} onClick={() => void more(source, direction)}>{paging === pageKey ? 'Loading page…' : 'Next page'}<span className="sr-only"> {referenceSourceLabel(source.source)} {direction}</span></button>}
                </>}
            </section>
          })}
        </div>)}
        <details className="work-disclosure"><summary>Latest native source pages</summary>{currentData.sources.map(source => <pre className="work-code" key={referenceSourceKey(source.source)}>{stringifyNativeJson(source.raw, 2)}</pre>)}</details>
      </>}
  </WorkSection>
}

function ReferenceRow({ edge, direction }: { edge: ReferenceEdgeView; direction: ReferenceDirection }) {
  const focus = direction === 'outgoing' ? edge.target : edge.source
  const id = direction === 'outgoing' ? edge.target.id : edge.sourceId
  const title = direction === 'outgoing' ? edge.targetTitle : edge.sourceTitle
  const reader = direction === 'outgoing' ? edge.navigation?.target_rich?.reader : edge.navigation?.source_reader
  const href = focus && reader && referenceFocusKey(focus) === referenceFocusKey(reader.focus) ? referenceReaderHref(reader) : null
  const content = <span>{title !== undefined && <strong>{title}</strong>}<code>{id}</code><small>{focus?.kind ?? 'knowledge'} · {edge.provenance}{edge.relation !== null && ' · ' + edge.relation}{edge.field !== null && ' · field ' + stringifyNativeJson(edge.field)}{focus && <span> · {stringifyNativeJson(focus.scope)}{focus.context === undefined ? '' : ' · context ' + stringifyNativeJson(focus.context)}</span>}{!href && ' · Target unavailable in this context'}</small></span>
  return href ? <Link className="record-link" to={href}>{content}</Link> : <div className="record-link">{content}</div>
}
