import { nativeRecordMultipartResponse, nativeRecordResponse } from './nativeRecordRead'
import { parseNativeJson, stringifyNativeJson, type NativeInteger } from './nativeJson'

export const fileKinds = ['skill', 'command', 'workflow', 'agent', 'rule', 'mcp', 'language', 'power'] as const
export type FileKind = typeof fileKinds[number]
export type ArtifactKind = FileKind | 'ast' | 'knowledge'
export const artifactKinds = [...fileKinds, 'ast', 'knowledge'] as const
export const localInstallKinds = ['agent', 'skill', 'command', 'workflow'] as const
export type LocalInstallKind = typeof localInstallKinds[number]
export const isLocalInstallKind = (kind: ArtifactKind): kind is LocalInstallKind => localInstallKinds.some(value => value === kind)
export interface AuthoredProject { project_id: string; project_version: string }
export interface AuthoredResource { artifact_type: ArtifactKind; id: string; version: string; name: null }
export interface AuthoredSelection extends AuthoredProject { artifact_type: ArtifactKind; id: string; version: string }
export interface AuthoredQuery extends AuthoredProject { artifact_type?: ArtifactKind; id?: string; version?: string; cursor?: string; limit: number }
export interface AuthoredPage extends AuthoredProject { status: 'ok'; entries: AuthoredResource[]; next_cursor: string | null }
export interface AuthoredReview extends AuthoredSelection {
  status: 'ok'; source: 'Resources'; metadata: null
  entries: { path: string; kind: 'File' | 'Directory'; bytes: NativeInteger }[]; next_offset: number | null
}
export interface AuthoredContent extends AuthoredSelection { status: 'ok'; content: { path: string; content: string; bytes: NativeInteger } }
export const nativeBindings = ['antigravity', 'claude', 'codex', 'cursor', 'kiro-v3', 'opencode', 'qwen', 'kimi'] as const
export type NativeBinding = typeof nativeBindings[number]
export type HubScope = { kind: 'global_cache' } | { kind: 'project'; project_id: string; project_version: string }
export type HubOrigin = { transport: 'broker'; provider: string } | { transport: 'standalone'; provider: string; profile?: string | null }
export type HubSource = { transport: 'broker'; provider: string } | { transport: 'standalone'; provider: string; profile?: string }
export interface CatalogArtifact {
  artifact_type: ArtifactKind; id: string; name: string; description: string | null
  tags: string[]; author: string | null; latest: string | null; versions: string[]
}
export interface ProjectPage { status: 'ok'; projects: { id: string; name: string }[]; next_cursor: string | null }
export interface ArtifactPage { status: 'ok'; producer: string; artifacts: CatalogArtifact[]; next_cursor: string | null }
export const sourceOrigins = ['HubCache', 'Resources', 'LocalLink'] as const
export type SourceOrigin = typeof sourceOrigins[number]
export interface InstalledFile {
  artifact_type: FileKind; id: string; version: string; producer: string
  target: { kind: 'global_cache' } | { kind: 'native_binding'; binding: NativeBinding } | { kind: 'shared_adapter'; adapter: string }
  source_origin: SourceOrigin; origin: HubOrigin | null; alias: string | null; entry_name: string | null
  display: { name: string; description: string | null; tags: string[] } | null
}
export interface InstalledFilePage { status: 'ok'; scope: HubScope; entries: InstalledFile[]; next_offset: number | null }
export interface RemoteSelection { transport: 'broker' | 'standalone'; provider: string; profile?: string; producer: string; artifact_type: FileKind; id: string; version: string; exact: boolean }
export type PhysicalMode = 'copy' | 'link'
export interface InstallSelection { selection: RemoteSelection; scope: HubScope; binding?: NativeBinding; alias?: string; entry_name?: string; mode?: PhysicalMode }
export interface LocalInstallSelection { source: AuthoredSelection & { artifact_type: LocalInstallKind }; scope: Extract<HubScope, { kind: 'project' }>; binding: NativeBinding; alias?: string; entry_name?: string; mode?: PhysicalMode }
export interface LifecycleSelection { scope: HubScope; artifact_type: FileKind; id: string; version?: string; binding?: NativeBinding }
export type FileOperation = 'install' | 'update' | 'remove' | 'recover' | 'forget' | 'install_local'
export interface PublicationState {
  change?: string; committed?: boolean | null; hub_pending?: boolean; core_pending?: boolean
  cleanup_pending?: boolean; observation_failed?: boolean
}
export interface FileEffects {
  cache?: string; publication?: string | PublicationState; cleanup_pending?: boolean
  stage_cleanup_pending?: boolean; blocked_by_pending?: boolean; observation_failed?: boolean; payload_preserved?: boolean
}
export interface HubPhysicalDescriptor {
  source: string; destination: string
  mode: 'Link' | { Copy: 'RejectFileLinks' | 'FollowRegularFiles' }; node_kind: 'File' | 'Directory'
}
export interface FileOutcome extends FileEffects {
  status: 'ok' | 'error'; operation: FileOperation; scope: HubScope
  source?: 'local'; project?: string; project_version?: string
  artifact_type?: FileKind; id?: string; version?: string; producer?: string; binding?: NativeBinding
  possible_parents?: string[]; forget_preview?: {
    owned?: HubPhysicalDescriptor
    pending?: { candidate: HubPhysicalDescriptor; staging_directory?: string; old_staging_directory?: string; old_destination?: string; payload?: string; backup?: string; permissions_may_be_modified: boolean }
    possible_parents: string[]
  } | null
}
export type UpdateAllInput = { scope: Extract<HubScope, { kind: 'project' }>; binding: NativeBinding } | { scope: { kind: 'global_cache' }; binding?: never }
export type UpdateAllItemEffects = FileEffects & Partial<Omit<FileOutcome, 'operation' | 'scope' | 'status'>> & { status: 'ok' | 'error' }
export const updateAllItemCodes = ['authentication_or_permission_denied', 'request_cancelled_or_expired', 'installation_or_release_not_found', 'installation_changed', 'update_failed'] as const
export type UpdateAllItem = { artifact_type: FileKind; id: string; producer: string; previous_version: string; target: InstalledFile['target'] } & (
  { status: 'not_attempted'; outcome?: never; code?: never } | { status: 'ok'; outcome: UpdateAllItemEffects; code?: never } | { status: 'error'; outcome: UpdateAllItemEffects; code: typeof updateAllItemCodes[number] })
