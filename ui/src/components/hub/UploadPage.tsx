import { useEffect, useRef, useState } from 'react'
import { artifactKinds, hubNative, type ArtifactKind, type PackageUploadMetadata } from '@/api/hubNative'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { FactList, WorkHeader, WorkNotice, WorkPage, WorkSection } from '@/components/shared/EngineeringUI'
import { useHubContext, scopeLabel, type HubContext } from './hubContext'
import { PublicationMetadataFields } from './PublicationMetadata'
import { emptyPublicationMetadata, type PublicationMetadataDraft } from './publicationDraft'
import { useHubPublication } from './useHubPublication'

export default function UploadPage() {
  const context = useHubContext()
  if (context.identity.state !== 'authenticated') return <WorkPage><WorkHeader title="Publish an artifact" description="Publish a reusable package to the Hub." /><WorkNotice title={context.identity.state === 'loading' ? 'Loading identity' : context.identity.state === 'unavailable' ? 'Identity unavailable' : 'Sign in to publish artifacts'}>Use the account control in the header.</WorkNotice></WorkPage>
  return <PublicationWorkspace key={context.identityKey} context={context} provider={context.identity.provider} />
}
function PublicationWorkspace({ context, provider }: { context: HubContext; provider: string }) {
  const publication = useHubPublication<PackageUploadMetadata>()
  const [packageFacts, setPackageFacts] = useState<{ name: string; size: number } | null>(null)
  const [draftNumber, setDraftNumber] = useState(0)
  const project = context.project?.kind === 'project' ? context.project : null
  const draftKey = JSON.stringify([project?.project_id, project?.project_version, draftNumber])
  async function publish(metadata: PackageUploadMetadata, file: File) {
    setPackageFacts({ name: file.name, size: file.size })
    return publication.publish(metadata, (captured, signal) => hubNative.upload(captured, file, signal))
  }
  return <WorkPage>
    <WorkHeader title="Publish an artifact" description="Choose a package, describe its contract and review the publication." />
    {!project ? <WorkNotice title={context.versionLoading ? 'Loading Hub working contexts' : 'Choose a publication project'}>Choose an authorized project and data version in the header.</WorkNotice>
      : <PackageDraft key={draftKey} project={project} provider={provider} pending={publication.pending} blocked={publication.blocked} onSubmit={publish} onNew={() => setDraftNumber(value => value + 1)} />}
    {publication.result && <>
      {packageFacts && <FactList items={[[ 'Submitted package', packageFacts.name ], [ 'Package bytes', String(packageFacts.size) ]]} />}
      {publication.feedback}
    </>}
  </WorkPage>
}
function PackageDraft({ project, provider, pending, blocked, onSubmit, onNew }: {
  project: Extract<NonNullable<HubContext['project']>, { kind: 'project' }>; provider: string; pending: boolean; blocked: boolean
  onSubmit: (metadata: PackageUploadMetadata, file: File) => Promise<boolean>; onNew: () => void
}) {
  const [step, setStep] = useState(0), [type, setType] = useState<ArtifactKind | ''>('')
  const [file, setFile] = useState<File | null>(null), [artifactId, setArtifactId] = useState(''), [version, setVersion] = useState('')
  const [draft, setDraft] = useState(emptyPublicationMetadata), [validation, setValidation] = useState('')
  const [dragging, setDragging] = useState(false), [published, setPublished] = useState(false)
  const panel = useRef<HTMLElement>(null), fileInput = useRef<HTMLInputElement>(null)
  const native = type === 'ast' || type === 'knowledge', extension = native ? '.' + type : '.zip'
  const patch = (values: Partial<PublicationMetadataDraft>) => setDraft(previous => ({ ...previous, ...values }))
  useEffect(() => {
    const heading = panel.current?.querySelector('h2')
    if (heading) { heading.tabIndex = -1; heading.focus({ preventScroll: true }); heading.scrollIntoView({ block: 'start' }) }
  }, [step])
  function chooseFiles(files: FileList | null) {
    if (!files?.length) return
    if (files.length !== 1 || !type || !files[0].name.toLowerCase().endsWith(extension)) { setFile(null); setValidation('Choose one ' + extension + ' package for the selected artifact type.'); if (fileInput.current) fileInput.current.value = ''; return }
    setFile(files[0]); setValidation('')
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault()
    if (pending || blocked || published) return
    if (!type || !file) { setValidation('Choose an artifact type and package file.'); return }
    if (step === 0) { setValidation(''); setStep(1); return }
    if (!artifactId || !version || !draft.name || version.toLowerCase() === 'latest') { setValidation('Enter an artifact ID, name and concrete version. Latest is a catalogue pointer.'); return }
    if (draft.dependencies?.some(value => !value.id || !value.selector)) { setValidation('Enter an ID and version selector for each dependency.'); return }
    if (!native && draft.expected_sha256 && !/^[a-f0-9]{64}$/.test(draft.expected_sha256)) { setValidation('Expected archive SHA256 must contain 64 lowercase hexadecimal characters.'); return }
    setValidation('')
    if (step === 1) { setStep(2); return }
    const common = { provider, id: artifactId, version, name: draft.name,
      ...(draft.description ? { description: draft.description } : {}), ...(draft.author ? { author: draft.author } : {}),
      ...(draft.tags ? { tags: draft.tags.split('\n') } : {}), ...(draft.dependencies !== undefined ? { dependencies: draft.dependencies } : {}) }
    const publication: PackageUploadMetadata['publication'] = type === 'ast' || type === 'knowledge'
      ? { ...common, format: 'native', artifact_type: type }
      : { ...common, format: 'file', artifact_type: type, set_latest: draft.set_latest, ...(draft.expected_sha256 ? { expected_sha256: draft.expected_sha256 } : {}) }
    if (await onSubmit({ project_id: project.project_id, project_version: project.project_version, publication }, file)) setPublished(true)
  }
  const fileReviewFacts: Array<[string, string]> = native ? [] : [['Expected archive SHA256', draft.expected_sha256], ['Latest pointer', draft.set_latest ? 'Set to this concrete version' : 'Preserve current pointer']]
  return <>
    <ol className="publish-steps" aria-label="Publication steps">{['Package', 'Metadata & dependencies', 'Review & publish'].map((label, index) => <li key={label} aria-current={step === index ? 'step' : undefined}><button type="button" disabled={index > step || pending || published} onClick={() => { setValidation(''); setStep(index) }}><span>{index + 1}</span>{label}</button></li>)}</ol>
    <div className="publish-layout">
      <section className="work-panel" ref={panel}>
        <form className="work-form" onSubmit={event => { void submit(event) }}>
          <fieldset disabled={pending || published}>
            {step === 0 && <WorkSection title="Choose what you are sharing" description="Artifact type determines the required package format.">
              <label className="work-field"><span>Artifact type</span><StyledSelect value={type} onChange={event => { setType(event.target.value as ArtifactKind); setFile(null); setValidation(''); if (fileInput.current) fileInput.current.value = '' }}><option value="">Select a type</option>{artifactKinds.map(kind => <option key={kind}>{kind}</option>)}</StyledSelect></label>
              <div className={'package-drop ' + (dragging ? 'dragging' : '')} onDragOver={event => { event.preventDefault(); if (!pending && !published) setDragging(true) }} onDragLeave={() => setDragging(false)} onDrop={event => { event.preventDefault(); setDragging(false); if (!pending && !published) chooseFiles(event.dataTransfer.files) }}>
                <label className="work-field"><span>Package file</span><input ref={fileInput} type="file" accept={extension} disabled={!type} onChange={event => chooseFiles(event.target.files)} /><small>{file ? file.name + ' · ' + file.size + ' bytes' : 'Choose one ' + extension + ' file or drop it here.'}</small></label>
              </div>
              {type && <WorkNotice title={native ? 'Use an exported Native package' : 'Use a complete ZIP package'}>{native ? 'The server validates the exported package and its Native store.' : 'All FILE types, including Power, require a real package. The server validates its contents.'}</WorkNotice>}
            </WorkSection>}
            {step === 1 && <WorkSection title="Make the artifact understandable" description="Use a stable identifier and describe when this context should be applied.">
              <div className="work-two-columns"><label className="work-field"><span>Artifact ID</span><input required value={artifactId} onChange={event => setArtifactId(event.target.value)} /></label><label className="work-field"><span>Version</span><input required value={version} onChange={event => setVersion(event.target.value)} /></label></div>
              <PublicationMetadataFields draft={draft} patch={patch} native={native} />
            </WorkSection>}
            {step === 2 && <WorkSection title="Review the publication" description="Publishing can leave confirmed or uncertain effects if final delivery fails.">
              <FactList items={[[ 'Package', file?.name ], [ 'Artifact', type + ' / ' + artifactId ], [ 'Concrete version', version ], [ 'Name', draft.name ], [ 'Author', draft.author ], [ 'Tags', draft.tags ], ...fileReviewFacts, [ 'Dependencies', draft.dependencies === undefined ? 'Preserve current dependencies' : draft.dependencies.length ? draft.dependencies.map(value => value.artifact_type + ' / ' + value.id + ' @ ' + value.selector).join('\n') : 'Clear dependencies' ]]} />
              {draft.description && <section aria-label="Description preview"><MarkdownContent content={draft.description} /></section>}
            </WorkSection>}
          </fieldset>
          {validation && <p role="alert">{validation}</p>}
          {blocked && <WorkNotice title="Inspect the original publication">Use the outcome below before another attempt. No publication is repeated automatically.</WorkNotice>}
          <div className="work-actions">{step > 0 && !published && <button type="button" className="work-button" disabled={pending} onClick={() => setStep(step - 1)}>Back</button>}{published ? <button type="button" className="work-button" onClick={onNew}>Choose another package</button> : <button className="work-button primary" disabled={pending || blocked || (step === 0 && (!type || !file))}>{pending ? 'Publishing…' : step === 0 ? 'Describe artifact' : step === 1 ? 'Review publication' : 'Upload & publish'}</button>}</div>
        </form>
      </section>
      <aside className="work-panel"><WorkSection title="Publication target"><FactList items={[[ 'Publisher project', scopeLabel(project) ], [ 'Broker provider', provider ], [ 'Package format', type ? native ? 'Native ' + extension : 'FILE ZIP' : 'Choose an artifact type' ]]} /><p>The selected project publishes the artifact. Installation targets and bindings do not grant publication permission.</p></WorkSection></aside>
    </div>
  </>
}
