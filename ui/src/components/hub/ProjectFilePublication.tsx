import { useId, useState, type ReactNode } from 'react'
import { hubNative, type FileKind, type FilePublicationInput, type AuthoredSelection } from '@/api/hubNative'
import { FactList, WorkSection } from '@/components/shared/EngineeringUI'
import { ModalPortal } from '@/components/shared/ModalPortal'
import { scopeLabel, type HubContext } from './hubContext'
import { AuthoredResources } from './AuthoredResources'
import { useHubPublication } from './useHubPublication'
import { PublicationMetadataFields } from './PublicationMetadata'
import { emptyPublicationMetadata, type PublicationMetadataDraft } from './publicationDraft'

export function ProjectFilePublication({ context, scopeKind, onInstall, installDisabled, installReason }: { context: HubContext; scopeKind: string; onInstall: (resource: AuthoredSelection) => void; installDisabled: boolean; installReason: string }) {
  const key = JSON.stringify([context.key, scopeKind])
  const [dialog, setDialog] = useState<{ key: string; source: AuthoredSelection & { artifact_type: FileKind } } | null>(null)
  const { pending, blocked, publish, feedback } = useHubPublication<FilePublicationInput>()
  if (dialog && dialog.key !== key) setDialog(null)
  const provider = context.identity.state === 'authenticated' ? context.identity.provider : ''
  const project = context.project?.kind === 'project' ? context.project : null

  return <>
    {project ? <AuthoredResources key={key} project={{ project_id: project.project_id, project_version: project.project_version }} publishingDisabled={!provider || pending || blocked} onInstall={onInstall} installDisabled={installDisabled} installReason={installReason} onPublish={source => {
      const artifact_type = source.artifact_type
      if (artifact_type === 'ast' || artifact_type === 'knowledge') return
      setDialog({ key, source: { ...source, artifact_type } })
    }} /> : <WorkSection title="Project resources"><p>Choose an available project and data version in the header to inspect maintained resources.</p></WorkSection>}
    {blocked && <p>Inspect the original publication outcome before another operation.</p>}
    {dialog?.key !== key && feedback}
    {dialog?.key === key && project && <PublicationDialog project={project} source={dialog.source} provider={provider} pending={pending} blocked={blocked} onSubmit={input => publish(input, hubNative.publish)} onClose={() => setDialog(null)}>{feedback}</PublicationDialog>}
  </>
}
type Draft = PublicationMetadataDraft & { artifact_type: FileKind; id: string; version: string }
const emptyDraft = (): Draft => ({ ...emptyPublicationMetadata(), artifact_type: 'skill', id: '', version: '' })
function PublicationDialog({ project, source, provider, pending, blocked, onSubmit, onClose, children }: { source: AuthoredSelection & { artifact_type: FileKind }; project: Extract<NonNullable<HubContext['project']>, {kind:'project'}>; provider: string; pending: boolean; blocked: boolean; onSubmit: (input: FilePublicationInput) => Promise<boolean>; onClose: () => void; children: ReactNode }) {
  const title = useId(), [draft, setDraft] = useState<Draft>(() => ({ ...emptyDraft(), artifact_type: source.artifact_type, id: source.id, version: source.version })), [review, setReview] = useState(false)
  const patch = (values: Partial<Draft>) => setDraft(previous => ({ ...previous, ...values }))
  const [validation, setValidation] = useState('')
  async function submit(event: React.FormEvent) {
    event.preventDefault()
    if (pending || blocked) return
    if (!draft.id || !draft.version || !draft.name || draft.version.toLowerCase() === 'latest') { setValidation('Enter an artifact ID, name and concrete version. Latest is a catalogue pointer.'); return }
    setValidation('')
    if (!review) { setReview(true); return }
    const publication: FilePublicationInput['publication'] = { provider, artifact_type: draft.artifact_type, id: draft.id, version: draft.version, name: draft.name, set_latest: draft.set_latest,
      ...(draft.description ? { description: draft.description } : {}), ...(draft.author ? { author: draft.author } : {}),
      ...(draft.tags ? { tags: draft.tags.split('\n') } : {}), ...(draft.expected_sha256 ? { expected_sha256: draft.expected_sha256 } : {}),
      ...(draft.dependencies !== undefined ? { dependencies: draft.dependencies } : {}) }
    if (await onSubmit({ project_id: project.project_id, project_version: project.project_version, publication })) onClose()
  }
  return <ModalPortal onClose={onClose}><div className="work-modal-backdrop"><form className="work-dialog" role="dialog" aria-modal="true" aria-labelledby={title} onSubmit={submit}>
    <header><h2 id={title}>{review ? 'Review project publication' : 'Publish project resources'}</h2><button type="button" className="work-button" onClick={onClose}>Close</button></header>
    <div className="work-dialog-body work-form">
      <FactList items={[[ 'Source project', scopeLabel(project) ], ['Broker provider', provider], ['Resource type', source.artifact_type], ['Resource ID', source.id], ['Resource artifact version', source.version]]} />
      {review ? <><FactList items={[[ 'Artifact', draft.artifact_type + ' / ' + draft.id ], ['Concrete version', draft.version], ['Name', draft.name], ['Description', draft.description], ['Author', draft.author], ['Tags', draft.tags], ['Expected archive SHA256', draft.expected_sha256], ['Latest pointer', draft.set_latest ? 'Set to this concrete version' : 'Preserve current pointer'], ['Dependencies', draft.dependencies === undefined ? 'Preserve current dependencies' : draft.dependencies.length ? draft.dependencies.map(value => value.artifact_type + ' / ' + value.id + ' @ ' + value.selector).join('\n') : 'Clear dependencies']]} /><p>Publication captures the current selected resource tree. Earlier reads do not freeze it; missing or changed resources can fail. Publication may leave confirmed or uncertain effects if final delivery fails.</p></> : <>
        <PublicationMetadataFields draft={draft} patch={patch} />
      </>}
      {validation && <p role="alert">{validation}</p>}
      {children}
    </div>
    <footer>{review && <button type="button" className="work-button" disabled={pending} onClick={() => setReview(false)}>Edit metadata</button>}<button type="button" className="work-button" onClick={onClose}>Cancel</button><button className="work-button primary" disabled={pending || blocked}>{pending ? 'Publishing…' : review ? 'Publish artifact' : 'Review publication'}</button></footer>
  </form></div></ModalPortal>
}