export interface UpdateAllOutcome { status: 'ok' | 'error'; operation: 'update_all'; scope: HubScope; global: boolean; items: UpdateAllItem[] }
export class HubUpdateAllError extends Error {
  constructor(public readonly status: number, public readonly code: string | null, public readonly outcome: UpdateAllOutcome | null,
    public readonly effects: FileEffects | null, public readonly inspectionRequired: boolean, public readonly request: UpdateAllInput,
    public readonly receiptCorrelated = true, public readonly notAttempted = false) { super('Collective update failed (HTTP ' + status + ')') }
}
export interface FileContent {
  status: 'ok'; source: 'remote' | 'installed'; producer: string; artifact_type: FileKind
  id: string; version: string; content: { path: string; content: string }; cache: 'ephemeral' | 'Verified'
}
export interface InstalledQuery { scope: HubScope; binding?: NativeBinding; artifact_type?: FileKind; id?: string; version?: string; offset: number; limit: number }
export type MutationSelection = InstallSelection | LifecycleSelection | LocalInstallSelection
export const mutationArtifact = (input: MutationSelection) => 'selection' in input ? input.selection : 'source' in input ? input.source : input
export interface FilePublicationInput {
  project_id: string; project_version: string
  publication: {
    provider: string; artifact_type: FileKind; id: string; version: string; name: string
    description?: string; author?: string; tags?: string[]
    dependencies?: { artifact_type: ArtifactKind; id: string; selector: string }[]
    expected_sha256?: string; set_latest?: boolean
  }
}
export interface FilePublicationReceipt {
  producer: string; artifact_type: FileKind; id: string; version: string
  archive_sha256: string; payload: string; catalog: string; root_sha256?: never
}
export interface FilePublicationEffects {
  publication?: Partial<FilePublicationReceipt> | string; project_metadata?: string
  stage?: string; cause?: string; owned_temporary_residue?: string | null; observation_failed?: boolean
}
export interface FilePublicationOutcome extends FilePublicationEffects {
  status: 'published'; publication: FilePublicationReceipt; project_metadata: string
}
export type PackageUploadMetadata = { project_id: string; project_version: string; publication: (FilePublicationInput['publication'] & { format: 'file' }) | (Omit<FilePublicationInput['publication'], 'artifact_type' | 'expected_sha256' | 'set_latest'> & { format: 'native'; artifact_type: 'ast' | 'knowledge' }) }
export interface NativePublicationReceipt {
  producer: string; artifact_type: 'ast' | 'knowledge'; id: string; version: string
  root_sha256: string; catalog: string; archive_sha256?: never; payload?: never
}
export type PublicationEffects = Omit<FilePublicationEffects, 'publication'> & { publication?: Partial<FilePublicationReceipt | NativePublicationReceipt> | string }
export interface NativePublicationOutcome extends PublicationEffects {
  status: 'published'; publication: NativePublicationReceipt; project_metadata: string
}
export type PackagePublicationOutcome = FilePublicationOutcome | NativePublicationOutcome
export type PublicationTarget = { project_id: string; project_version: string; publication: { provider: string; artifact_type: ArtifactKind; id: string; version: string } }
export class HubPublicationError extends Error {
  constructor(public readonly status: number, public readonly code: string | null,
    public readonly effects: PublicationEffects | null, public readonly inspectionRequired: boolean,
    public readonly request: FilePublicationInput | PackageUploadMetadata) { super(`Hub publication failed (HTTP ${status})`) }
}
export interface HubUnpublishInput {
  provider: string; producer: string; artifact_type: ArtifactKind; id: string
  target: { kind: 'version'; version: string; expected_sha256?: string } | { kind: 'artifact' }
}
export interface HubRetractionReceipt {
  producer: string; artifact_type: ArtifactKind; id: string; targeted_versions: string[]
  desired_latest: string | null; catalog: 'updated' | 'removed'
}
export interface HubUnpublishEffects {
  status?: string; retraction?: Partial<Omit<HubRetractionReceipt, 'catalog'>> & { catalog?: string }
  stage?: string; cause?: string; observation_failed?: boolean
}
export interface HubUnpublishOutcome extends HubUnpublishEffects { status: 'unpublished'; retraction: HubRetractionReceipt }
export class HubUnpublishError extends Error {
  constructor(public readonly status: number, public readonly code: string | null,
    public readonly effects: HubUnpublishEffects | null, public readonly inspectionRequired: boolean,
    public readonly request: HubUnpublishInput, public readonly receiptCorrelated = true) { super(`Hub unpublication failed (HTTP ${status})`) }
}
export function unpublishNeedsInspection(effects: HubUnpublishEffects | null): boolean {
  return effects?.observation_failed === true || !effects?.retraction
    || !['not_attempted', 'rejected', 'updated', 'removed'].includes(effects.retraction.catalog ?? '')
}
type JsonObject = Record<string, unknown>
const object = (value: unknown): value is JsonObject => value !== null && typeof value === 'object' && !Array.isArray(value)
const cursor = (value: unknown) => value === null || typeof value === 'string'
function validInstalledSource(entry: unknown): boolean {
  if (!object(entry) || !sourceOrigins.some(origin => origin === entry.source_origin)) return false
  const remote = entry.origin
  if (remote === null) return true
  return entry.source_origin === 'HubCache' && object(remote)
    && (remote.transport === 'broker' || remote.transport === 'standalone')
    && typeof remote.provider === 'string'
    && (remote.transport !== 'standalone' || remote.profile === undefined || remote.profile === null || typeof remote.profile === 'string')
}
const sameScope = (a: HubScope | undefined, b: HubScope) => a?.kind === b.kind && (a.kind === 'global_cache' || (b.kind === 'project' && a.project_id === b.project_id && a.project_version === b.project_version))
const sameAuthoredProject = (a: AuthoredProject, b: AuthoredProject) => a.project_id === b.project_id && a.project_version === b.project_version
const sameAuthoredSelection = (a: AuthoredSelection, b: AuthoredSelection) => sameAuthoredProject(a, b) && a.artifact_type === b.artifact_type && a.id === b.id && a.version === b.version
const authoredRelativePath = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && !value.startsWith('/') && !value.split('/').includes('..') && !value.includes('\0')
const nativeByteCount = (value: unknown): value is NativeInteger => typeof value === 'bigint' ? value >= 0n : typeof value === 'number' && Number.isSafeInteger(value) && value >= 0

