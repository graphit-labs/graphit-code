import { useEffect, useRef, useState, type ReactNode } from 'react'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { FactList, WorkBadge, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { fileKinds, type CatalogArtifact, type FileKind, type FileOperation, type HubScope, type HubSource, type InstalledFile, type NativeBinding } from '@/api/hubNative'
import { bindingLabel, scopeLabel } from './hubContext'
import { HubContent } from './HubContent'

type Common = { canAct: boolean; unavailableReason: string; onClose: () => void }
type Props = Common & ({ kind: 'catalog'; entry: CatalogArtifact; source: HubSource; producer: string; onInstall: (version: string) => void; canUnpublish: boolean; unpublishReason: string; onUnpublish: (version: string | null) => void } | { kind: 'installed'; entry: InstalledFile; scope: HubScope; binding: NativeBinding | null; onAction: (operation: FileOperation) => void })
function Inspector({ title, type, description, onClose, children }: { title: string; type: string; description: string | null; onClose: () => void; children: ReactNode }) {
  const region = useRef<HTMLElement>(null)
  useEffect(() => { region.current?.focus() }, [])
  return <article className="artifact-inspector" ref={region} tabIndex={-1} aria-label="Artifact details" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onClose() } }}>
    <header><WorkBadge>{type}</WorkBadge><h2>{title}</h2>{description ? <div className="markdown-preview"><MarkdownContent content={description} /></div> : <p>No description supplied.</p>}<button className="work-button" onClick={onClose}>Close details</button></header>{children}
  </article>
}
export function ArtifactCard(props: Props) {
  return props.kind === 'catalog' ? <CatalogInspector {...props} /> : <InstalledInspector {...props} />
}
function CatalogInspector({ entry, source, producer, canAct, unavailableReason, onInstall, onClose, canUnpublish, unpublishReason, onUnpublish }: Extract<Props, { kind: 'catalog' }>) {
  const [version, setVersion] = useState('')
  const file = fileKinds.includes(entry.artifact_type as FileKind)
  const remote = { ...source, producer, artifact_type: entry.artifact_type as FileKind, id: entry.id, version, exact: true }
  return <Inspector title={entry.name} type={entry.artifact_type} description={entry.description} onClose={onClose}>
    <WorkSection title="Identity and origin"><FactList items={[[ 'Identifier',entry.id ],['Producer project',producer],['Author',entry.author],['Source',source.transport],['Provider',source.provider],['Profile',source.transport === 'standalone' ? source.profile : null],['Declared latest',entry.latest]]} /></WorkSection>
    <div className="work-actions">{entry.tags.map(tag => <WorkBadge key={tag}>{tag}</WorkBadge>)}</div>
    <WorkSection title="Published version"><label className="work-field"><span>Exact version</span><StyledSelect value={version} onChange={event => setVersion(event.target.value)}><option value="">Choose a published version</option>{entry.versions.map(item => <option key={item}>{item}</option>)}</StyledSelect></label>
      {file ? <><button className="work-button primary" disabled={!version || !canAct} onClick={() => onInstall(version)}>Install artifact</button>{!canAct && <p>{unavailableReason}</p>}</> : <WorkNotice title="Installation unavailable here">This page installs FILE artifacts. Installed code and knowledge libraries have their separate read-only inventory.</WorkNotice>}
    </WorkSection>
    <WorkSection title="Unpublish from remote catalogue"><p>Retract a version or explicitly remove the whole published artifact. Payloads, source and existing installations are preserved.</p><div className="work-actions"><button className="work-button danger" disabled={!version || !canUnpublish} onClick={() => onUnpublish(version)}>Unpublish version</button><button className="work-button danger" disabled={!canUnpublish} onClick={() => onUnpublish(null)}>Unpublish artifact</button></div>{!canUnpublish && <p>{unpublishReason}</p>}</WorkSection>
    {file && version && <HubContent key={JSON.stringify(remote)} remote={remote} />}
  </Inspector>
}
function InstalledInspector({ entry, scope, binding, canAct, unavailableReason, onAction, onClose }: Extract<Props, { kind: 'installed' }>) {
  const target = entry.target.kind === 'global_cache' ? 'Installation cache' : entry.target.kind === 'native_binding' ? bindingLabel(entry.target.binding) : 'Shared adapter: ' + entry.target.adapter
  const remote = entry.source_origin === 'HubCache' && entry.origin !== null
  const installed = { scope, artifact_type: entry.artifact_type, id: entry.id, version: entry.version, ...(scope.kind === 'project' && binding ? { binding } : {}) }
  return <Inspector title={entry.display?.name || entry.id} type={entry.artifact_type} description={entry.display?.description || null} onClose={onClose}>
    <WorkSection title="Identity and origin"><FactList items={[[ 'Identifier',entry.id ],['Installed version',entry.version],['Producer project',entry.producer],['Scope',scopeLabel(scope)],['Target',target],['Source',entry.source_origin],['Remote transport',entry.origin?.transport],['Provider',entry.origin?.provider],['Profile',entry.origin?.transport === 'standalone' ? entry.origin.profile : null],['Alias',entry.alias],['Entry name',entry.entry_name]]} /></WorkSection>
    <div className="work-actions">{entry.display?.tags.map(tag => <WorkBadge key={tag}>{tag}</WorkBadge>)}</div>
    <WorkSection title="Manage installation"><div className="work-actions">
      {remote && <button className="work-button" disabled={!canAct} onClick={() => onAction('update')}>Update artifact</button>}
      <button className="work-button danger" disabled={!canAct} onClick={() => onAction('remove')}>Remove artifact</button>
      {scope.kind === 'project' && <><button className="work-button" disabled={!canAct} onClick={() => onAction('recover')}>Recover target</button><button className="work-button danger" disabled={!canAct} onClick={() => onAction('forget')}>Forget logical ownership</button></>}
    </div>{!canAct && <p>{unavailableReason}</p>}</WorkSection>
    {remote && <HubContent key={JSON.stringify(installed)} installed={installed} />}
  </Inspector>
}
