import { FactList, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { HubRequestError, HubUpdateAllError, mutationArtifact } from '@/api/hubNative'
import type { SingleHubMutationResult } from './useHubMutation'
import { bindingLabel, scopeLabel } from './hubContext'
import { HubObservedEffects } from './HubObservedEffects'

export function HubFailure({ error }: { error: unknown }) {
  const known = error instanceof HubRequestError || error instanceof HubUpdateAllError
  const status = known ? error.status : null
  const code = known ? error.code : null
  const title = status === 401 ? 'Sign in again' : status === 403 ? 'Access unavailable' : status === 404 ? 'Selected resource not found'
    : code === 'catalog_restart_required' ? 'The catalog changed' : status === 409 ? 'The selected state changed'
      : status === 413 ? 'The response exceeded its limit' : status === 499 ? 'Request cancelled' : status === 504 ? 'Request deadline reached'
        : status === 501 ? 'Operation unavailable' : status === 503 ? 'Service unavailable' : status === 400 ? 'Check the supplied selection' : 'Hub request failed'
  return <WorkNotice title={title} tone="error">{code === 'catalog_restart_required' ? 'Restart from the first page with the current source and filters.' : 'Review the selected source and target. Use the header to check your account or refresh the current state.'}</WorkNotice>
}

export function HubMutationFeedback({ result, pending, onInspect, inspecting }: { result: SingleHubMutationResult | null; pending: boolean; onInspect: () => void; inspecting: boolean }) {
  if (pending && !result) return <p role="status">Applying the selected operation…</p>
  if (!result) return null
  const input = result.request
  const artifact = mutationArtifact(input)
  const effects = result.effects
  return <section className="hub-outcome" aria-label="Operation outcome">
    {pending ? <p role="status">Applying the selected operation…</p> : result.error ? <HubFailure error={result.error} /> : <WorkNotice title={result.completed ? 'Operation completed' : 'Inspect the remaining state'} tone={result.completed ? 'success' : undefined} />}
    <WorkSection title="Captured target">
      <FactList items={[[ 'Operation', result.operation ],['Scope', scopeLabel(input.scope)],['Identifier',artifact.id],['Producer', 'selection' in input ? input.selection.producer : result.outcome?.producer],['Source', 'selection' in input ? input.selection.transport + ' / ' + input.selection.provider : 'source' in input ? 'Local Resources' : null],...('source' in input ? [['Source project', input.source.project_id + ' @ ' + input.source.project_version] as [string, string], ['Requested alias', input.alias ?? 'Omitted (preserve existing)'] as [string, string], ['Requested entry name', input.entry_name ?? 'Omitted (preserve existing)'] as [string, string]] : []),['Version',artifact.version],...(('selection' in input || 'source' in input) && input.scope.kind === 'project' ? [['Requested installation mode', 'Copy'] as [string, string]] : []),['Binding', input.binding ? bindingLabel(input.binding) : null]]} />
      {result.inspectionRequired && <WorkNotice title={result.inspected ? 'Captured target inspected' : 'Inspection required'}>{result.inspected ? 'Review the observations before choosing the next operation.' : 'The response does not establish a complete result. Inspect this original target before retrying or recovering. No rollback is implied.'}</WorkNotice>}
      <button className="work-button" onClick={onInspect} disabled={pending || inspecting}>{inspecting ? 'Inspecting target…' : 'Inspect captured target'}</button>
    </WorkSection>
    <HubObservedEffects effects={effects} outcome={result.outcome} />
  </section>
}