function retractionMatches(value: unknown, input: HubUnpublishInput, complete: boolean): boolean {
  if (!object(value)) return false
  if (['producer', 'artifact_type', 'id'].some(key => (complete || value[key] !== undefined) && value[key] !== input[key as 'producer' | 'artifact_type' | 'id'])) return false
  const versions = value.targeted_versions
  if (complete || versions !== undefined) {
    if (!Array.isArray(versions) || !versions.length || !versions.every(item => typeof item === 'string' && item.length > 0 && item.toLowerCase() !== 'latest')
      || new Set(versions).size !== versions.length
      || (input.target.kind === 'version' && (versions.length !== 1 || versions[0] !== input.target.version))) return false
  }
  if (complete || value.desired_latest !== undefined) {
    if (value.desired_latest !== null && (typeof value.desired_latest !== 'string' || !value.desired_latest.length
      || value.desired_latest.toLowerCase() === 'latest' || (Array.isArray(versions) && versions.includes(value.desired_latest)))) return false
  }
  if (value.catalog === 'removed' && value.desired_latest !== null) return false
  return !complete || (['updated', 'removed'].includes(String(value.catalog)) && (input.target.kind !== 'artifact' || value.catalog === 'removed'))
}

function validUnpublishEffects(value: unknown): value is HubUnpublishEffects {
  if (!object(value) || ['status', 'stage', 'cause'].some(key => value[key] !== undefined && typeof value[key] !== 'string')
    || (value.observation_failed !== undefined && typeof value.observation_failed !== 'boolean')) return false
  const receipt = value.retraction
  return receipt === undefined || (object(receipt)
    && ['producer', 'artifact_type', 'id', 'catalog'].every(key => receipt[key] === undefined || typeof receipt[key] === 'string')
    && (receipt.targeted_versions === undefined || (Array.isArray(receipt.targeted_versions) && receipt.targeted_versions.every(item => typeof item === 'string')))
    && (receipt.desired_latest === undefined || receipt.desired_latest === null || typeof receipt.desired_latest === 'string'))
}

