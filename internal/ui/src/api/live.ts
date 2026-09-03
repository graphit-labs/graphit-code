

const API = () => window.__API_BASE__ ?? ''


export type LiveState = 'preparing' | 'ready' | 'running' | 'failed' | 'closed'


export interface LiveArtifact {
  id: string

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

  error?: string

  last_seq?: number
}


export type LiveEventKind =
  | 'state' | 'prep' | 'prompt' | 'text' | 'thinking'
  | 'tool_use' | 'tool_result' | 'stderr' | 'error' | 'turn_done'

export interface LiveEvent {

  seq: number
  kind: LiveEventKind
  text?: string
  tool?: string
  detail?: string
  state?: LiveState
  at: string
}


const EVENT_KINDS: LiveEventKind[] = [
  'state', 'prep', 'prompt', 'text', 'thinking',
  'tool_use', 'tool_result', 'stderr', 'error', 'turn_done',
]

export interface CreateLiveSessionRequest {

  ide: string
  title?: string
  artifacts?: LiveArtifact[]

  prompt?: string
}


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


export async function removeLiveSession(id: string): Promise<void> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!r.ok) return refuse(r)
}


export async function sendLiveMessage(id: string, prompt: string): Promise<LiveSession> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ prompt }),
  })
  if (!r.ok) return refuse(r)
  return r.json() as Promise<LiveSession>
}


export async function cancelLiveTurn(id: string): Promise<void> {
  const r = await fetch(`${API()}/api/live/sessions/${encodeURIComponent(id)}/cancel`, { method: 'POST' })
  if (!r.ok) return refuse(r)
}

export interface LiveSubscription {
  close: () => void
}


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
    } catch { /* ignored */ }
  }
  for (const kind of EVENT_KINDS) {
    source.addEventListener(kind, dispatch as EventListener)
  }
  if (handlers.onOpen) source.onopen = handlers.onOpen
  if (handlers.onError) source.onerror = handlers.onError

  return { close: () => source.close() }
}
