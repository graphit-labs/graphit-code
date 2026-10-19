import { parseNativeJson, stringifyNativeJson } from './nativeJson'
import type { KnowledgeReferenceEdge, KnowledgeReferenceSource, KnowledgeReferenceTarget, ReferencePage, ReferenceRead } from './nativeRecordReferences'

const API = () => window.__API_BASE__ ?? ''
export type LiveState = 'preparing' | 'ready' | 'running' | 'failed' | 'closed'
export interface LiveProjectSelection { project_id: string; project_version: string }
export interface LiveArtifactSelection {
  source: string
  project_id: string | null
  instance_id: string | null
  project_kind: string | null
  type: string
  id: string
  version: string
}
export interface LiveCursor { generation: string; sequence: string }
export interface LiveSession {
  id: string
  project: LiveProjectSelection
  binding: string
  agent: LiveArtifactSelection
  artifacts: LiveArtifactSelection[]
  title: string | null
  model: string
  state: LiveState
  cursor: LiveCursor
  created_at: string
  updated_at: string
}
export type LiveFailure = 'cancelled' | 'deadline' | 'permission_denied' | 'unavailable' | 'invalid_context' | 'native' | 'storage' | 'interrupted' | 'cleanup_incomplete' | 'limit' | 'corrupt'
export type LiveOutcome = 'completed' | 'cancelled' | 'failed'
export type LiveEventData =
  | { type: 'state'; state: LiveState }
  | { type: 'prep'; message: string; artifact: LiveArtifactSelection | null }
  | { type: 'prompt' | 'text' | 'thinking' | 'stdout' | 'stderr'; text: string }
  | { type: 'tool_use'; tool_call_id: string; name: string | null; title: string; input: unknown }
  | { type: 'tool_result'; tool_call_id: string; output: unknown; is_error: boolean }
  | { type: 'error'; code: LiveFailure }
  | { type: 'turn_done'; outcome: LiveOutcome; joined: boolean; clean: boolean }
export interface LiveEvent { sequence: string; turn_id: string | null; data: LiveEventData }
export interface LiveTerminal {
  turn_id: string
  outcome: LiveOutcome
  error: LiveFailure | null
  joined: boolean
  clean: boolean
  persisted: boolean
}
export interface LiveDetail { session: LiveSession; events: LiveEvent[]; terminal: LiveTerminal | null }
export interface LiveOpening { session: LiveSession; after: LiveCursor; terminal: LiveTerminal | null }
export interface LiveKnowledgeCandidate {
  title: string
  path: string
  source_path: string
  context: LiveArtifactSelection
}
export interface LiveKnowledgeReferences {
  kind: 'native' | 'imported'
  data: Omit<ReferenceRead, 'focused_source' | 'outgoing' | 'incoming'> & {
    focused_source: KnowledgeReferenceSource | null
    outgoing: ReferencePage<KnowledgeReferenceEdge> | null
    incoming: ReferencePage<KnowledgeReferenceEdge> | null
  }
}
export interface LiveKnowledgePage extends LiveKnowledgeCandidate {
  content: string
  generation: string
  refs: LiveKnowledgeReferences
}
export type LiveKnowledgeResult = LiveKnowledgePage | { generation: string; candidates: LiveKnowledgeCandidate[] }
export interface LiveKnowledgeReference {
  namespace: KnowledgeReferenceTarget['namespace']
  witness: KnowledgeReferenceSource['witness']
}
export interface LiveKnowledgeQuery { context?: LiveArtifactSelection; reference?: LiveKnowledgeReference }
export interface CreateLiveSessionRequest {
  request_id: string
  project: LiveProjectSelection
  binding: string
  agent: LiveArtifactSelection
  artifacts: LiveArtifactSelection[]
  title?: string | null
  prompt?: string | null
  model: string
}
export interface SendLiveMessageRequest { turn_id: string; text: string }

