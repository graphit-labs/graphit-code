import { FactList, WorkSection } from '@/components/shared/EngineeringUI'
import type { ArtifactEffect, FileEffects, FileOutcome } from '@/api/hubNative'

const effectLabels: Record<ArtifactEffect, string> = {
  NotApplied: 'No filesystem changes',
  MayHaveApplied: 'Files may have changed',
  Applied: 'Filesystem changes applied',
}

export function HubObservedEffects({ effects, outcome }: { effects: FileEffects | null; outcome: Partial<FileOutcome> | null }) {
  const publication = effects?.publication
  const effect = typeof publication === 'object' ? publication?.effect : undefined
  const forgotten = outcome?.forgotten
  return <>
    <WorkSection title="Operation result">
      <FactList items={[
        ['Cache', effects?.cache],
        ['Installation', typeof publication === 'string' ? publication : publication?.change],
        ['Files', effect ? effectLabels[effect] : null],
      ]} />
      {effect === 'MayHaveApplied' && <p>Review the selected files before running the operation again.</p>}
      {effect === 'Applied' && <p>The files changed before the operation failed. Run sync again to finish where possible; edited files are preserved.</p>}
      {effects?.payload_preserved && <p>Installed files were preserved.</p>}
    </WorkSection>
    {forgotten && <WorkSection title="Files preserved">
      <p>The installation record was removed. These files were not deleted.</p>
      <FactList items={[
        ['Destination', forgotten.destination], ['Source', forgotten.source],
        ['Mode', forgotten.mode === 'Link' ? 'Link' : 'Copy'], ['Kind', forgotten.kind],
      ]} />
    </WorkSection>}
  </>
}
