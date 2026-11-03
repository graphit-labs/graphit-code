import { useId, useState } from 'react'
import { ModalPortal } from '@/components/shared/ModalPortal'
import { FactList } from '@/components/shared/EngineeringUI'
import { isLocalInstallKind, mutationArtifact, type FileOperation, type MutationSelection } from '@/api/hubNative'
import { HubFailure } from './HubFeedback'
import { bindingLabel, scopeLabel } from './hubContext'

export interface FileIntent { operation: FileOperation; request: MutationSelection; versions?: string[] }
export function FileActionDialog({ intent, pending, blocked, error, onSubmit, onCancel }: { intent: FileIntent; pending: boolean; blocked: boolean; error: unknown; onSubmit: (input: MutationSelection) => Promise<boolean>; onCancel: () => void }) {
  const id = useId()
  const request = intent.request
  const remote = 'selection' in request, local = 'source' in request
  const artifact = mutationArtifact(request)
  const rendered = isLocalInstallKind(artifact.artifact_type)
  const [version, setVersion] = useState(artifact.version || '')
  const [alias, setAlias] = useState(remote || local ? request.alias || '' : '')
  const [entryName, setEntryName] = useState(remote || local ? request.entry_name || '' : '')
  const [submitted, setSubmitted] = useState(false)
  async function submit(event: React.FormEvent) {
    event.preventDefault()
    if (pending || blocked || (remote && !version.trim())) return
    const input = structuredClone(intent.request)
    if ('source' in input) {
      if (alias !== '') input.alias = alias; else delete input.alias
      if (entryName !== '') input.entry_name = entryName; else delete input.entry_name
    } else if ('selection' in input) {
      input.selection.version = version.trim()
      if (input.scope.kind === 'project') {
          if (alias.trim()) input.alias = alias.trim(); else delete input.alias
        if (entryName.trim()) input.entry_name = entryName.trim(); else delete input.entry_name
      } else {
        delete input.mode; delete input.binding; delete input.alias; delete input.entry_name
      }
    }
    setSubmitted(true)
    if (await onSubmit(input)) onCancel()
  }
  const operation = intent.operation[0].toUpperCase() + intent.operation.slice(1)
  const title = local ? 'Install local resource' : operation + ' artifact'
  return <ModalPortal onClose={onCancel}><div className="work-modal-backdrop"><form className="work-dialog" role="dialog" aria-modal="true" aria-labelledby={id} onSubmit={submit}>
    <header><h2 id={id}>{title}</h2><button type="button" className="work-button" onClick={onCancel}>Close</button></header>
    <div className="work-dialog-body work-form">
      <FactList items={[[ 'Identifier',artifact.id ],['Type',artifact.artifact_type],['Captured target',scopeLabel(request.scope)],['Binding', request.binding ? bindingLabel(request.binding) : null],['Version',remote ? null : artifact.version],...(local ? [['Source project', request.source.project_id + ' @ ' + request.source.project_version] as [string, string],['Source', 'Local Resources'] as [string, string]] : [])]} />
      {remote && <label className="work-field"><span>Version to {intent.operation}</span>{intent.versions ? <select autoFocus required value={version} onChange={event => setVersion(event.target.value)}><option value="">Choose an exact published version</option>{intent.versions.map(item => <option key={item}>{item}</option>)}</select> : <input autoFocus required value={version} onChange={event => setVersion(event.target.value)} />}</label>}
      {(local || (remote && request.scope.kind === 'project')) && <>
        <p>{rendered ? 'Graphit renders this resource for the selected agent and copies it into the checkout.' : 'Graphit copies the selected files into the checkout.'}</p>
        <label className="work-field"><span>Local alias (optional)</span><input value={alias} onChange={event => setAlias(event.target.value)} /></label>
        <label className="work-field"><span>Entry name (optional)</span><input value={entryName} onChange={event => setEntryName(event.target.value)} /></label>
        {(local || intent.operation === 'update') && <p>Leave a name blank to preserve the existing value.</p>}
      </>}
      {intent.operation === 'remove' && <p>{request.scope.kind === 'global_cache' ? 'Remove this version from the installation cache inventory. Physical payload is preserved.' : 'Remove this exact installation. Foreign or pending state may require inspection and recovery.'}</p>}
      {intent.operation === 'recover' && <p>Resume the pending state for this captured target. A pending operation for another key will remain untouched.</p>}
      {intent.operation === 'forget' && <p className="work-notice">Forget logical ownership of this exact target. Physical files and residues remain. This does not authorize deleting those paths.</p>}
      {submitted && !pending && (local || (remote && request.scope.kind === 'project')) && error != null && <HubFailure error={error} />}
      {submitted && !pending && <p role="alert">{blocked ? 'Inspect the captured target in the operation outcome before choosing another operation.' : 'The operation did not complete. Review its outcome; your entered values are retained.'}</p>}
      {pending && <p role="status">Applying {local ? 'local installation' : intent.operation} to the captured target…</p>}
    </div>
    <footer><button type="button" className="work-button" onClick={onCancel}>Cancel</button><button className={'work-button ' + (intent.operation === 'remove' || intent.operation === 'forget' ? 'danger' : 'primary')} disabled={pending || blocked || (remote && !version.trim())}>{pending ? 'Applying…' : title}</button></footer>
  </form></div></ModalPortal>
}