export class LiveError extends Error {
  constructor(public readonly status: number | null, public readonly code: string | null = null, public readonly journalEffect: string | null = null) {
    super('Live request failed (' + (status === null ? code || 'stream' : 'HTTP ' + status + (code ? ', ' + code : '')) + ')')
    this.name = 'LiveError'
  }
}
const protocolError = () => new LiveError(null, 'invalid_live_response')
function sequence(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > 18446744073709551615n) throw protocolError()
}
function cursor(value: LiveCursor) {
  if (!value || typeof value.generation !== 'string' || !/^[a-fA-F0-9]+$/.test(value.generation)) throw protocolError()
  sequence(value.sequence)
}
function session(value: LiveSession) { cursor(value.cursor); return value }
const eventTypes = new Set(['state', 'prep', 'prompt', 'text', 'thinking', 'tool_use', 'tool_result', 'stdout', 'stderr', 'error', 'turn_done'])
function event(value: LiveEvent) {
  sequence(value.sequence)
  if (!value.data || !eventTypes.has(value.data.type)) throw protocolError()
  return value
}
export async function response<T>(r: Response): Promise<T> {
  if (!r.ok) {
    let code: string | null = null, effect: string | null = null
    try {
      const value = parseNativeJson<{ error?: { code?: unknown; journal_effect?: unknown } }>(await r.text())
      if (typeof value.error?.code === 'string') code = value.error.code
      if (typeof value.error?.journal_effect === 'string') effect = value.error.journal_effect
    } catch { /* Preserve the actual HTTP status without displaying an untrusted response body. */ }
    throw new LiveError(r.status, code, effect)
  }
  return parseNativeJson<T>(await r.text())
}
const sessionURL = (id: string) => API() + '/api/live/sessions/' + encodeURIComponent(id)
const mutation = { credentials: 'same-origin' as const, headers: { 'X-Graphit-Request': 'ui' } }

export async function createLiveSession(req: CreateLiveSessionRequest): Promise<LiveSession> {
  const body = stringifyNativeJson(req)
  return session(await response(await fetch(API() + '/api/live/sessions', {
    ...mutation, method: 'POST', headers: { ...mutation.headers, 'Content-Type': 'application/json' }, body,
  })))
}
export async function listLiveSessions(signal?: AbortSignal): Promise<LiveSession[]> {
  const value = await response<{ sessions: LiveSession[] }>(await fetch(API() + '/api/live/sessions', { credentials: 'same-origin', signal }))
  return value.sessions.map(session)
}
export async function getLiveSession(id: string, signal?: AbortSignal): Promise<LiveDetail> {
  const value = await response<LiveDetail>(await fetch(sessionURL(id), { credentials: 'same-origin', signal }))
  session(value.session)
  if (value.session.id !== id) throw protocolError()
  let previous: bigint | undefined
  for (const item of value.events) {
    event(item)
    const current = BigInt(item.sequence)
    if ((previous !== undefined && current <= previous) || current > BigInt(value.session.cursor.sequence)) throw protocolError()
    previous = current
  }
  return value
}
export async function getLiveKnowledgePage(id: string, generation: string, page: string, query: LiveKnowledgeQuery = {}, signal?: AbortSignal): Promise<LiveKnowledgeResult> {
  const params = new URLSearchParams({ generation, page })
  if (query.context !== undefined) params.set('context', stringifyNativeJson(query.context))
  if (query.reference !== undefined) params.set('reference', stringifyNativeJson(query.reference))
  const value = await response<LiveKnowledgeResult>(await fetch(sessionURL(id) + '/knowledge/page?' + params, { credentials: 'same-origin', signal }))
  if (value.generation !== generation) throw protocolError()
  return value
}
export async function removeLiveSession(id: string): Promise<{ deleted: boolean }> {
  return response(await fetch(sessionURL(id), { ...mutation, method: 'DELETE' }))
}
export async function sendLiveMessage(id: string, request: SendLiveMessageRequest): Promise<LiveSession> {
  return session(await response(await fetch(sessionURL(id) + '/messages', {
    ...mutation, method: 'POST', headers: { ...mutation.headers, 'Content-Type': 'application/json' }, body: stringifyNativeJson(request),
  })))
}
export async function cancelLiveTurn(id: string): Promise<{ requested: boolean }> {
  return response(await fetch(sessionURL(id) + '/cancel', { ...mutation, method: 'POST' }))
}
export async function closeLiveSession(id: string): Promise<LiveSession> {
  const value = await response<{ session: LiveSession }>(await fetch(sessionURL(id) + '/close', { ...mutation, method: 'POST' }))
  if (value.session.id !== id || value.session.state !== 'closed') throw protocolError()
  return session(value.session)
}

