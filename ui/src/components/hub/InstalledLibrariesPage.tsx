import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { installedLibraries, libraryKey, type InstalledLibraryPage, type LibraryKind, type LibraryQuery } from '@/api/installedLibraries'
import { NativeRecordReadError } from '@/api/nativeRecordRead'
import { useAppStore, type WebIdentity } from '@/store/appStore'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { FactList, RecordLink, WorkBadge, WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSearch, WorkSection, WorkTabs } from '@/components/shared/EngineeringUI'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import './installed-libraries.css'

export default function InstalledLibrariesPage() {
  const identity = useAppStore(state => state.webIdentity)
  const apiBase = useAppStore(state => state.apiBase)
  const key = JSON.stringify([apiBase, identity])
  return <Libraries key={key} identity={identity} />
}
function Libraries({ identity }: { identity: WebIdentity }) {
  const [kind, setKind] = useState('')
  const [id, setId] = useState(''), [version, setVersion] = useState('')
  const [query, setQuery] = useState<LibraryQuery>({ offset: 0, limit: 30 })
  const [page, setPage] = useState<InstalledLibraryPage | null>(null)
  const [loading, setLoading] = useState(identity.state === 'authenticated'), [error, setError] = useState<unknown>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const pending = useRef<AbortController | null>(null)
  const detail = useRef<HTMLElement>(null), lastSelection = useRef<HTMLElement | null>(null)
  const authorized = identity.state === 'authenticated'
  const load = useCallback(() => {
    pending.current?.abort()
    if (!authorized) return Promise.resolve()
    const request = new AbortController(); pending.current = request
    return installedLibraries(query, request.signal).then(response => {
      if (!request.signal.aborted) setPage(response)
    }, cause => {
      if (!request.signal.aborted) setError(cause)
    }).finally(() => {
      if (!request.signal.aborted) setLoading(false)
    })
  }, [authorized, query])
  useEffect(() => { void load(); const request = pending.current; return () => request?.abort() }, [load])
  const refresh = useCallback(async () => {
    setPage(null); setError(null); setLoading(true)
    await load()
  }, [load])
  usePageRefresh(refresh)
  function changeQuery(next: LibraryQuery) {
    setPage(null); setError(null); setLoading(true); setQuery(next)
  }
  const chosen = useMemo(() => page?.entries.find(entry => libraryKey(entry) === selected), [page, selected])
  useEffect(() => { if (chosen) detail.current?.focus() }, [chosen])
  const filtered = !!query.id || !!query.version || !!query.artifact_type
  const denied = error instanceof NativeRecordReadError && (error.status === 401 || error.status === 403)
  const close = () => { setSelected(null); lastSelection.current?.focus() }
  function apply(nextKind = kind) {
    changeQuery({ offset: 0, limit: 30, ...(nextKind ? { artifact_type: nextKind as LibraryKind } : {}), ...(id.trim() ? { id: id.trim() } : {}), ...(version.trim() ? { version: version.trim() } : {}) })
  }
  return <WorkPage className="installed-libraries">
    <WorkHeader title="Installed libraries" description="AST, Knowledge and language libraries installed for this installation. Open a library to inspect its exact version and origin." context={<WorkBadge>Installation · all projects</WorkBadge>} />
    <WorkTabs value={kind} onChange={value => { setKind(value); apply(value) }} label="Library type" items={[[ '', 'All libraries' ],['ast','AST'],['knowledge','Knowledge'],['language','Languages']]} />
    <form className="work-toolbar" onSubmit={event => { event.preventDefault(); apply() }}>
      <WorkSearch value={id} onChange={setId} label="Artifact identifier" placeholder="Exact artifact id, for example vendor/library" />
      <label className="library-version">Version<input value={version} onChange={event => setVersion(event.target.value)} placeholder="Any installed version" /></label>
      <button className="work-button" type="submit" disabled={!authorized}>Apply filters</button>
    </form>
    {identity.state === 'loading' ? <WorkNotice title="Loading identity">Waiting for the current Web session.</WorkNotice>
      : identity.state === 'unavailable' ? <WorkNotice title="Identity unavailable" tone="error">Use the account control to retry loading your identity.</WorkNotice>
      : !authorized ? <WorkNotice title="Sign in to inspect installed libraries">Use the account control in the header.</WorkNotice>
      : denied ? <WorkNotice title="Library access unavailable" tone="error">Your current session cannot read this installation. Check the account selected in the header.</WorkNotice>
      : error ? <WorkNotice title="Installed libraries could not be loaded" tone="error">Refresh from the header to try again.</WorkNotice>
      : loading ? <div role="status" className="library-loading">Loading installed libraries…</div>
      : page && !page.entries.length ? <WorkEmpty title={filtered ? 'No libraries match these filters' : 'No installed libraries'}>{filtered ? 'Try another type, identifier or version.' : 'Libraries installed in this installation will appear here.'}</WorkEmpty>
      : page && <>
        <div className="library-workspace">
          <section aria-label="Installed library list" className="library-list">
            {page.entries.map(entry => <div key={libraryKey(entry)} className="library-row" onFocusCapture={event => { lastSelection.current = event.target as HTMLElement }}>
              <RecordLink title={entry.display?.name || entry.id} meta={<><span>{entry.id}@{entry.version}</span><span>{entry.artifact_type}</span></>} selected={libraryKey(entry) === selected} onClick={() => setSelected(libraryKey(entry))} />
            </div>)}
          </section>
          {chosen ? <section className="library-detail" ref={detail} tabIndex={-1} aria-label="Library details" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); close() } }}>
            <WorkSection title={chosen.display?.name || chosen.id} actions={<button className="work-button" onClick={close} aria-label="Close library details">Close</button>}>
              <FactList items={[[ 'Identifier', chosen.id ],['Version',chosen.version],['Type',chosen.artifact_type],['Producer',chosen.producer],['Installed for','All projects in this installation'],['Source',({ HubCache: 'Published library', Resources: 'Project resources', LocalLink: 'Local project link' })[chosen.origin]],['Provider',chosen.provider],['Alias',chosen.alias],['Entry name',chosen.entry_name]]} />
              {chosen.display?.description && <div className="library-description"><MarkdownContent content={chosen.display.description} /></div>}
              {!!chosen.display?.tags.length && <div className="library-tags" aria-label="Artifact tags">{chosen.display.tags.map(tag => <WorkBadge key={tag}>{tag}</WorkBadge>)}</div>}
            </WorkSection>
          </section> : <WorkEmpty title="Inspect a library">Choose an installed library to see its version, producer and origin.</WorkEmpty>}
        </div>
        <nav className="library-pagination" aria-label="Library pages">
          <button className="work-button" disabled={query.offset === 0} onClick={() => changeQuery({ ...query, offset: Math.max(0, query.offset - query.limit) })}>Previous</button>
          <span>Showing {query.offset + 1}–{query.offset + page.entries.length}</span>
          <button className="work-button" disabled={page.next_offset === null} onClick={() => { if (page.next_offset !== null) changeQuery({ ...query, offset: page.next_offset }) }}>Next</button>
        </nav>
      </>}
  </WorkPage>
}
