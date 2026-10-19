import { fileKinds, type ArtifactKind } from '@/api/hubNative'
import type { PublicationMetadataDraft } from './publicationDraft'
export function PublicationMetadataFields({ draft, patch, native = false }: { draft: PublicationMetadataDraft; patch: (values: Partial<PublicationMetadataDraft>) => void; native?: boolean }) {
  return <>
        <label className="work-field"><span>Publication name</span><input required value={draft.name} onChange={event => patch({ name: event.target.value })} /></label>
        {!native && <label><input type="checkbox" checked={draft.set_latest} onChange={event => patch({ set_latest: event.target.checked })} /> Set latest to this concrete version</label>}
        <details><summary>Optional publication metadata</summary><div className="work-form">
          <label className="work-field"><span>Description</span><textarea rows={3} value={draft.description} onChange={event => patch({ description: event.target.value })} /></label>
          <label className="work-field"><span>Author</span><input value={draft.author} onChange={event => patch({ author: event.target.value })} /></label>
          <label className="work-field"><span>Tags (one per line)</span><textarea rows={3} value={draft.tags} onChange={event => patch({ tags: event.target.value })} /></label>
          {!native && <label className="work-field"><span>Expected archive SHA256 for replacement</span><input pattern="[a-f0-9]{64}" value={draft.expected_sha256} onChange={event => patch({ expected_sha256: event.target.value })} /></label>}
          <label className="work-field"><span>Dependency handling</span><select value={draft.dependencies === undefined ? 'preserve' : 'replace'} onChange={event => patch({ dependencies: event.target.value === 'preserve' ? undefined : [] })}><option value="preserve">Preserve current dependencies</option><option value="replace">Replace dependencies (empty clears them)</option></select></label>
          {draft.dependencies?.map((dependency, index) => <fieldset key={index}><legend>Dependency {index + 1}</legend>
            <label className="work-field"><span>Dependency type</span><select value={dependency.artifact_type} onChange={event => patch({ dependencies: draft.dependencies!.map((value, at) => at === index ? { ...value, artifact_type: event.target.value as ArtifactKind } : value) })}>{[...fileKinds, 'ast', 'knowledge'].map(kind => <option key={kind}>{kind}</option>)}</select></label>
            <label className="work-field"><span>Dependency artifact ID</span><input required value={dependency.id} onChange={event => patch({ dependencies: draft.dependencies!.map((value, at) => at === index ? { ...value, id: event.target.value } : value) })} /></label>
            <label className="work-field"><span>Dependency version selector</span><input required value={dependency.selector} onChange={event => patch({ dependencies: draft.dependencies!.map((value, at) => at === index ? { ...value, selector: event.target.value } : value) })} /></label>
            <button type="button" className="work-button" onClick={() => patch({ dependencies: draft.dependencies!.filter((_value, at) => at !== index) })}>Remove dependency {index + 1}</button>
          </fieldset>)}
          {draft.dependencies !== undefined && <button type="button" className="work-button" onClick={() => patch({ dependencies: [...draft.dependencies!, { artifact_type: 'skill', id: '', selector: '' }] })}>Add dependency</button>}
        </div></details>
  </>
}