export class HubRequestError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string | null,
    public readonly outcome: FileOutcome | null,
    public readonly inspectionRequired: boolean,
    public readonly effects: FileEffects | null,
    public readonly request: unknown,
  ) { super(`Hub request failed (HTTP ${status})`) }
}

async function request<T>(path: string, input: unknown, signal?: AbortSignal): Promise<T> {
  // The snapshot names the original selection when a bounded response omits its receipt.
  const captured = structuredClone(input)
  const response = await nativeRecordResponse(path, captured, signal)
  const text = await response.text()
  let body: unknown = null
  try { if (text) body = parseNativeJson<unknown>(text) } catch { /* Keep the actual status and inspection header. */ }
  const envelope = object(body) ? body : null
  const inspection = response.headers.get('X-Graphit-Inspection-Required') === 'request-selection' || envelope?.inspection_required === true
  if (!response.ok || envelope?.status === 'error') {
    const error = envelope?.error
    const code = object(error) && typeof error.code === 'string' ? error.code : null
    throw new HubRequestError(response.status, code,
      object(envelope?.outcome) ? envelope.outcome as unknown as FileOutcome : null,
      inspection, object(envelope?.effects) ? envelope.effects as FileEffects : null, captured)
  }
  if (!envelope || envelope.status !== 'ok') throw new Error('Invalid Hub response')
  return envelope as T
}

