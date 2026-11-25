import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { hubNative, installedFileKey, sourceSelection, fileKinds, isLocalInstallKind, type FileKind, type HubScope, type InstalledFile, type InstalledFilePage } from '@/api/hubNative'
import { WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSection, RecordLink } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { ArtifactCard } from './ArtifactCard'
import { HubActions } from './HubActions'
import { ProjectFilePublication } from './ProjectFilePublication'
import { HubFailure } from './HubFeedback'
import { scopeLabel, useHubContext, type HubContext } from './hubContext'
import './hub-native.css'

type Filters = { artifact_type?: FileKind; id?: string; version?: string; offset: number; limit: number }
export default function ProjectArtifactsPage() {
  const context = useHubContext()
  if (context.identity.state !== 'authenticated') return <WorkPage><WorkHeader title="Installed artifacts" description="Inspect FILE installations and maintain their exact targets." /><WorkNotice title={context.identity.state === 'loading' ? 'Loading identity' : context.identity.state === 'unavailable' ? 'Identity unavailable' : 'Sign in to inspect installations'}>Use the account control in the header.</WorkNotice></WorkPage>
  return <InstalledWorkspace key={context.identityKey} context={context} />
}
function InstalledWorkspace({ context }: { context: HubContext }) {
  const [scopeKind, setScopeKind] = useState<'project' | 'global_cache'>('project')
  const scope = useMemo<HubScope | null>(() => scopeKind === 'global_cache' ? { kind: 'global_cache' } : context.project, [scopeKind,context.project])
  const [kind, setKind] = useState(''), [id, setId] = useState(''), [version, setVersion] = useState('')
  const [filters, setFilters] = useState<Filters>({ offset: 0, limit: 20 })
  const [page, setPage] = useState<InstalledFilePage | null>(null), [error, setError] = useState<unknown>(null)
  const [loading, setLoading] = useState(true), [selected, setSelected] = useState('')
  const pending = useRef<AbortController | null>(null), lastSelection = useRef<HTMLElement | null>(null)
  const ready = scope !== null && (scope.kind === 'global_cache' || context.binding !== null)
  const load = useCallback(async () => {
    pending.current?.abort()
    if (!scope || (scope.kind === 'project' && !context.binding)) return
    const request = new AbortController(); pending.current = request
    setLoading(true); setError(null); setPage(null)
    try {
      const result = await hubNative.installed({ ...filters, scope, ...(scope.kind === 'project' && context.binding ? { binding: context.binding } : {}) }, request.signal)
      if (!request.signal.aborted) setPage(result)
    } catch (cause) { if (!request.signal.aborted) setError(cause) }
    finally { if (!request.signal.aborted) setLoading(false) }
  }, [scope,context.binding,filters])
  useEffect(() => { let current = true; queueMicrotask(() => { if (current) void load() }); return () => { current = false; pending.current?.abort() } }, [load])
  usePageRefresh(load)
  const selectionKey = (entry: InstalledFile) => JSON.stringify([scope, scope?.kind === 'project' ? context.binding : null, installedFileKey(entry)])
  const chosen = page?.entries.find(entry => selectionKey(entry) === selected)
  function change(next: Filters) { setPage(null); setError(null); setLoading(true); setFilters(next) }
  const close = () => { setSelected(''); lastSelection.current?.focus() }
  return <WorkPage className="hub-native"><WorkHeader title="Installed artifacts" description="Review concrete FILE versions, provenance and the effects of local maintenance." />
    <HubActions contextKey={JSON.stringify([context.key,scopeKind])} onCompleted={load}>{actions => <>
    <ProjectFilePublication context={context} scopeKind={scopeKind} installDisabled={actions.disabled || scope?.kind !== 'project' || !context.binding} installReason={actions.reason || (scope?.kind !== 'project' ? 'Choose Project selected in the header as the installation scope.' : 'Choose an available project and agent binding in the header.')} onInstall={source => {
      const artifact_type = source.artifact_type
      if (!isLocalInstallKind(artifact_type) || scope?.kind !== 'project' || !context.binding) return
      actions.open({ operation: 'install_local', request: { source: { ...source, artifact_type }, scope, binding: context.binding } })
    }} />
    <WorkSection title="Installation scope"><label className="work-field"><span>Inspect installations in</span><StyledSelect value={scopeKind} onChange={event => { setPage(null); setError(null); setSelected(''); setLoading(true); setScopeKind(event.target.value as typeof scopeKind); setFilters({ ...filters, offset: 0 }) }}><option value="project">Project selected in the header</option><option value="global_cache">Installation cache</option></StyledSelect></label>{scope && <p>{scopeLabel(scope)}</p>}<button type="button" className="work-button" disabled={!ready || actions.disabled} onClick={() => {
      if (scope?.kind === 'global_cache') actions.open({ operation: 'update_all', request: { scope } })
      else if (scope?.kind === 'project' && context.binding) actions.open({ operation: 'update_all', request: { scope, binding: context.binding } })
    }}>Update all remote installations</button><p>Updates eligible remote installations throughout this scope, across all filters and pages.</p></WorkSection>
    <form className="hub-source" onSubmit={event => { event.preventDefault(); change({ offset: 0, limit: 20, ...(kind ? { artifact_type: kind as FileKind } : {}), ...(id.trim() ? { id: id.trim() } : {}), ...(version.trim() ? { version: version.trim() } : {}) }) }}>
      <label className="work-field"><span>Artifact type</span><StyledSelect value={kind} onChange={event => setKind(event.target.value)}><option value="">All FILE types</option>{fileKinds.map(item => <option key={item}>{item}</option>)}</StyledSelect></label>
      <label className="work-field"><span>Exact artifact ID (optional)</span><input value={id} onChange={event => setId(event.target.value)} /></label>
      <label className="work-field"><span>Installed version (optional)</span><input value={version} onChange={event => setVersion(event.target.value)} /></label><button className="work-button" disabled={!ready}>Apply filters</button>
    </form>
      {!ready ? <WorkEmpty title={context.versionLoading ? 'Loading the current project version' : 'Choose an available installation scope'}>{context.projectUnavailable}</WorkEmpty> : loading ? <p role="status">Loading installed artifacts…</p> : error ? <HubFailure error={error} /> : page && <>
        {!page.entries.length ? <WorkEmpty title={filters.id || filters.version || filters.artifact_type ? 'No installations match these filters' : 'No installed FILE artifacts'}>Choose another scope or inspect a published artifact in the Hub registry.</WorkEmpty> : <div className="artifact-directory"><section className="hub-directory" aria-label="Installed artifact list">{page.entries.map(entry => <div key={selectionKey(entry)} onFocusCapture={event => { lastSelection.current = event.target as HTMLElement }}><RecordLink title={entry.display?.name || entry.id} meta={entry.id + '@' + entry.version + ' · ' + entry.artifact_type + ' · ' + entry.source_origin} selected={selected === selectionKey(entry)} onClick={() => setSelected(selectionKey(entry))} /></div>)}</section>
          {chosen && scope ? <ArtifactCard key={installedFileKey(chosen)} kind="installed" entry={chosen} scope={scope} binding={context.binding} onClose={close} canAct={!actions.disabled} unavailableReason={actions.reason} onAction={operation => {
            const binding = scope.kind === 'project' && context.binding ? { binding: context.binding } : {}
            if (operation === 'update' && chosen.source_origin === 'HubCache' && chosen.origin) actions.open({ operation, request: { selection: { ...sourceSelection(chosen.origin), producer: chosen.producer, artifact_type: chosen.artifact_type, id: chosen.id, version: chosen.version, exact: true }, scope, ...binding } })
            else actions.open({ operation, request: { scope, artifact_type: chosen.artifact_type, id: chosen.id, version: chosen.version, ...binding } })
          }} /> : <WorkEmpty title="Inspect an installation">Choose an exact version to see its producer, target and available maintenance actions.</WorkEmpty>}
        </div>}
        <nav className="hub-pagination" aria-label="Installation pages"><button className="work-button" disabled={filters.offset === 0} onClick={() => { setSelected(''); change({ ...filters, offset: Math.max(0, filters.offset - filters.limit) }) }}>Previous page</button><span>{page.entries.length ? 'Showing ' + (filters.offset + 1) + '–' + (filters.offset + page.entries.length) : 'No entries on this page'}</span><button className="work-button" disabled={page.next_offset === null} onClick={() => { if (page.next_offset !== null) { setSelected(''); change({ ...filters, offset: page.next_offset }) } }}>Next page</button></nav>
      </>}
    </>}</HubActions>
  </WorkPage>
}
