import type { ReactNode } from 'react'
import { HubPublicationError, type PublicationTarget, type PublicationEffects } from '@/api/hubNative'
import { FactList, WorkNotice } from '@/components/shared/EngineeringUI'

export type PublicationResult = { epoch: number; request: PublicationTarget; effects: PublicationEffects | null; error: unknown; inspection: boolean }
const observed = (value: unknown) => typeof value === 'string' ? value : value == null ? undefined : 'Invalid response field'
function message(error: unknown) {
  if (error instanceof HubPublicationError) {
    if (error.code === 'invalid_response') return 'The server returned an invalid publication result. Inspect the original publication before another attempt.'
    if (error.inspectionRequired) return 'Inspect the original publication before another attempt. The response does not establish that nothing changed.'
    if (error.status === 401 || error.status === 403) return 'Publication access is unavailable. Check your account and project publication permissions.'
    if (error.status === 409) return 'Publication conflicted with the current catalogue. Inspect the original version before choosing an expected archive digest.'
    return 'Publication did not complete (HTTP ' + error.status + '). Review the recorded effects before another attempt.'
  }
  return 'The publication result could not be observed. Inspect the original publication before another attempt.'
}
export function PublicationFeedback({ result, pending, inspected, inspecting, inspectionError, onInspect }: { result: PublicationResult; pending: boolean; inspected: string | null; inspecting: boolean; inspectionError: unknown; onInspect: () => void }) {
  const effects = result.effects
  const receipt = typeof effects?.publication === 'object' ? effects.publication : null
  const native = 'format' in result.request.publication && result.request.publication.format === 'native'
  const receiptFacts: Array<[string, ReactNode]> = native ? [['Root SHA256', observed(receipt?.root_sha256)]]
    : [['Payload', observed(receipt?.payload) ?? (typeof effects?.publication === 'string' ? effects.publication : 'Not observed')], ['Archive SHA256', observed(receipt?.archive_sha256)]]
  const responseFacts: Array<[string, ReactNode]> = result.error instanceof HubPublicationError ? [['HTTP status', String(result.error.status)], ['Error code', result.error.code]] : []
  return <section className="hub-outcome" role="region" aria-label="Publication outcome" aria-live="polite">
    <h3>{pending ? 'Publishing artifact…' : result.error ? 'Publication needs review' : 'Publication confirmed'}</h3>
    <FactList items={[
      ['Source project', result.request.project_id + ' @ ' + result.request.project_version], ['Provider', result.request.publication.provider],
      ['Artifact', result.request.publication.artifact_type + ' / ' + result.request.publication.id], ['Concrete version', result.request.publication.version],
      ...responseFacts,
      ['Project metadata', observed(effects?.project_metadata) ?? 'Not observed'], ...receiptFacts,
      ['Catalogue', observed(receipt?.catalog) ?? 'Not observed'], ['Stage', observed(effects?.stage)], ['Cause', observed(effects?.cause)],
      ['Temporary residue', observed(effects?.owned_temporary_residue)], ['Observation', effects?.observation_failed === true ? 'Unavailable' : null],
    ]} />
    {!pending && result.error != null && <p role="alert">{message(result.error)}</p>}
    {!pending && <button type="button" className="work-button" disabled={inspecting} onClick={onInspect}>{inspecting ? 'Inspecting…' : 'Inspect original publication'}</button>}
    {inspected && <WorkNotice title="Publication catalogue inspected">{inspected}</WorkNotice>}
    {inspectionError != null && <WorkNotice title="Publication inspection failed">The original catalogue could not be read. No publication was repeated.</WorkNotice>}
  </section>
}