export const hubNative = {
  async authoredList(input: AuthoredQuery, signal?: AbortSignal): Promise<AuthoredPage> {
    const captured = structuredClone(input)
    const result = await request<AuthoredPage>('/hub/authored/list', captured, signal)
    if (!sameAuthoredProject(result, captured) || !Array.isArray(result.entries) || !cursor(result.next_cursor)
      || (result.next_cursor !== null && result.next_cursor === captured.cursor)
      || !result.entries.every(entry => object(entry) && artifactKinds.includes(entry.artifact_type)
        && typeof entry.id === 'string' && entry.id.length > 0 && typeof entry.version === 'string' && entry.version.length > 0
        && entry.name === null && (!captured.artifact_type || entry.artifact_type === captured.artifact_type)
        && (captured.id === undefined || entry.id === captured.id) && (captured.version === undefined || entry.version === captured.version))) throw new Error('Invalid authored resource page')
    return result
  },
  async authoredReview(input: AuthoredSelection & { offset?: number; limit: number }, signal?: AbortSignal): Promise<AuthoredReview> {
    const captured = structuredClone(input)
    const result = await request<AuthoredReview>('/hub/authored/review', captured, signal)
    if (!sameAuthoredSelection(result, captured) || result.source !== 'Resources' || result.metadata !== null || !Array.isArray(result.entries)
      || (result.next_offset !== null && (!Number.isSafeInteger(result.next_offset) || result.next_offset <= (captured.offset ?? 0)))
      || !result.entries.every(entry => object(entry) && authoredRelativePath(entry.path)
        && (entry.kind === 'File' || entry.kind === 'Directory') && nativeByteCount(entry.bytes))) throw new Error('Invalid authored resource review')
    return result
  },
  async authoredContent(input: AuthoredSelection & { path: string }, signal?: AbortSignal): Promise<AuthoredContent> {
    if (!authoredRelativePath(input.path)) throw new Error('Invalid authored resource path')
    const captured = structuredClone(input)
    const result = await request<AuthoredContent>('/hub/authored/content', captured, signal)
    if (!sameAuthoredSelection(result, captured) || !object(result.content) || result.content.path !== captured.path
      || typeof result.content.content !== 'string' || !nativeByteCount(result.content.bytes)) throw new Error('Invalid authored resource content')
    return result
  },
  async unpublish(input: HubUnpublishInput, signal?: AbortSignal): Promise<HubUnpublishOutcome> {
    const captured = structuredClone(input)
    let response: Response
    try { response = await nativeRecordResponse('/hub/unpublish', captured, signal) }
    catch (cause) {
      if (signal?.aborted) throw cause
      throw new HubUnpublishError(0, null, null, true, captured)
    }
    let body: unknown = null
    try { const text = await response.text(); if (text) body = parseNativeJson<unknown>(text) } catch { /* Preserve actual HTTP status and inspection header. */ }
    const envelope = object(body) ? body : null
    const rawEffects = object(envelope?.outcome) ? envelope.outcome : object(envelope?.effects) ? envelope.effects : null
    const effects = validUnpublishEffects(rawEffects) ? rawEffects : null
    const inspection = response.status === 413 || response.headers.get('X-Graphit-Inspection-Required') === 'request-selection'
      || envelope?.inspection_required === true
    const error = envelope?.error
    const receiptCorrelated = rawEffects === null || (effects !== null && (effects.retraction === undefined || retractionMatches(effects.retraction, captured, ['updated', 'removed'].includes(effects.retraction.catalog ?? ''))))
    if (!response.ok) throw new HubUnpublishError(response.status, object(error) && typeof error.code === 'string' ? error.code : null,
      effects, inspection || !receiptCorrelated || unpublishNeedsInspection(effects), captured, receiptCorrelated)
    const receipt = envelope?.retraction
    if (envelope?.status !== 'unpublished' || !retractionMatches(receipt, captured, true)) {
      throw new HubUnpublishError(response.status, 'invalid_response', validUnpublishEffects(envelope) ? envelope : null, true, captured, false)
    }
    return envelope as unknown as HubUnpublishOutcome
  },
  async publish(input: FilePublicationInput, signal?: AbortSignal): Promise<FilePublicationOutcome> {
    const captured = structuredClone(input)
    const response = await nativeRecordResponse('/hub/files/publish', captured, signal)
    return publicationResponse(response, captured)
  },
  async upload(input: PackageUploadMetadata, file: File, signal?: AbortSignal): Promise<PackagePublicationOutcome> {
    const captured = structuredClone(input), body = new FormData()
    body.append('metadata', stringifyNativeJson(captured))
    body.append('file', file)
    return publicationResponse(await nativeRecordMultipartResponse('/hub/upload', body, signal), captured)
  },
  async projects(input: HubSource & { limit: number; cursor?: string }, signal?: AbortSignal): Promise<ProjectPage> {
    const result = await request<ProjectPage>('/hub/remote/projects', input, signal)
    if (!Array.isArray(result.projects) || !cursor(result.next_cursor)) throw new Error('Invalid Hub project page')
    return result
  },
  async search(input: HubSource & { producer: string; limit: number; artifact_type?: ArtifactKind; id?: string; version?: string; cursor?: string }, signal?: AbortSignal): Promise<ArtifactPage> {
    const result = await request<ArtifactPage>('/hub/remote/search', input, signal)
    if (result.producer !== input.producer || !Array.isArray(result.artifacts) || !cursor(result.next_cursor)) throw new Error('Invalid Hub artifact page')
    return result
  },
  async installed(input: InstalledQuery, signal?: AbortSignal): Promise<InstalledFilePage> {
    const result = await request<InstalledFilePage>('/hub/installed/files', input, signal)
    if (!sameScope(result.scope, input.scope) || !Array.isArray(result.entries) || !result.entries.every(validInstalledSource)
      || (result.next_offset !== null && (!Number.isSafeInteger(result.next_offset) || result.next_offset <= input.offset))) throw new Error('Invalid installed FILE page')
    return result
  },
  content(input: { remote: { selection: RemoteSelection; path: string } } | { installed: LifecycleSelection & { version: string; path: string } }, signal?: AbortSignal) {
    return request<FileContent>('/hub/files/content', input, signal)
  },
  async updateAll(input: UpdateAllInput, signal?: AbortSignal): Promise<UpdateAllOutcome> {
    const captured = structuredClone(input), response = await nativeRecordResponse('/hub/files/update', captured, signal)
    let body: unknown = null
    try { const text = await response.text(); if (text) body = parseNativeJson<unknown>(text) } catch { /* Preserve actual HTTP status and inspection header. */ }
    const envelope = object(body) ? body : null
    const inspection = response.status === 413 || response.headers.get('X-Graphit-Inspection-Required') === 'request-selection' || envelope?.inspection_required === true
    if (!response.ok || object(envelope?.error) || envelope?.status === 'error') {
      const error = object(envelope?.error) ? envelope.error : null
      const observed = error ? envelope?.outcome : undefined
      const outcome = validUpdateAllOutcome(observed, captured) ? observed : null
      const earlySummary = validUpdateAllEarlySummary(observed, captured) ? observed : null
      const summary = earlySummary ?? (observed === undefined && object(envelope?.effects) ? envelope.effects
        : object(observed) && observed.operation === undefined && observed.scope === undefined && observed.items === undefined ? observed : null)
      const correlated = error !== null && typeof error.code === 'string' && error.code.length > 0 && (observed === undefined || outcome !== null || (summary !== null && validUpdateAllEffects(summary)))
      throw new HubUpdateAllError(response.status, error && typeof error.code === 'string' ? error.code : response.ok ? 'invalid_response' : null, outcome,
        summary && validUpdateAllEffects(summary) ? summary : null, inspection || !correlated, captured, correlated, correlated && earlySummary !== null)
    }
    if (!validUpdateAllOutcome(envelope, captured) || envelope.status !== 'ok') throw new HubUpdateAllError(response.status, 'invalid_response', null, null, true, captured, false)
    return envelope
  },
  async mutate(operation: FileOperation, input: MutationSelection, signal?: AbortSignal) {
    const captured = structuredClone(input)
    const local = 'source' in captured
    if ((operation === 'install_local') !== local) throw new Error('Installation source does not match the operation')
    const result = await request<FileOutcome>(local ? '/hub/authored/install' : '/hub/files/' + operation, captured, signal)
    const artifact = mutationArtifact(captured)
    const versionMatches = 'selection' in captured
      ? typeof result.version === 'string' && result.version.length > 0 && (!captured.selection.exact || result.version === artifact.version)
      : artifact.version === undefined || result.version === artifact.version
    if (result.operation !== operation || !sameScope(result.scope, captured.scope)
      || result.id !== artifact.id || result.artifact_type !== artifact.artifact_type
      || result.binding !== captured.binding || !versionMatches
      || ('selection' in captured && result.producer !== captured.selection.producer)
      || (local && (result.source !== 'local' || result.cache !== 'not_attempted' || result.producer !== captured.source.project_id
        || result.project !== captured.scope.project_id || result.project_version !== captured.scope.project_version))) {
      throw new HubRequestError(200, 'invalid_response', result, true, result, captured)
    }
    return result
  },
}