export interface LiveSubscription { close: () => void }
export interface LiveHandlers {
  onEvent: (event: LiveEvent) => void
  onOpen?: (opening: LiveOpening) => void
  onTerminal?: (terminal: LiveTerminal) => void
  onClosed?: (session: LiveSession) => void
  onError?: (error?: Error) => void
}
export function subscribeLiveEvents(id: string, after: LiveCursor, handlers: LiveHandlers): LiveSubscription {
  cursor(after)
  let accepted = { ...after }, active = true, source: EventSource | undefined
  let retry: ReturnType<typeof setTimeout> | undefined
  const close = () => { active = false; clearTimeout(retry); source?.close() }
  const fail = (error: Error) => { close(); handlers.onError?.(error) }
  const connect = () => {
    if (!active) return
    const params = new URLSearchParams(accepted)
    const current = new EventSource(sessionURL(id) + '/events?' + params, { withCredentials: true })
    source = current
    let opened = false
    const receive = <T>(name: string, consume: (value: T, frame: MessageEvent) => void) => current.addEventListener(name, raw => {
      if (!active || source !== current) return
      try { consume(parseNativeJson<T>((raw as MessageEvent).data as string), raw as MessageEvent) }
      catch { fail(protocolError()) }
    })
    receive<LiveOpening>('opened', value => {
      session(value.session); cursor(value.after)
      if (value.session.id !== id || value.after.generation !== accepted.generation || value.session.cursor.generation !== accepted.generation || value.after.sequence !== accepted.sequence) throw protocolError()
      opened = true
      // The observed head is not the replay filter. Advance only after accepting events.
      handlers.onOpen?.(value)
    })
    receive<LiveEvent>('event', (value, frame) => {
      event(value)
      if (!opened || frame.lastEventId !== accepted.generation + ':' + value.sequence) throw protocolError()
      if (BigInt(value.sequence) <= BigInt(accepted.sequence)) return
      handlers.onEvent(value)
      accepted = { generation: accepted.generation, sequence: value.sequence }
    })
    receive<LiveTerminal>('terminal', value => {
      if (!opened) throw protocolError()
      handlers.onTerminal?.(value)
    })
    receive<LiveSession>('closed', value => {
      session(value)
      if (!opened || value.id !== id || value.state !== 'closed' || value.cursor.generation !== accepted.generation) throw protocolError()
      close()
      handlers.onClosed?.(value)
    })
    current.onerror = raw => {
      if (!active || source !== current) return
      if (raw instanceof MessageEvent) {
        try {
          const value = parseNativeJson<{ error: { code: string; journal_effect?: string } }>(raw.data as string)
          if (typeof value.error?.code !== 'string') throw protocolError()
          fail(new LiveError(null, value.error.code, value.error.journal_effect ?? null))
        } catch { fail(protocolError()) }
        return
      }
      // EventSource exposes no HTTP status here. CLOSED means the browser refused retry.
      if (current.readyState === EventSource.CLOSED) { fail(new LiveError(null, 'stream_closed')); return }
      // Native automatic retry would combine our query cursor with Last-Event-ID.
      current.close()
      source = undefined
      handlers.onError?.()
      retry = setTimeout(connect, 1000)
    }
  }
  connect()
  return { close }
}
