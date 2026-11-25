import { response as readLiveResponse, type LiveArtifactSelection, type LiveProjectSelection } from './live'
import { stringifyNativeJson } from './nativeJson'

export interface LiveCatalogEntry {
  id: string
  name: string
  description?: string
  project_name?: string
  tags?: string[]
  type: string
  ref: LiveArtifactSelection
  sourceLabel: string
  unavailable?: string
}
export interface LiveCatalogBinding {
  binding: string
  available: boolean
  model_selection: 'explicit'
}
export interface LiveCatalog {
  entries: LiveCatalogEntry[]
  errors: string[]
  projects: LiveProjectSelection[]
  bindings: LiveCatalogBinding[]
}
export const artifactKey = (a: LiveArtifactSelection) =>
  stringifyNativeJson([a.source, a.project_id, a.instance_id, a.project_kind, a.type, a.id, a.version])
export const projectKey = (project: LiveProjectSelection) =>
  stringifyNativeJson([project.project_id, project.project_version])

export async function loadLiveCatalog(signal?: AbortSignal): Promise<LiveCatalog> {
  return readLiveResponse<LiveCatalog>(await fetch((window.__API_BASE__ ?? '') + '/api/live/artifacts', { credentials: 'same-origin', signal }))
}
export function filterLiveCatalog(entries: LiveCatalogEntry[], query: string, type: string, source: string) {
  const q = query.trim().toLocaleLowerCase()
  return entries.filter(entry => (!type || entry.type === type) && (!source || entry.ref.source === source) &&
    (!q || [entry.id, entry.type, entry.name, entry.description, entry.project_name, ...(entry.tags ?? []), entry.ref.id, entry.ref.project_id, entry.ref.instance_id, entry.ref.project_kind, entry.ref.version, entry.sourceLabel].join(' ').toLocaleLowerCase().includes(q)))
}
