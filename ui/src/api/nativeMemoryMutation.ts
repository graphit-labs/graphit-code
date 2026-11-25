import { nativeRecordResponse, NativeRecordReadError } from './nativeRecordRead'
import { parseNativeJson, type NativeInteger } from './nativeJson'
import { nativeSelectionKey } from './nativeRecordNavigation'
import type { MemoryEnvelope, MemoryReadScope, NativeMemory, MemoryReference, UserMemoryGeneration } from './nativeMemory'

export interface MemoryDraft { title: string; body: string; classification: NativeMemory['attributes']['classification']; important: boolean; mandatory: boolean; tags: string[]; reason: string }
export interface MemoryCreateInput { idempotency_key: string; title: string; body: string; attributes: NativeMemory['attributes']; tags: string[]; references?: MemoryReference[] }
export interface MemoryUpdateInput { id: string; expected_revision: NativeInteger; idempotency_key: string; reason: string; title?: string; body?: string; classification?: NativeMemory['attributes']['classification']; important?: boolean; mandatory?: boolean; tags?: string[]; references?: MemoryReference[] }
export interface MemoryDeleteInput { id: string; expected_revision: NativeInteger; idempotency_key: string; reason: string }
export type MutationOutcome = 'completed' | 'confirmed' | 'uncertain' | 'not_applied'
export interface MemoryRecoveryKey { corpus_uri: string; physical_branch: string | null; base_version: NativeInteger; transaction_id: string }
export interface MemoryIndexPublication { sdk_version: NativeInteger; generation: UserMemoryGeneration | null; recovery_key: MemoryRecoveryKey }
export interface MemoryReceipt {
  state: MutationOutcome; id: string | null; revision: NativeInteger | null; idempotency_key: string; sdk_version: NativeInteger | null
  generation: UserMemoryGeneration | null; recovery_key: MemoryRecoveryKey | null; index_publication?: MemoryIndexPublication
  completion_error: string | null; delivery_error: string | null
}
export interface MemoryHeaderReceipt { index_sdk_version?: NativeInteger; state: MutationOutcome; id: string | null; revision: NativeInteger | null; sdk_version: NativeInteger | null; response_error: string | null; completion_error: string | null; delivery_error: string | null }
export type MemoryMutationResult = { delivery: 'receipt'; envelope: MemoryEnvelope<MemoryReceipt> } | { delivery: 'headers'; http_status: number; receipt: MemoryHeaderReceipt }
export class MemoryMutationError extends NativeRecordReadError {
  constructor(status: number, code: string | null, public readonly expected: NativeInteger | null = null, public readonly actual: NativeInteger | null = null) {
    super(status, code)
    this.message = code === 'revision_conflict' ? `Memory revision conflict (expected ${expected}, actual ${actual}). Refresh the record before editing again.` : `Memory request failed (HTTP ${status}${code ? ', ' + code : ''})`
  }
}

function decimalHeader(response: Response, name: string): NativeInteger | null {
  const value = response.headers.get('x-graphit-memory-' + name)
  if (value === null) return null
  if (!/^\d+$/.test(value) || BigInt(value) > 18446744073709551615n) throw new Error('Invalid Memory receipt integer')
  return parseNativeJson<NativeInteger>(value)
}

async function mutate(operation: 'create' | 'update' | 'delete', scope: MemoryReadScope, input: MemoryCreateInput | MemoryUpdateInput | MemoryDeleteInput, signal?: AbortSignal): Promise<MemoryMutationResult> {
  const response = await nativeRecordResponse('/' + (scope.kind === 'project' ? 'memory' : 'user-memory') + '/' + operation, scope.kind === 'project' ? { selection: scope.selection, input } : { input }, signal)
  const outcome = response.headers.get('x-graphit-memory-outcome')
  const indexVersion = decimalHeader(response, 'index-sdk-version')
  if (outcome === 'completed' || outcome === 'confirmed' || outcome === 'uncertain' || outcome === 'not_applied') return { delivery: 'headers', http_status: response.status, receipt: {
    ...(indexVersion === null ? {} : { index_sdk_version: indexVersion }),
    state: outcome, id: response.headers.get('x-graphit-memory-id'), revision: decimalHeader(response, 'revision'), sdk_version: decimalHeader(response, 'sdk-version'),
    response_error: response.headers.get('x-graphit-memory-response-error'), completion_error: response.headers.get('x-graphit-memory-completion-error'), delivery_error: response.headers.get('x-graphit-memory-delivery-error'),
  } }
  const text = await response.text()
  if (!response.ok) {
    let cause: { error?: { code?: string; expected?: NativeInteger; actual?: NativeInteger } } = {}
    try { cause = parseNativeJson(text) } catch { /* A non-JSON HTTP failure still retains its real status. */ }
    throw new MemoryMutationError(response.status, cause.error?.code ?? null, cause.error?.expected ?? null, cause.error?.actual ?? null)
  }
  const envelope = parseNativeJson<MemoryEnvelope<MemoryReceipt>>(text)
  if (!['completed', 'confirmed', 'uncertain', 'not_applied'].includes(envelope.native?.state) || envelope.native.idempotency_key !== input.idempotency_key) throw new Error('Invalid Memory mutation receipt; inspect the original record before retrying')
  if (scope.kind === 'project') {
    if (!('selection' in envelope) || nativeSelectionKey(envelope.selection) !== nativeSelectionKey(scope.selection)) throw new Error('The Memory mutation selection changed; inspect the original context before retrying')
  } else if (!('scope_id' in envelope) || (scope.requested_scope_id !== undefined && envelope.scope_id !== scope.requested_scope_id)) throw new Error('The User Memory mutation namespace changed; inspect the original namespace before retrying')
  return { delivery: 'receipt', envelope }
}

export const nativeMemoryMutationApi = {
  create: (scope: MemoryReadScope, input: MemoryCreateInput, signal?: AbortSignal) => mutate('create', scope, input, signal),
  update: (scope: MemoryReadScope, input: MemoryUpdateInput, signal?: AbortSignal) => mutate('update', scope, input, signal),
  remove: (scope: MemoryReadScope, input: MemoryDeleteInput, signal?: AbortSignal) => mutate('delete', scope, input, signal),
}
