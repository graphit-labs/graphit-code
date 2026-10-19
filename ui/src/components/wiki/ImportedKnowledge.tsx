import { RecordReferences } from '@/components/shared/RecordReferences'
import { knowledgeReferenceBinding } from '@/api/nativeRecordReferences'
import { referenceDestinationError } from '@/api/nativeReferenceNavigation'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { nativeKnowledgeApi, type ImportedKnowledgeEnvelope, type ImportedKnowledgeHeadsEnvelope, type ImportedKnowledgeSource } from '@/api/nativeKnowledge'
import { importedKnowledgeHref, importedSourceKey } from '@/api/nativeRecordNavigation'
import { stringifyNativeJson } from '@/api/nativeJson'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { FactList, RecordLink, WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSection } from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { NativeKnowledgeDocument } from './NativeKnowledgeDocument'
import { NativeKnowledgeSearch } from './NativeKnowledgeSearch'

export function ImportedKnowledge({ source: capturedSource, slug }: { source: ImportedKnowledgeSource | null; slug: string | null }) {
  const [source] = useState(capturedSource)
  const navigate = useNavigate()
  const location = useLocation()
  const [contexts, setContexts] = useState<ImportedKnowledgeSource[] | null>(null)
  const [contextsError, setContextsError] = useState('')
  const [heads, setHeads] = useState<ImportedKnowledgeHeadsEnvelope['native'] | null>(null)
  const [headsError, setHeadsError] = useState('')
  const [page, setPage] = useState<{ slug: string; value: ImportedKnowledgeEnvelope } | null>(null)
  const [pageError, setPageError] = useState<{ slug: string; message: string } | null>(null)
  const contextsController = useRef<AbortController | null>(null)
  const pageController = useRef<AbortController | null>(null)
  const headsController = useRef<AbortController | null>(null)
  const loadHeads = useCallback(async () => {
    headsController.current?.abort()
    if (!source) return
    const controller = new AbortController(); headsController.current = controller
    setHeads(null); setHeadsError('')
    try {
      const result = await nativeKnowledgeApi.importedHeads(source, controller.signal)
      if (!controller.signal.aborted) setHeads(result.native)
    } catch (cause) {
      if (!controller.signal.aborted) setHeadsError(cause instanceof Error ? cause.message : String(cause))
    }
  }, [source])
  const loadContexts = useCallback(async () => {
    contextsController.current?.abort()
    const controller = new AbortController(); contextsController.current = controller
    setContexts(null); setContextsError('')
    try {
      const result = await nativeKnowledgeApi.importedContexts(controller.signal)
      if (!controller.signal.aborted) setContexts(result.contexts)
    } catch (cause) {
      if (!controller.signal.aborted) setContextsError(cause instanceof Error ? cause.message : String(cause))
    }
  }, [])
  const loadPage = useCallback(async () => {
    pageController.current?.abort()
    if (!source || slug === null) return
    const controller = new AbortController(); pageController.current = controller
    setPage(null); setPageError(null)
    try {
      const value = await nativeKnowledgeApi.importedPage(source, slug, controller.signal)
      if (!controller.signal.aborted) setPage({ slug, value })
    } catch (cause) {
      if (!controller.signal.aborted) setPageError({ slug, message: cause instanceof Error ? cause.message : String(cause) })
    }
  }, [source, slug])
  useEffect(() => {
    let cancelled = false
    queueMicrotask(() => { if (!cancelled) void loadContexts() })
    return () => { cancelled = true; contextsController.current?.abort() }
  }, [loadContexts])
  useEffect(() => {
    let cancelled = false
    queueMicrotask(() => { if (!cancelled) void loadPage() })
    return () => { cancelled = true; pageController.current?.abort() }
  }, [loadPage])
  useEffect(() => {
    let cancelled = false
    queueMicrotask(() => { if (!cancelled) void loadHeads() })
    return () => { cancelled = true; headsController.current?.abort() }
  }, [loadHeads])
  usePageRefresh(async () => { await Promise.all([loadContexts(), loadHeads(), loadPage()]) })
  const sourceKey = importedSourceKey(source)
  const available = contexts?.some(value => importedSourceKey(value) === sourceKey)
  const currentEnvelope = page?.slug === slug ? page.value : null
  const current = currentEnvelope?.native ?? null
  const referenceBinding = currentEnvelope ? knowledgeReferenceBinding(null, currentEnvelope.source, currentEnvelope.focused_source) : null
  const destinationError = referenceDestinationError(location.search, referenceBinding, current?.state === 'published' && current.page.state === 'found')
  const error = pageError?.slug === slug ? pageError.message : ''
  const openPage = (target: string) => { if (source) navigate(importedKnowledgeHref(source, target)) }
  return <WorkPage className="knowledge-library">
    <WorkHeader title="Imported Knowledge" description="Browse and read published pages from installed Knowledge artifacts." actions={<button className="work-button" onClick={() => navigate('/knowledge/explorer')}>Project Knowledge</button>} />
    <WorkSection title="Installed artifacts">
      {contextsError ? <WorkNotice title="Unable to read installed artifacts" tone="error">{contextsError}<button className="work-button" onClick={() => void loadContexts()}>Retry artifacts</button></WorkNotice>
        : !contexts ? <LoadingSpinner label="Loading installed artifacts…" />
        : <>
          {contexts.length === 0 && <WorkEmpty title="No Knowledge artifacts are installed" />}
          <label className="work-field">Artifact<StyledSelect aria-label="Imported Knowledge artifact" value={source ? sourceKey : ''} onChange={event => {
            const selected = contexts.find(value => importedSourceKey(value) === event.target.value)
            if (selected) navigate(importedKnowledgeHref(selected))
          }}><option value="" disabled>Select an artifact</option>{source && !available && <option value={sourceKey}>{source.artifact.id} @ {source.artifact.version} (not in inventory)</option>}{contexts.map(value => <option key={importedSourceKey(value)} value={importedSourceKey(value)}>{value.artifact.id} @ {value.artifact.version}{value.artifact.context_alias === undefined ? '' : ' · ' + value.artifact.context_alias}</option>)}</StyledSelect></label>
        </>}
      {source && <><FactList items={[["Artifact", source.artifact.id], ["Version", source.artifact.version], ["Context alias", source.artifact.context_alias]]} />
        <ImportedPageForm key={slug} slug={slug} read={openPage} />
        <details className="work-disclosure"><summary>Exact imported source</summary><pre className="work-code">{stringifyNativeJson(source, 2)}</pre></details></>}
    </WorkSection>
    {source && <NativeKnowledgeSearch scope={{ kind: 'imported', source }} openPage={openPage} />}
    {source && <WorkSection title="Published imported pages">
      {headsError ? <WorkNotice title="Unable to read imported library" tone="error">{headsError}<button className="work-button" onClick={() => void loadHeads()}>Retry imported library</button></WorkNotice>
        : !heads ? <LoadingSpinner label="Loading imported pages…" />
        : heads.state === 'registry_absent' ? <WorkEmpty title="Imported library is not registered" />
        : <><FactList items={[["Publication version", heads.generation.version], ["Physical branch", heads.generation.physical_branch], ["Inherited target", heads.generation.inherited_target]]} />
          {heads.heads.length === 0 ? <WorkEmpty title="The imported library is empty" /> : <nav aria-label="Published imported Knowledge pages">{heads.heads.map(head => <RecordLink key={head.slug} title={head.title || head.slug} meta={head.slug} selected={slug === head.slug} onClick={() => openPage(head.slug)}><span>{head.updated_at ?? '—'}</span></RecordLink>)}</nav>}</>}
    </WorkSection>}
    {!source ? <WorkEmpty title="Select an installed artifact" /> : slug === null ? null
      : <article className="knowledge-reader" aria-label="Imported Knowledge document">
        {destinationError ? <WorkNotice title="Reference destination unavailable" tone="error">{destinationError}</WorkNotice> : error ? <WorkNotice title="Unable to read imported document" tone="error">{error}<button className="work-button" onClick={() => void loadPage()}>Retry document</button></WorkNotice>
          : !current ? <LoadingSpinner label="Loading imported Knowledge document…" />
          : current.state === 'registry_absent' ? <WorkEmpty title="Knowledge artifact is not registered">{source.artifact.id} @ {source.artifact.version}</WorkEmpty>
          : current.page.state === 'published_empty' ? <WorkEmpty title="The Knowledge publication is empty" />
          : current.page.state === 'page_absent' ? <WorkEmpty title="Knowledge page not found">{slug}</WorkEmpty>
          : <><NativeKnowledgeDocument key={slug} publication={current} openPage={openPage} /><RecordReferences binding={referenceBinding} /></>}
      </article>}
  </WorkPage>
}

function ImportedPageForm({ slug, read }: { slug: string | null; read: (slug: string) => void }) {
  const [value, setValue] = useState(slug ?? '')
  return <form className="work-actions" onSubmit={event => { event.preventDefault(); read(value) }}><label className="work-field">Page slug<input aria-label="Imported Knowledge page slug" value={value} onChange={event => setValue(event.target.value)} /></label><button className="work-button" type="submit">Read page</button></form>
}