const observedCacheChanges = new Set(['Created', 'Unchanged', 'Removed', 'Updated', 'not_attempted'])

export function effectsNeedInspection(effects: FileEffects): boolean {
  const publication = effects.publication
  return effects.cleanup_pending === true || effects.stage_cleanup_pending === true || effects.blocked_by_pending === true
    || effects.observation_failed === true || (effects.cache !== undefined && !observedCacheChanges.has(effects.cache))
    || publication === 'Unknown'
    || (typeof publication === 'object' && publication !== null && (publication.committed === null
      || publication.hub_pending === true || publication.core_pending === true || publication.cleanup_pending === true || publication.observation_failed === true || publication.change === 'Unknown'))
}

const confirmedPublicationChanges = new Set([
  'Physical(Created)', 'Physical(Unchanged)', 'Physical(Removed)', 'Physical(Updated)',
  'MetadataOnly', 'MetadataRemoved', 'VerifiedUnchanged', 'Forgotten', 'Recovered',
])

export const mutationCompleted = (outcome: FileOutcome) => {
  const cacheConfirmed = outcome.operation === 'install_local' ? outcome.cache === 'not_attempted' : outcome.operation === 'install' || outcome.operation === 'update'
    ? outcome.cache === 'Created' || outcome.cache === 'Unchanged' || outcome.cache === 'Updated'
    : outcome.cache === undefined || outcome.cache === 'not_attempted'
  return outcome.status === 'ok' && cacheConfirmed && publicationConfirmed(outcome) && !effectsNeedInspection(outcome)
}

