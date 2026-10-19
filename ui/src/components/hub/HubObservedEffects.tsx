import { FactList, WorkSection } from '@/components/shared/EngineeringUI'
import type { FileEffects, FileOutcome, HubPhysicalDescriptor } from '@/api/hubNative'


function physicalModeLabel(mode: HubPhysicalDescriptor['mode'] | undefined): string | undefined {
  return mode === undefined ? undefined : mode === 'Link' ? 'Link' : 'Copy (' + mode.Copy + ')'
}

export function HubObservedEffects({ effects, outcome }: { effects: FileEffects | null; outcome: Partial<FileOutcome> | null }) {
  const publication = effects?.publication
  const preview = outcome?.forget_preview
  const paths = new Map([...(outcome?.possible_parents || []), ...(preview?.possible_parents || [])].map(path => [path, path]))
  const flags = [
    effects?.cleanup_pending && 'Publication cleanup remains pending.',
    effects?.stage_cleanup_pending && 'Temporary stage cleanup remains pending.',
    effects?.blocked_by_pending && 'Another pending key blocks this operation.',
    effects?.observation_failed && 'The final state could not be observed.',
    typeof publication === 'object' && publication?.hub_pending && 'Hub reconciliation remains pending.',
    typeof publication === 'object' && publication?.core_pending && 'Physical reconciliation remains pending.',
    typeof publication === 'object' && publication?.cleanup_pending && 'Publication cleanup remains pending.',
    effects?.payload_preserved && 'Physical payload was preserved.',
  ].filter(Boolean)
  return <>
    <WorkSection title="Observed effects"><FactList items={[[ 'Cache', effects?.cache ], [ 'Publication', typeof publication === 'string' ? publication : publication?.change ], [ 'Commit', typeof publication === 'object' && publication ? publication.committed === true ? 'Confirmed' : publication.committed === false ? 'Not committed' : publication.committed === null ? 'Unknown' : null : null ]]} />
      {!!flags.length && <ul>{flags.map((flag, index) => <li key={index}>{flag}</li>)}</ul>}
    </WorkSection>
    {preview && <WorkSection title="Preserved physical residues"><p>Forget changes logical ownership. It does not authorize deletion of these paths.</p><FactList items={[[ 'Owned destination',preview.owned?.destination],['Owned source',preview.owned?.source],['Owned mode',physicalModeLabel(preview.owned?.mode)],['Owned node kind',preview.owned?.node_kind],['Pending destination',preview.pending?.candidate.destination],['Pending source',preview.pending?.candidate.source],['Pending mode',physicalModeLabel(preview.pending?.candidate.mode)],['Pending node kind',preview.pending?.candidate.node_kind],['Staging directory',preview.pending?.staging_directory],['Old staging directory',preview.pending?.old_staging_directory],['Old destination',preview.pending?.old_destination],['Payload',preview.pending?.payload],['Backup',preview.pending?.backup]]} /></WorkSection>}
    {!!paths.size && <WorkSection title="Possible parent residues"><ul className="hub-paths">{Array.from(paths, ([key, path]) => <li key={key}>{path}</li>)}</ul></WorkSection>}
  </>
}
