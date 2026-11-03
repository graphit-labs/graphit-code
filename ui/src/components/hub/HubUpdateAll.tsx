import { useId, useState } from 'react'
import { HubUpdateAllError, type InstalledFile, type UpdateAllInput } from '@/api/hubNative'
import { FactList, WorkEmpty, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { ModalPortal } from '@/components/shared/ModalPortal'
import { HubFailure } from './HubFeedback'
import { HubObservedEffects } from './HubObservedEffects'
import { bindingLabel, scopeLabel } from './hubContext'
import type { HubUpdateAllResult } from './useHubMutation'
export type UpdateAllIntent = { operation: 'update_all'; request: UpdateAllInput }
const targetLabel = (target: InstalledFile['target']) => target.kind === 'global_cache' ? 'Installation cache' : target.kind === 'native_binding' ? bindingLabel(target.binding) : 'Adapter ' + target.adapter
export function HubUpdateAllFeedback({ result, pending, onInspect, inspecting }: { result: HubUpdateAllResult; pending: boolean; onInspect: () => void; inspecting: boolean }) {
  const outcome = result.outcome
  return <section className="hub-outcome" aria-label="Collective update outcome" aria-live="polite">
    {pending ? <p role="status">Updating remote installations…</p> : result.error ? <HubFailure error={result.error} /> : <WorkNotice title={result.completed ? outcome?.items.length ? 'Remote installations updated' : 'No remote installations to update' : 'Update needs review'} tone={result.completed && outcome?.items.length ? 'success' : undefined} />}
    <WorkSection title="Captured update scope"><FactList items={[[ 'Scope', scopeLabel(result.request.scope) ], [ 'Binding', result.request.binding ? bindingLabel(result.request.binding) : 'Not applicable' ], [ 'Status', outcome?.status ?? 'Not observed' ], [ 'HTTP status', result.error instanceof HubUpdateAllError ? String(result.error.status) : undefined ], [ 'Error code', result.error instanceof HubUpdateAllError ? result.error.code : undefined ]]} />
      {result.inspectionRequired && <WorkNotice title={result.inspected ? 'Captured target inspected' : 'Inspection required'}>{result.inspected ? 'Review the observations before choosing another operation.' : 'Inspect the original scope before another operation. No rollback or absence of effects is implied.'}</WorkNotice>}
      <button className="work-button" disabled={pending || inspecting} onClick={onInspect}>{inspecting ? 'Inspecting target…' : 'Inspect captured target'}</button>
    </WorkSection>
    {outcome ? outcome.items.length ? <ol aria-label="Remote update items">{outcome.items.map((item, index) => <li key={index}>
      <WorkSection title={item.artifact_type + ' / ' + item.id}><FactList items={[[ 'Producer', item.producer ], [ 'Previous version', item.previous_version ], [ 'Target', targetLabel(item.target) ], [ 'Item status', item.status ], [ 'Item error', item.code ], [ 'Resolved version', item.outcome?.version ]]} />
        {item.status === 'not_attempted' ? <p>This installation was not attempted.</p> : <HubObservedEffects effects={item.outcome} outcome={item.outcome} />}
      </WorkSection>
    </li>)}</ol> : <WorkEmpty title="No eligible remote installations">The captured scope returned no remote installations to update.</WorkEmpty>
      : <><p>{result.inspectionRequired ? 'A complete item list was not observed. Inspect the captured scope; omitted items do not establish that nothing ran.' : 'No item outcomes were reported.'}</p>{result.effects && <HubObservedEffects effects={result.effects} outcome={null} />}</>}
  </section>
}
export function UpdateAllDialog({ intent, pending, blocked, onSubmit, onCancel }: { intent: UpdateAllIntent; pending: boolean; blocked: boolean; onSubmit: (input: UpdateAllInput) => Promise<boolean>; onCancel: () => void }) {
  const title = useId(), [submitted, setSubmitted] = useState(false)
  async function submit(event: React.FormEvent) { event.preventDefault(); if (pending || blocked) return; setSubmitted(true); if (await onSubmit(structuredClone(intent.request))) onCancel() }
  return <ModalPortal onClose={onCancel}><div className="work-modal-backdrop"><form className="work-dialog" role="dialog" aria-modal="true" aria-labelledby={title} onSubmit={event => { void submit(event) }}>
    <header><h2 id={title}>Update all remote installations</h2><button type="button" className="work-button" onClick={onCancel}>Close</button></header>
    <div className="work-dialog-body work-form"><FactList items={[[ 'Captured scope', scopeLabel(intent.request.scope) ], [ 'Binding', intent.request.binding ? bindingLabel(intent.request.binding) : 'Not applicable' ]]} />
      <p>Update every eligible remote FILE installation in this scope to its publisher’s latest release. Current list filters and pages do not limit this operation.</p><p>Existing aliases and entry names are preserved. Resources and LocalLink installations are excluded. Processing stops at the first failure; earlier updates may remain applied.</p>
      {submitted && !pending && <p role="alert">{blocked ? 'Inspect the captured scope in the outcome before another operation.' : 'The update did not fully complete. Review each item and its recorded effects before another attempt.'}</p>}
      {pending && <p role="status">Updating the captured scope…</p>}
    </div><footer><button type="button" className="work-button" onClick={onCancel}>Cancel</button><button className="work-button primary" disabled={pending || blocked}>{pending ? 'Updating…' : 'Update remote installations'}</button></footer>
  </form></div></ModalPortal>
}