export function sourceSelection(origin: HubOrigin): HubSource {
  return origin.transport === 'broker' ? { transport: 'broker', provider: origin.provider }
    : { transport: 'standalone', provider: origin.provider, ...(origin.profile ? { profile: origin.profile } : {}) }
}

export const installedFileKey = (entry: InstalledFile) => JSON.stringify([entry.artifact_type, entry.id, entry.version, entry.producer, entry.target, entry.source_origin, entry.origin, entry.alias, entry.entry_name])

async function publicationResponse(response: Response, captured: FilePublicationInput): Promise<FilePublicationOutcome>
async function publicationResponse(response: Response, captured: PackageUploadMetadata): Promise<PackagePublicationOutcome>
async function publicationResponse(response: Response, captured: FilePublicationInput | PackageUploadMetadata): Promise<PackagePublicationOutcome> {
  const text = await response.text()
  let body: unknown = null
  try { if (text) body = parseNativeJson<unknown>(text) } catch { /* Preserve HTTP status and the inspection header. */ }
  const envelope = object(body) ? body : null
  const inspection = response.headers.get('X-Graphit-Inspection-Required') === 'request-selection' || envelope?.inspection_required === true
  const error = envelope?.error
  const effects = object(envelope?.outcome) ? envelope.outcome : object(envelope?.effects) ? envelope.effects : null
  if (!response.ok) throw new HubPublicationError(response.status, object(error) && typeof error.code === 'string' ? error.code : null, effects as PublicationEffects | null, inspection, captured)
  const receipt = envelope?.publication
  const native = 'format' in captured.publication && captured.publication.format === 'native'
  if (envelope?.status !== 'published' || !object(receipt)
    || receipt.producer !== captured.project_id || receipt.artifact_type !== captured.publication.artifact_type
    || receipt.id !== captured.publication.id || receipt.version !== captured.publication.version
    || receipt.catalog !== 'published' || (envelope.project_metadata !== 'published' && envelope.project_metadata !== 'unchanged')
    || Object.keys(receipt).some(key => !(native ? ['producer', 'artifact_type', 'id', 'version', 'root_sha256', 'catalog'] : ['producer', 'artifact_type', 'id', 'version', 'archive_sha256', 'payload', 'catalog']).includes(key))
    || (native ? typeof receipt.root_sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(receipt.root_sha256) || 'payload' in receipt || 'archive_sha256' in receipt
      : typeof receipt.archive_sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(receipt.archive_sha256) || receipt.payload !== 'published' || 'root_sha256' in receipt)) {
    throw new HubPublicationError(response.status, 'invalid_response', envelope as PublicationEffects | null, true, captured)
  }
  return envelope as unknown as PackagePublicationOutcome
}

