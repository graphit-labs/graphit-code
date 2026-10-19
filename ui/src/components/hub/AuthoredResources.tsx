import { useEffect, useMemo, useRef, useState } from 'react'
import { artifactKinds, fileKinds, isLocalInstallKind, hubNative, HubRequestError, type ArtifactKind, type AuthoredContent, type AuthoredPage, type AuthoredProject, type AuthoredQuery, type AuthoredResource, type AuthoredReview, type AuthoredSelection } from '@/api/hubNative'
import { FactList, RecordLink, WorkEmpty, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { HubFailure } from './HubFeedback'

function ReadFailure({ error }: { error: unknown }) {
  if (error instanceof HubRequestError && error.code === 'authored_content_not_text') return <WorkNotice title="This file is not text">Choose another file in the resource tree.</WorkNotice>
  if (error instanceof HubRequestError && error.code === 'authored_too_large') return <WorkNotice title="The resource exceeds its read limit">Choose a smaller resource or file. The response was not presented as a complete read.</WorkNotice>
  if (error instanceof HubRequestError && error.code === 'authored_file_not_found') return <WorkNotice title="The resource file is no longer available">Refresh the current resource before reviewing it again.</WorkNotice>
  return <HubFailure error={error} />
}

export function AuthoredResources({ project, onPublish, publishingDisabled, onInstall, installDisabled, installReason }: { project: AuthoredProject; onPublish: (resource: AuthoredSelection) => void; publishingDisabled: boolean; onInstall: (resource: AuthoredSelection) => void; installDisabled: boolean; installReason: string }) {
  const [opened, setOpened] = useState(false)
  return <WorkSection title="Project resources">
    <p>Browse maintained Resources in the project and data version selected in the header. Inspect a resource and its files before reviewing a publication.</p>
    <button type="button" className="work-button" onClick={() => setOpened(value => !value)} aria-expanded={opened}>{opened ? 'Close project resources' : 'Browse project resources'}</button>
    {opened && <ResourceBrowser project={project} onPublish={onPublish} publishingDisabled={publishingDisabled} onInstall={onInstall} installDisabled={installDisabled} installReason={installReason} />}
  </WorkSection>
}

function ResourceBrowser({ project, onPublish, publishingDisabled, onInstall, installDisabled, installReason }: { project: AuthoredProject; onPublish: (resource: AuthoredSelection) => void; publishingDisabled: boolean; onInstall: (resource: AuthoredSelection) => void; installDisabled: boolean; installReason: string }) {
  const [kind, setKind] = useState(''), [id, setId] = useState(''), [version, setVersion] = useState('')
  const [query, setQuery] = useState<AuthoredQuery>({ ...project, limit: 20 })
  const [page, setPage] = useState<AuthoredPage | null>(null), [error, setError] = useState<unknown>(null), [loading, setLoading] = useState(true)
  const [selected, setSelected] = useState<AuthoredResource | null>(null)
  const request = useRef<AbortController | null>(null), trigger = useRef<HTMLElement | null>(null)
  useEffect(() => {
    const controller = new AbortController(); request.current = controller
    void hubNative.authoredList(query, controller.signal).then(value => { if (!controller.signal.aborted) setPage(value) }, cause => { if (!controller.signal.aborted) setError(cause) }).finally(() => { if (!controller.signal.aborted) setLoading(false) })
    return () => controller.abort()
  }, [query])
  function load(next: AuthoredQuery) {
    request.current?.abort(); setSelected(null); setPage(null); setError(null); setLoading(true); setQuery(next)
  }
  const selection = useMemo(() => selected ? { project_id: project.project_id, project_version: project.project_version, artifact_type: selected.artifact_type, id: selected.id, version: selected.version } : null, [project.project_id, project.project_version, selected])
  const identity = (entry: AuthoredResource) => JSON.stringify([entry.artifact_type, entry.id, entry.version])
  return <div className="hub-authored">
    <form className="hub-source" onSubmit={event => { event.preventDefault(); load({ ...project, limit: 20, ...(kind ? { artifact_type: kind as ArtifactKind } : {}), ...(id ? { id } : {}), ...(version ? { version } : {}) }) }}>
      <label className="work-field"><span>Resource type</span><StyledSelect value={kind} onChange={event => setKind(event.target.value)}><option value="">All resource types</option>{artifactKinds.map(type => <option key={type}>{type}</option>)}</StyledSelect></label>
      <label className="work-field"><span>Resource ID (optional)</span><input value={id} onChange={event => setId(event.target.value)} /></label>
      <label className="work-field"><span>Resource artifact version (optional)</span><input value={version} onChange={event => setVersion(event.target.value)} /></label>
      <button className="work-button">Find project resources</button>
    </form>
    <p>The inventory reflects current resources. Refresh from the first page to include new items before the current cursor.</p>
    {loading ? <p role="status">Loading project resources…</p> : error ? <ReadFailure error={error} /> : page && (!page.entries.length ? <WorkEmpty title={query.id || query.version || query.artifact_type ? 'No resources match these filters' : 'No maintained project resources'}>Choose another filter or refresh the current project.</WorkEmpty> : <div className="artifact-directory">
      <section className="hub-directory" aria-label="Project resource list">{page.entries.map(entry => <div key={identity(entry)} onFocusCapture={event => { trigger.current = event.target as HTMLElement }}><RecordLink title={entry.id} meta={entry.artifact_type + ' · ' + entry.version} selected={selected !== null && identity(selected) === identity(entry)} onClick={() => setSelected(entry)} /></div>)}</section>
      {selected && selection ? <ResourceReview key={identity(selected)} selection={selection} onClose={() => { setSelected(null); trigger.current?.focus() }} onPublish={onPublish} publishingDisabled={publishingDisabled} onInstall={onInstall} installDisabled={installDisabled} installReason={installReason} /> : <WorkEmpty title="Inspect a project resource">Select a concrete resource to read its tree and review publication options.</WorkEmpty>}
    </div>)}
    <nav className="hub-pagination" aria-label="Project resource pages"><button className="work-button" onClick={() => { const next = { ...query }; delete next.cursor; load(next) }}>Refresh project resources</button><button className="work-button" disabled={loading || !page?.next_cursor} onClick={() => { if (page?.next_cursor) load({ ...query, cursor: page.next_cursor }) }}>Next resource page</button></nav>
  </div>
}

function ResourceReview({ selection, onClose, onPublish, publishingDisabled, onInstall, installDisabled, installReason }: { selection: AuthoredSelection; onClose: () => void; onPublish: (resource: AuthoredSelection) => void; publishingDisabled: boolean; onInstall: (resource: AuthoredSelection) => void; installDisabled: boolean; installReason: string }) {
  const [query, setQuery] = useState({ offset: 0 }), [page, setPage] = useState<AuthoredReview | null>(null)
  const [error, setError] = useState<unknown>(null), [loading, setLoading] = useState(true)
  const [content, setContent] = useState<AuthoredContent | null>(null), [contentError, setContentError] = useState<unknown>(null), [reading, setReading] = useState(false)
  const review = useRef<AbortController | null>(null), read = useRef<AbortController | null>(null)
  const region = useRef<HTMLElement>(null), reader = useRef<HTMLElement>(null), trigger = useRef<HTMLButtonElement | null>(null)
  useEffect(() => { region.current?.focus(); return () => read.current?.abort() }, [])
  useEffect(() => { if (content) reader.current?.focus() }, [content])
  useEffect(() => {
    const controller = new AbortController(); review.current = controller
    void hubNative.authoredReview({ ...selection, offset: query.offset, limit: 20 }, controller.signal).then(value => { if (!controller.signal.aborted) setPage(value) }, cause => { if (!controller.signal.aborted) setError(cause) }).finally(() => { if (!controller.signal.aborted) setLoading(false) })
    return () => controller.abort()
  }, [selection, query])
  function load(next: number) {
    review.current?.abort(); read.current?.abort(); setContent(null); setContentError(null); setReading(false); setPage(null); setError(null); setLoading(true); setQuery({ offset: next })
  }
  async function readFile(path: string, button: HTMLButtonElement) {
    read.current?.abort(); const controller = new AbortController(); read.current = controller; trigger.current = button
    setContent(null); setContentError(null); setReading(true)
    try { const result = await hubNative.authoredContent({ ...selection, path }, controller.signal); if (!controller.signal.aborted) setContent(result) }
    catch (cause) { if (!controller.signal.aborted) setContentError(cause) }
    finally { if (!controller.signal.aborted) setReading(false) }
  }
  const publishable = fileKinds.some(kind => kind === selection.artifact_type)
  const installable = isLocalInstallKind(selection.artifact_type)
  return <article className="artifact-inspector" ref={region} tabIndex={-1} aria-label="Project resource details" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onClose() } }}>
    <header className="work-toolbar"><h3>{selection.id}</h3><button type="button" className="work-button" onClick={onClose}>Close resource</button></header>
    <FactList items={[[ 'Source', 'Resources' ], ['Source project', selection.project_id + ' @ ' + selection.project_version], ['Resource type', selection.artifact_type], ['Artifact version', selection.version]]} />
    {loading ? <p role="status">Loading resource tree…</p> : error ? <><ReadFailure error={error} /><button type="button" className="work-button" onClick={() => load(0)}>Refresh resource tree</button></> : page && <>
      <WorkNotice title="No source metadata descriptor">This resource supplies no publication metadata. Enter publication metadata explicitly when preparing a FILE publication.</WorkNotice>
      <WorkSection title="Resource tree">{page.entries.length ? <ul className="hub-resource-tree">{page.entries.map(entry => <li key={entry.path}>{entry.kind === 'File' ? <button type="button" className="work-button" onClick={event => { void readFile(entry.path, event.currentTarget) }}>Read {entry.path}</button> : <span>{entry.path} · Directory</span>} <span>{String(entry.bytes)} bytes</span></li>)}</ul> : <p>No entries on this resource tree page.</p>}
        <nav className="hub-pagination" aria-label="Resource tree pages"><button className="work-button" onClick={() => load(0)}>Refresh resource tree</button><button className="work-button" disabled={page.next_offset === null} onClick={() => { if (page.next_offset !== null) load(page.next_offset) }}>Next tree page</button></nav>
      </WorkSection>
      {publishable ? <button type="button" className="work-button primary" disabled={publishingDisabled} onClick={() => onPublish(structuredClone(selection))}>Publish project resources</button> : <WorkNotice title="Read-only resource here">AST and Knowledge resources can be inspected here. This form publishes FILE resources.</WorkNotice>}
      <WorkSection title="Local installation">{installable ? <><button type="button" className="work-button" disabled={installDisabled} onClick={() => onInstall(structuredClone(selection))}>Install local resource</button>{installDisabled && <p>{installReason}</p>}<p>Install this selected Resources version in the current project and agent binding. This does not publish it.</p></> : <p>Local installation supports agents, skills, commands and workflows. This resource remains available for inspection.</p>}</WorkSection>
      <p>Each read captures the current source independently. Review does not freeze the tree or grant publication access.</p>
    </>}
    {reading && <p role="status">Reading resource file…</p>}
    {contentError !== null && <ReadFailure error={contentError} />}
    {content && <section ref={reader} className="hub-file" tabIndex={-1} aria-label="Resource file content" onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setContent(null); trigger.current?.focus() } }}><p>{content.content.path} · {String(content.content.bytes)} bytes</p>{content.content.content === '' ? <p>This text file is empty.</p> : <pre><code>{content.content.content}</code></pre>}</section>}
  </article>
}
