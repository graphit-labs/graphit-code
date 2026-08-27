/**
 * Live search: a server-side session you subscribe to, not a request that returns an
 * answer.
 *
 * The work outlives any single request — preparing the throwaway project takes most of
 * the time, and the agent then runs for minutes — so the browser's job is to watch a
 * session, not to hold a connection open until an answer appears. Watching is
 * Server-Sent Events; sending is an ordinary POST, because that half is small.
 *
 * Closing the tab does not stop the run. Reopening it replays what was missed.
 */

const API = () => window.__API_BASE__ ?? ''

/** What a session is currently doing. */
export type LiveState = 'preparing' | 'ready' | 'running' | 'failed' | 'closed'

/** One Hub artifact chosen for a session. Any type the Hub carries is allowed. */
export interface LiveArtifact {
  id: string
  /** Omitted lets the registry resolve it; required when an ID exists under several types. */
  type?: string
  version?: string
}

export interface LiveSession {
  id: string
  state: LiveState
  ide: string
  title?: string
  artifacts?: LiveArtifact[]
  created_at: string
  updated_at: string
  /** Why a failed session failed. */
  error?: string
  /**
   * Where the event log currently ends.
   *
   * Optional because the list endpoint omits it: a session that is not live in the
   * server process has no current sequence to report, and inventing one would name a
   * resume point that does not exist. Present on create, send and single-session reads.
   */
  last_seq?: number
}

/**
 * Everything that can happen in a session.
 *
 * `text` is the answer and arrives in chunks that concatenate — a chunk is not a line
 * and not a sentence. `prompt` echoes the question into the log, which is what lets a
 * reconnecting client rebuild the whole transcript from the log alone.
 */
export type LiveEventKind =
  | 'state' | 'prep' | 'prompt' | 'text' | 'thinking'
  | 'tool_use' | 'tool_result' | 'stderr' | 'error' | 'turn_done'

export interface LiveEvent {
  /** Monotonic within a session, and the SSE event id. */
  seq: number
  kind: LiveEventKind
  text?: string
  tool?: string
  detail?: string
  state?: LiveState
  at: string
}

/**
 * Every kind, listed because EventSource has no wildcard: a named event reaches only
 * the listener registered for that name, never onmessage. The list is safe to
 * enumerate here because this UI is compiled into the same binary as the server that
 * produces the events, so the two cannot disagree about what exists.
 */
const EVENT_KINDS: LiveEventKind[] = [
  'state', 'prep', 'prompt', 'text', 'thinking',
  'tool_use', 'tool_result', 'stderr', 'error', 'turn_done',
]

export interface CreateLiveSessionRequest {
  /** Required: it decides which rules, skills and MCP configuration the agent finds. */
  ide: string
  title?: string
  artifacts?: LiveArtifact[]
  /** Asked as soon as the workspace is ready, without the client having to wait for it. */
  prompt?: string
}

/** An error carrying the status, so a caller can tell "ask again later" from "gone". */
export class LiveError extends Error {
  constructor(public status: number, message: string) {
    super(message)
    this.name = 'LiveError'
  }
}

async function refuse(r: Response): Promise<never> {
  const body = (await r.text()).trim()
  throw new LiveError(r.status, body || `HTTP ${r.status}`)
}

export async function createLiveSession(req: CreateLiveSessionRequest): Promise<LiveSession> {
  const r = await fetch(`${API()}/api/live/sessions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!r.ok) return refuse(r)
  return r.json() as Promise<LiveSession>
}

export async function listLiveSessions(): Promise<LiveSession[]> {
  const r = await fetch(`${API()}/api/live/sessions`)
  if (!r.ok) return refuse(r)
  return r.json() as Promise<LiveSession[]>
}

export async function getLiveSession(id: string): Promise<LiveSession> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}`)
  if (!r.ok) return refuse(r)
  return r.json() as Promise<LiveSession>
}

/** Deletes the session and its throwaway project. This is the remove button. */
export async function removeLiveSession(id: string): Promise<void> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!r.ok) return refuse(r)
}

/**
 * Asks a question. The answer is not in the response and never will be: it arrives on
 * the stream, which is the only place a turn's output exists.
 */
export async function sendLiveMessage(id: string, prompt: string): Promise<LiveSession> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ prompt }),
  })
  if (!r.ok) return refuse(r)
  return r.json() as Promise<LiveSession>
}

/** Stops the turn in progress and leaves the session usable. */
export async function cancelLiveTurn(id: string): Promise<void> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}/cancel`, { method: 'POST' })
  if (!r.ok) return refuse(r)
}

export interface LiveSubscription {
  close: () => void
}

/**
 * Subscribes to a session's events, from `afterSeq` onwards.
 *
 * The starting point goes in the query string rather than a header because EventSource
 * cannot set headers. The browser does send Last-Event-ID by itself, but only on a
 * reconnect it initiated — a page that was reloaded has to say where it got to.
 *
 * Reconnection is the browser's job and is left to it: the server sends a retry hint
 * and the ids needed to resume, so a dropped connection recovers without this code
 * doing anything. `onError` is for telling the user the stream is quiet, not for
 * reconnecting.
 */
export function subscribeLiveEvents(
  id: string,
  afterSeq: number,
  handlers: { onEvent: (ev: LiveEvent) => void; onError?: () => void; onOpen?: () => void },
): LiveSubscription {
  const params = new URLSearchParams()
  if (afterSeq > 0) params.set('last_event_id', String(afterSeq))
  const qs = params.toString()
  const url = `${API()}/api/live/sessions/${encodeURIComponent(id)}/stream${qs ? `?${qs}` : ''}`

  const source = new EventSource(url)
  const dispatch = (e: MessageEvent) => {
    try {
      handlers.onEvent(JSON.parse(e.data as string) as LiveEvent)
    } catch {
      // A frame we cannot read is one event lost, not a reason to tear down a
      // subscription that is otherwise delivering.
    }
  }
  for (const kind of EVENT_KINDS) {
    source.addEventListener(kind, dispatch as EventListener)
  }
  if (handlers.onOpen) source.onopen = handlers.onOpen
  if (handlers.onError) source.onerror = handlers.onError

  return { close: () => source.close() }
}

// There is deliberately no IDE lookup here. The IDE is one application-wide setting,
// held in the app store, chosen in the project switcher and persisted with the rest of
// the state — the store already loads it from /api/global-projects along with the
// projects. A second fetch of the same endpoint would be a second answer to the same
// question, and the moment they disagreed a session would be prepared for conventions
// the user is not working in.
