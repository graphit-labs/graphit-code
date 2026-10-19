import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { hubNative, sourceSelection, type ArtifactKind, type ArtifactPage, type HubSource, type ProjectPage } from '@/api/hubNative'
import { WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSection, RecordLink } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { ArtifactCard } from './ArtifactCard'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { HubUnpublish } from './HubUnpublish'
import { HubActions } from './HubActions'
import { HubFailure } from './HubFeedback'
import { useHubContext, type HubContext } from './hubContext'
import './hub-native.css'

type Search = Parameters<typeof hubNative.search>[0]
export default function RegistryPage() {
  const context = useHubContext()
  if (context.identity.state !== 'authenticated') return <WorkPage><WorkHeader title="Hub registry" description="Inspect published artifacts and their exact versions." /><WorkNotice title={context.identity.state === 'loading' ? 'Loading identity' : context.identity.state === 'unavailable' ? 'Identity unavailable' : 'Sign in to browse the Hub'}>Use the account control in the header.</WorkNotice></WorkPage>
  return <RegistryWorkspace key={context.identityKey} context={context} provider={context.identity.provider} />
}
function RegistryWorkspace({ context, provider: authenticatedProvider }: { context: HubContext; provider: string }) {
  const [transport, setTransport] = useState<'broker' | 'standalone'>('broker')
  const [provider, setProvider] = useState(authenticatedProvider), [profile, setProfile] = useState('')
  const [producer, setProducer] = useState(''), [kind, setKind] = useState(''), [id, setId] = useState(''), [version, setVersion] = useState('')
  const [destination, setDestination] = useState<'project' | 'global_cache'>('project')
  const [publisherQuery, setPublisherQuery] = useState<(HubSource & { limit: number; cursor?: string }) | null>({ transport: 'broker', provider: authenticatedProvider, limit: 20 })
  const [publishers, setPublishers] = useState<ProjectPage | null>(null), [publisherError, setPublisherError] = useState<unknown>(null)
  const [publisherLoading, setPublisherLoading] = useState(true)
  const [query, setQuery] = useState<Search | null>(null), [page, setPage] = useState<ArtifactPage | null>(null)
  const [loading, setLoading] = useState(false), [error, setError] = useState<unknown>(null), [selected, setSelected] = useState('')
  const [previous, setPrevious] = useState<(string | undefined)[]>([])
  const publishersRequest = useRef<AbortController | null>(null), catalogRequest = useRef<AbortController | null>(null)
  const lastSelection = useRef<HTMLElement | null>(null)
  const publishersId = useId()
  const source = (nextTransport = transport, nextProvider = provider, nextProfile = profile): HubSource => nextTransport === 'broker' ? { transport: nextTransport, provider: authenticatedProvider } : { transport: nextTransport, provider: nextProvider.trim(), ...(nextProfile.trim() ? { profile: nextProfile.trim() } : {}) }
  function changeSource(nextTransport: typeof transport, nextProvider: string, nextProfile: string) {
    if (JSON.stringify(source(nextTransport, nextProvider, nextProfile)) !== JSON.stringify(source())) {
      publishersRequest.current?.abort()
      setPublisherQuery(null); setPublishers(null); setPublisherError(null); setPublisherLoading(false)
    }
    setTransport(nextTransport); setProvider(nextProvider); setProfile(nextProfile)
  }
  const loadPublishers = useCallback(async () => {
    publishersRequest.current?.abort()
    if (!publisherQuery) return
    const request = new AbortController(); publishersRequest.current = request
    setPublisherLoading(true); setPublisherError(null); setPublishers(null)
    try { const result = await hubNative.projects(publisherQuery, request.signal); if (!request.signal.aborted) setPublishers(result) }
    catch (cause) { if (!request.signal.aborted) setPublisherError(cause) }
    finally { if (!request.signal.aborted) setPublisherLoading(false) }
  }, [publisherQuery])
  const loadCatalog = useCallback(async () => {
    catalogRequest.current?.abort()
    if (!query) return
    const request = new AbortController(); catalogRequest.current = request
    setLoading(true); setError(null); setPage(null)
    try { const result = await hubNative.search(query, request.signal); if (!request.signal.aborted) setPage(result) }
    catch (cause) { if (!request.signal.aborted) setError(cause) }
    finally { if (!request.signal.aborted) setLoading(false) }
  }, [query])
  useEffect(() => { let current = true; queueMicrotask(() => { if (current) void loadPublishers() }); return () => { current = false; publishersRequest.current?.abort() } }, [loadPublishers, context.selectionKey])
  useEffect(() => { let current = true; queueMicrotask(() => { if (current) void loadCatalog() }); return () => { current = false; catalogRequest.current?.abort() } }, [loadCatalog, context.selectionKey])
  usePageRefresh(useCallback(async () => { await Promise.all([loadPublishers(), loadCatalog()]) }, [loadPublishers, loadCatalog]))
  const chosen = page?.artifacts.find(entry => entry.artifact_type + '/' + entry.id === selected)
  const scope = destination === 'global_cache' ? { kind: 'global_cache' as const } : context.project
  const targetReady = !!scope && (scope.kind === 'global_cache' || !!context.binding)
  const close = () => { setSelected(''); lastSelection.current?.focus() }
  function changeQuery(next: Search) { setPage(null); setError(null); setLoading(true); setQuery(next) }
  function search(event: React.FormEvent) {
    event.preventDefault()
    if (!producer.trim() || !source().provider) return
    setPrevious([]); setSelected('')
    changeQuery({ ...source(), producer: producer.trim(), limit: 20, ...(kind ? { artifact_type: kind as ArtifactKind } : {}), ...(id.trim() ? { id: id.trim() } : {}), ...(version.trim() ? { version: version.trim() } : {}) })
  }
  return <WorkPage className="hub-native"><WorkHeader title="Hub registry" description="Choose a publisher, inspect its versions and install an exact FILE artifact." />
    <WorkSection title="Published source"><form onSubmit={search}>
      <div className="hub-source">
        <label className="work-field"><span>Source</span><StyledSelect value={transport} onChange={event => changeSource(event.target.value as typeof transport, authenticatedProvider, profile)}><option value="broker">Broker</option><option value="standalone">Standalone storage</option></StyledSelect></label>
        <label className="work-field"><span>Provider</span><input required value={transport === 'broker' ? authenticatedProvider : provider} readOnly={transport === 'broker'} onChange={event => changeSource(transport, event.target.value, profile)} /></label>
        {transport === 'standalone' && <label className="work-field"><span>Storage profile (optional)</span><input value={profile} onChange={event => changeSource(transport, provider, event.target.value)} /></label>}
        <button type="button" className="work-button" disabled={publisherLoading || !source().provider} onClick={() => setPublisherQuery({ ...source(), limit: 20 })}>Browse publishers</button>
      </div>
      {publisherLoading ? <p role="status">Loading publishers…</p> : publisherError ? <HubFailure error={publisherError} /> : publishers && <>
        {!publishers.projects.length && <p>No publishers in this authorized page. You can supply an exact publisher ID.</p>}
        <datalist id={publishersId}>{publishers.projects.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</datalist>
        {publishers.next_cursor && publisherQuery && <button type="button" className="work-button" onClick={() => setPublisherQuery({ ...publisherQuery, cursor: publishers.next_cursor! })}>Next publisher page</button>}
      </>}
      <div className="hub-source">
        <label className="work-field"><span>Publisher project ID</span><input required list={publishersId} value={producer} onChange={event => setProducer(event.target.value)} /></label>
        <label className="work-field"><span>Artifact type</span><StyledSelect value={kind} onChange={event => setKind(event.target.value)}><option value="">All types</option>{['skill','command','workflow','agent','rule','mcp','language','power','ast','knowledge'].map(item => <option key={item}>{item}</option>)}</StyledSelect></label>
        <label className="work-field"><span>Exact artifact ID (optional)</span><input value={id} onChange={event => setId(event.target.value)} /></label>
        <label className="work-field"><span>Published version filter (optional)</span><input value={version} onChange={event => setVersion(event.target.value)} /></label>
        <button className="work-button primary">Find artifacts</button>
      </div>
    </form></WorkSection>
    <label className="work-field"><span>Installation destination</span><StyledSelect value={destination} onChange={event => setDestination(event.target.value as typeof destination)}><option value="project">Project selected in the header</option><option value="global_cache">Installation cache</option></StyledSelect></label>
    <HubUnpublish contextKey={JSON.stringify([source(), query && sourceSelection(query), query?.producer, selected])} onCompleted={loadCatalog}>{unpublish => <HubActions contextKey={JSON.stringify([context.key,destination])} onCompleted={loadCatalog}>{actions => <>
      {!query ? <WorkEmpty title="Choose a publisher">Find artifacts using its exact project ID. Published availability does not grant installation permission.</WorkEmpty> : loading ? <p role="status">Loading artifacts…</p> : error ? <><HubFailure error={error} /><button className="work-button" onClick={() => { setPrevious([]); changeQuery({ ...query, cursor: undefined }) }}>Restart from first page</button></> : page && <>
        <p>Published by {page.producer} through {query.provider}.</p>
        {!page.artifacts.length ? <WorkEmpty title={query.id || query.version || query.artifact_type ? 'No artifacts match these filters' : 'No published artifacts'}>Try another publisher or filter.</WorkEmpty> : <div className="artifact-directory"><section className="hub-directory" aria-label="Published artifact list">{page.artifacts.map(entry => <div key={entry.artifact_type + '/' + entry.id} onFocusCapture={event => { lastSelection.current = event.target as HTMLElement }}><RecordLink title={entry.name} meta={entry.artifact_type + ' · ' + entry.id} selected={selected === entry.artifact_type + '/' + entry.id} onClick={() => setSelected(entry.artifact_type + '/' + entry.id)} />{entry.description && <div className="hub-summary"><MarkdownContent content={entry.description} /></div>}</div>)}</section>
          {chosen ? <ArtifactCard key={chosen.artifact_type + '/' + chosen.id} kind="catalog" entry={chosen} producer={page.producer} source={sourceSelection(query)} onClose={close} canUnpublish={query.transport === 'broker' && query.provider === authenticatedProvider && !unpublish.disabled} unpublishReason={unpublish.reason || 'Unpublication requires the authenticated Broker source. Catalogue visibility does not grant write permission.'} onUnpublish={chosenVersion => unpublish.open({ provider: query.provider, producer: page.producer, artifact_type: chosen.artifact_type, id: chosen.id, target: chosenVersion === null ? { kind: 'artifact' } : { kind: 'version', version: chosenVersion } })} canAct={targetReady && !actions.disabled} unavailableReason={actions.disabled ? actions.reason : context.versionLoading ? 'Waiting for the current project version.' : context.projectUnavailable} onInstall={chosenVersion => { if (!scope) return; actions.open({ operation: 'install', versions: chosen.versions, request: { selection: { ...sourceSelection(query), producer: page.producer, artifact_type: chosen.artifact_type as import('@/api/hubNative').FileKind, id: chosen.id, version: chosenVersion, exact: true }, scope, ...(scope.kind === 'project' && context.binding ? { binding: context.binding } : {}) } }) }} /> : <WorkEmpty title="Inspect an artifact">Choose a result to review its source and published versions.</WorkEmpty>}
        </div>}
        <nav className="hub-pagination" aria-label="Artifact pages"><button className="work-button" disabled={!previous.length} onClick={() => { const history = previous.slice(); const cursor = history.pop(); setPrevious(history); setSelected(''); changeQuery({ ...query, cursor }) }}>Previous page</button><button className="work-button" disabled={!page.next_cursor} onClick={() => { if (!page.next_cursor) return; setPrevious([...previous,query.cursor]); setSelected(''); changeQuery({ ...query, cursor: page.next_cursor }) }}>Next page</button></nav>
      </>}
    </>}</HubActions>}</HubUnpublish>
  </WorkPage>
}
