import type { ReferenceBinding, ReferenceFocus, ReferenceReader, ReferenceSource } from './nativeRecordReferences'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'
import { importedKnowledgeHref, importedSourceKey, projectRecordHref, requestedImportedKnowledgeSource, userMemoryHref } from './nativeRecordNavigation'

export const referenceSourceKey = (source: ReferenceSource) => source.kind === 'imported_knowledge' ? importedSourceKey(source) : source.kind
export const referenceFocusKey = (focus: ReferenceFocus) => stringifyNativeJson([focus.kind, focus.scope, focus.id, focus.context ?? null])

function checkedFocus(value: unknown): ReferenceFocus {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('The reference focus is invalid')
  const focus = value as Record<string, unknown>, scope = focus.scope as Record<string, unknown> | undefined
  if (Object.keys(focus).some(k => !['kind', 'scope', 'id', 'context'].includes(k)) || typeof focus.kind !== 'string' || !['task', 'session', 'memory', 'knowledge'].includes(focus.kind) || typeof focus.id !== 'string' ||
    (Object.prototype.hasOwnProperty.call(focus, 'context') && (focus.kind !== 'knowledge' || typeof focus.context !== 'string')) ||
    !scope || typeof scope !== 'object' || Array.isArray(scope) || Object.keys(scope).length !== 1 || !Object.keys(scope).every(k => (k === 'project' || k === 'user') && typeof scope[k] === 'string')) throw new Error('The reference focus is invalid')
  return value as ReferenceFocus
}

function checkedSource(value: unknown): ReferenceSource {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('The reference source is invalid')
  const source = value as Record<string, unknown>
  if (source.kind === 'imported_knowledge') return requestedImportedKnowledgeSource(new URLSearchParams({ library: 'imported', source: stringifyNativeJson(value) }).toString())!
  if (Object.keys(source).length !== 1 || typeof source.kind !== 'string' || !['project_task', 'project_memory', 'project_knowledge', 'user_memory'].includes(source.kind)) throw new Error('The reference source is invalid')
  return value as ReferenceSource
}

export interface ExpectedReference { focus: ReferenceFocus; source: ReferenceSource }
export function requestedReference(search: string): ExpectedReference | null {
  const params = new URLSearchParams(search), focuses = params.getAll('reference_focus'), sources = params.getAll('reference_source')
  if (!focuses.length && !sources.length) return null
  if (focuses.length !== 1 || sources.length !== 1) throw new Error('The reference destination is ambiguous')
  return { focus: checkedFocus(parseNativeJson(focuses[0])), source: checkedSource(parseNativeJson(sources[0])) }
}
export function referenceBindingMatches(expected: ExpectedReference | null, observed: ReferenceBinding | null): boolean {
  return !expected || !!observed && referenceFocusKey(expected.focus) === referenceFocusKey(observed.focus) && referenceSourceKey(expected.source) === referenceSourceKey(observed.outgoing_source)
}
export function referenceDestinationError(search: string, observed: ReferenceBinding | null, verifyBinding: boolean): string {
  try {
    const expected = requestedReference(search)
    return expected && verifyBinding && !referenceBindingMatches(expected, observed) ? 'The reference destination no longer matches its observed namespace and source.' : ''
  } catch (cause) { return cause instanceof Error ? cause.message : 'The reference destination is invalid' }
}

/** A descriptor supplies a locator. The destination reader still admits and checks it. */
export function referenceReaderHref(reader: ReferenceReader | null): string | null {
  if (!reader || reader.availability !== 'available' || reader.method !== 'POST') return null
  try { checkedFocus(reader.focus); checkedSource(reader.source) } catch { return null }
  const { selection, focus, input, source } = reader
  if (!input || typeof input !== 'object') return null
  let href: string | null = null
  if (reader.path === '/api/tasks/get' || reader.path === '/api/tasks/session' || reader.path === '/api/memory/get') {
    const kind = reader.path === '/api/tasks/get' ? 'task' : reader.path === '/api/tasks/session' ? 'session' : 'memory'
    const sourceKind = kind === 'memory' ? 'project_memory' : 'project_task'
    if (source.kind === sourceKind && selection && focus.kind === kind && 'project' in focus.scope && focus.scope.project === selection.project_id && 'id' in input && input.id === focus.id && Object.keys(input).length === 1) href = projectRecordHref(kind, input.id, selection)
  } else if (reader.path === '/api/user-memory/get') {
    if (source.kind === 'user_memory' && selection === null && focus.kind === 'memory' && 'user' in focus.scope && 'id' in input && input.id === focus.id && Object.keys(input).length === 1) href = userMemoryHref(input.id, focus.scope.user)
  } else if (reader.path === '/api/knowledge/page') {
    if (source.kind === 'project_knowledge' && selection && focus.kind === 'knowledge' && 'slug' in input && input.slug === focus.id && Object.keys(input).length === 1) href = projectRecordHref('knowledge', input.slug, selection)
  } else if (reader.path === '/api/knowledge/imported/page') {
    if (source.kind === 'imported_knowledge' && selection === null && focus.kind === 'knowledge' && 'source' in input && input.slug === focus.id && Object.keys(input).length === 2) {
      try { const inputSource = checkedSource(input.source); if (referenceSourceKey(inputSource) === referenceSourceKey(source)) href = importedKnowledgeHref(source, input.slug) } catch { return null }
    }
  }
  if (href === null) return null
  const url = new URL(href, 'http://native-route.invalid')
  url.searchParams.set('reference_focus', stringifyNativeJson(focus))
  url.searchParams.set('reference_source', stringifyNativeJson(source))
  return url.pathname + url.search
}
