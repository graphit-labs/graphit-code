import type { KnowledgeReferenceEdge, MemoryReferenceEdge, NativeReferenceEdge, ReferenceDirection, ReferenceFocus, ReferenceNavigation, ReferencePage, ReferenceRead, ReferenceSource, ReferenceSourceResult, TaskReferenceEdge } from '@/api/nativeRecordReferences'
import { stringifyNativeJson } from '@/api/nativeJson'

export interface ReferenceEdgeView { raw: NativeReferenceEdge; source: ReferenceFocus | null; sourceId: string; sourceTitle?: string; target: ReferenceFocus; targetTitle?: string; relation: string | null; field: string | null; provenance: string; navigation: ReferenceNavigation | null }
export interface ReferencePageView { native: ReferencePage; edges: ReferenceEdgeView[] }
export interface ReferenceSourceView { source: ReferenceSource; raw: ReferenceSourceResult; state: 'no_corpus' | 'registry_absent' | 'published'; generation: unknown; coverage: Record<string, unknown> | null; warnings: unknown[]; outgoing?: ReferencePageView | null; incoming?: ReferencePageView | null }

export function referenceSourceLabel(source: ReferenceSource): string {
  if (source.kind === 'imported_knowledge') return 'Imported Knowledge: ' + source.artifact.id + ' @ ' + source.artifact.version + (source.artifact.context_alias === undefined ? '' : ' (' + source.artifact.context_alias + ')')
  return { project_task: 'Task and Session', project_memory: 'Project Memory', project_knowledge: 'Project Knowledge', user_memory: 'Personal Memory' }[source.kind]
}
function contextFocus(focus: Omit<ReferenceFocus, 'context'>, context: string | null): ReferenceFocus { return { ...focus, ...(context === null ? {} : { context }) } }
function resolutionTitle(value: TaskReferenceEdge['source_observation'] | MemoryReferenceEdge['source_observation']): string | undefined {
  if (value === 'unresolved') return undefined
  if ('task' in value) return value.task.title
  if ('session' in value) return value.session.title
  return value.memory.title
}
function edgeView(raw: NativeReferenceEdge, navigation: ReferenceNavigation | null, coverage: Record<string, unknown>): ReferenceEdgeView {
  if ('payload' in raw) {
    const edge = raw as KnowledgeReferenceEdge, payload = 'authored' in edge.payload ? edge.payload.authored : edge.payload.wiki_link
    const target = contextFocus({ kind: payload.target.target_kind, scope: payload.target.namespace.scope, id: payload.target.id }, payload.target.namespace.context)
    const source = edge.source.namespace ? contextFocus({ kind: 'knowledge', scope: edge.source.namespace.scope, id: edge.source.slug }, edge.source.namespace.context) : null
    return { raw, source, sourceId: edge.source.slug, sourceTitle: edge.source.title, target,
      targetTitle: navigation?.target_rich?.title ?? (edge.target_observation === 'unresolved' ? undefined : edge.target_observation.knowledge_head.title),
      relation: 'authored' in edge.payload ? edge.payload.authored.relation : null, field: 'authored' in edge.payload ? edge.payload.authored.field : null, provenance: edge.provenance, navigation }
  }
  const ref = raw.reference, scope = ref.scope
  const target = contextFocus({ kind: ref.target_kind, id: ref.id, scope: 'Project' in scope ? { project: scope.Project } : 'User' in scope ? { user: scope.User } : scope }, ref.context)
  let source: ReferenceFocus | null = null, sourceId: string
  if (typeof raw.source === 'string') {
    sourceId = raw.source
    if ('source_scope' in raw && raw.source_scope !== undefined) source = { kind: 'memory', scope: { user: raw.source_scope }, id: sourceId }
    else if (typeof coverage.project_id === 'string') source = { kind: 'memory', scope: { project: coverage.project_id }, id: sourceId }
  } else {
    const kind = 'task' in raw.source ? 'task' : 'session'
    sourceId = 'task' in raw.source ? raw.source.task : raw.source.session
    if (typeof coverage.project_id === 'string') source = { kind, scope: { project: coverage.project_id }, id: sourceId }
  }
  return { raw, source, sourceId, sourceTitle: resolutionTitle(raw.source_observation), target, targetTitle: navigation?.target_rich?.title ?? resolutionTitle(raw.target_resolution), relation: ref.relation, field: ref.field, provenance: raw.provenance, navigation }
}

/** Projection for rendering only. SDK replies, cursors, and edge payloads remain intact. */
export function nativeReferencesView(result: ReferenceSourceResult): ReferenceSourceView {
  const { native, source } = result
  if (native === 'no_corpus') return { source, raw: result, state: 'no_corpus', generation: null, coverage: null, warnings: [] }
  if ('state' in native && native.state === 'registry_absent') return { source, raw: result, state: 'registry_absent', generation: null, coverage: null, warnings: [] }
  let read: ReferenceRead, generation: unknown
  if ('published' in native) { const published = native.published; read = 'read' in published ? published.read : published; generation = published.generation }
  else { read = native.read; generation = native.generation }
  const view: ReferenceSourceView = { source, raw: result, state: 'published', generation, coverage: read.coverage, warnings: read.warnings }
  for (const direction of ['outgoing', 'incoming'] as ReferenceDirection[]) {
    const page = read[direction]
    if (page === undefined || page === null) { view[direction] = page; continue }
    const navigation = result.navigation[direction]
    if (navigation.length !== page.results.length) throw new Error('The reference navigation does not match its native page')
    view[direction] = { native: page, edges: page.results.map((edge, index) => edgeView(edge, navigation[index], read.coverage)) }
  }
  return view
}
export const referenceGenerationKey = (view: ReferenceSourceView) => stringifyNativeJson(view.generation)