function validUpdateAllEarlySummary(value: unknown, input: UpdateAllInput): value is FileEffects & JsonObject {
  if (!object(value) || value.status !== 'error' || value.operation !== 'update' || value.cache !== 'not_attempted'
    || value.publication !== 'not_attempted' || !object(value.scope) || !sameScope(value.scope as unknown as HubScope, input.scope)) return false
  const scopeKeys = input.scope.kind === 'project' ? ['kind', 'project_id', 'project_version'] : ['kind']
  return Object.keys(value).every(key => ['status', 'operation', 'scope', 'cache', 'publication'].includes(key))
    && Object.keys(value.scope).every(key => scopeKeys.includes(key))
}
function validUpdateAllEffects(value: unknown): value is FileEffects & Partial<Omit<FileOutcome, 'operation' | 'scope'>> & JsonObject {
  if (!object(value) || (value.status !== undefined && value.status !== 'ok' && value.status !== 'error')) return false
  if (value.cache !== undefined && typeof value.cache !== 'string') return false
  if (value.publication !== undefined && typeof value.publication !== 'string') {
    if (!object(value.publication)) return false
    for (const field of ['change']) if (value.publication[field] !== undefined && typeof value.publication[field] !== 'string') return false
    for (const field of ['committed', 'hub_pending', 'core_pending', 'cleanup_pending', 'observation_failed']) {
      const fact = value.publication[field]
      if (fact !== undefined && typeof fact !== 'boolean' && !(field === 'committed' && fact === null)) return false
    }
  }
  for (const field of ['cleanup_pending', 'stage_cleanup_pending', 'blocked_by_pending', 'observation_failed', 'payload_preserved']) if (value[field] !== undefined && typeof value[field] !== 'boolean') return false
  if (value.possible_parents !== undefined && (!Array.isArray(value.possible_parents) || !value.possible_parents.every(validUpdateAllPath))) return false
  if (value.forget_preview !== undefined && value.forget_preview !== null) return false
  return true
}
function validUpdateAllPath(value: unknown): value is string {
  if (typeof value !== 'string' || !value || value.includes('\0')) return false
  try { encodeURIComponent(value); return true } catch { return false }
}
function validUpdateAllTarget(value: unknown): value is InstalledFile['target'] {
  return object(value) && (value.kind === 'global_cache' ? Object.keys(value).length === 1
    : value.kind === 'native_binding' ? nativeBindings.includes(value.binding as NativeBinding) && Object.keys(value).every(key => key === 'kind' || key === 'binding')
      : value.kind === 'shared_adapter' && typeof value.adapter === 'string' && value.adapter.length > 0 && Object.keys(value).every(key => key === 'kind' || key === 'adapter'))
}
function validUpdateAllOutcome(value: unknown, input: UpdateAllInput): value is UpdateAllOutcome {
  if (!object(value) || (value.status !== 'ok' && value.status !== 'error') || value.operation !== 'update_all' || !object(value.scope)
    || !sameScope(value.scope as unknown as HubScope, input.scope) || value.global !== (input.scope.kind === 'global_cache') || !Array.isArray(value.items)) return false
  let stopped = false
  for (const item of value.items) {
    if (!object(item) || !fileKinds.includes(item.artifact_type as FileKind) || !['id', 'producer', 'previous_version'].every(key => typeof item[key] === 'string' && item[key].length > 0)
      || !validUpdateAllTarget(item.target) || (input.scope.kind === 'global_cache' && item.target.kind !== 'global_cache')
      || (input.scope.kind === 'project' && (item.target.kind === 'global_cache' || (item.target.kind === 'native_binding' && item.target.binding !== input.binding)))) return false
    if (item.status === 'not_attempted') { if ('outcome' in item || 'code' in item) return false; stopped = true; continue }
    if (stopped || (item.status !== 'ok' && item.status !== 'error') || !validUpdateAllEffects(item.outcome) || item.outcome.status !== item.status
      || item.outcome.cache === undefined || item.outcome.publication === undefined) return false
    const effects = item.outcome
    if (['id', 'producer', 'artifact_type'].some(key => effects[key] !== undefined && effects[key] !== item[key])
      || ('operation' in effects) || ('scope' in effects)
      || (effects.version !== undefined && (typeof effects.version !== 'string' || !effects.version))
      || (effects.project !== undefined && (input.scope.kind !== 'project' || effects.project !== input.scope.project_id))
      || (effects.project_version !== undefined && (input.scope.kind !== 'project' || effects.project_version !== input.scope.project_version))
      || (effects.binding !== undefined && effects.binding !== input.binding)) return false
    if (item.status === 'error') { if (!updateAllItemCodes.includes(item.code as typeof updateAllItemCodes[number])) return false; stopped = true }
    else if ('code' in item) return false
  }
  return value.status === 'error' || value.items.every(item => item.status === 'ok')
}
export function updateAllNeedsInspection(outcome: UpdateAllOutcome): boolean {
  return outcome.items.some(item => item.outcome !== undefined && effectsNeedInspection(item.outcome))
}
export function updateAllCompleted(outcome: UpdateAllOutcome): boolean {
  return outcome.status === 'ok' && outcome.items.every(item => item.status === 'ok' && item.outcome.status === 'ok'
    && ['Created', 'Unchanged', 'Updated'].includes(item.outcome.cache ?? '') && publicationConfirmed(item.outcome) && !effectsNeedInspection(item.outcome))
}
function publicationConfirmed(effects: FileEffects): boolean {
  const publication = typeof effects.publication === 'string' ? effects.publication : effects.publication?.change
  return publication !== undefined && confirmedPublicationChanges.has(publication)
}
