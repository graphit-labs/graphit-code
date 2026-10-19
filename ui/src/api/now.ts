import { getApiBase } from '@/lib/utils'
import { parseNativeJson, stringifyNativeJson, type NativeInteger } from './nativeJson'
import type { TaskContext, TaskSelection } from './task'

export interface NowItem {
 id?: string; slug?: string; title: string; owner?: string; updated_by?: string;
 updated_at?: string | null; lease_expires_at?: string;
 completed_tasks?: NativeInteger; total_tasks?: NativeInteger
}
export interface NowSection {
 availability: 'available' | 'no_corpus' | 'failed'; items: NowItem[];
 total: NativeInteger | null; has_more: boolean | null; error: string | null
}
export interface NowSnapshot { project_id: string; generated_at: string; sections: Record<string, NowSection> }
export interface NowUpdate {
 selection: TaskContext;
 snapshot: { type: 'snapshot'; project_id: string; instances: { instance_id: string; version: string; now: NowSnapshot }[] }
}
export class NowReadError extends Error {
 constructor(public readonly status: number, message: string) { super(message) }
 get retryable() { return ![400, 401, 403, 404].includes(this.status) }
}
/** One owned stream; reconnect decisions belong to its mounted consumer. */
export async function readNow(selection: TaskSelection, signal: AbortSignal, update: (value: NowUpdate) => void): Promise<void> {
 const base = getApiBase().replace(/\/$/, '')
 const response = await fetch(base + (base.endsWith('/api') ? '' : '/api') + '/now/web/subscribe', {
  method: 'POST', credentials: 'same-origin', signal, cache: 'no-store',
  headers: { 'Content-Type': 'application/json', Accept: 'application/x-ndjson', 'X-Graphit-Request': 'ui' },
  body: stringifyNativeJson({ selection }),
 })
 if (!response.ok) throw new NowReadError(response.status, response.status === 401 ? 'Sign in to follow project activity.' : response.status === 403 ? 'Project activity is not permitted.' : `Activity request failed (HTTP ${response.status}).`)
 if (!response.body || response.headers.get('Content-Type')?.split(';')[0] !== 'application/x-ndjson') throw new NowReadError(502, 'The activity stream is unavailable.')
 const reader = response.body.getReader(), decoder = new TextDecoder('utf-8', { fatal: true })
 const abort = () => { void reader.cancel().catch(() => {}) }; signal.addEventListener('abort', abort, { once: true })
 let pending = ''
 try {
  while (!signal.aborted) {
   const { value, done } = await reader.read()
   if (signal.aborted) return
   pending += decoder.decode(value, { stream: !done })
   let end: number
   while ((end = pending.indexOf('\n')) >= 0) {
    const line = pending.slice(0, end); pending = pending.slice(end + 1)
    if (!line) continue
    const data = parseNativeJson<NowUpdate>(line)
    if (data.snapshot?.type !== 'snapshot' || data.snapshot.project_id !== data.selection?.project_id || !Array.isArray(data.snapshot.instances)) throw new NowReadError(502, 'The activity stream returned invalid data.')
    if (selection.project_id && data.selection.project_id !== selection.project_id || selection.project_version && data.selection.project_version !== selection.project_version) throw new NowReadError(502, 'The activity stream changed project scope.')
    if (data.snapshot.instances.some(instance => instance.version !== data.selection.project_version)) throw new NowReadError(502, 'The activity stream changed version.')
    if (selection.project_dir && selection.project_dir !== data.selection.project_dir) throw new NowReadError(502, 'The activity stream changed working copy.')
    update(data)
   }
   if (done) throw new NowReadError(502, 'The activity connection ended.')
  }
 } finally {
  await reader.cancel().catch(() => {})
  signal.removeEventListener('abort', abort)
  reader.releaseLock()
 }
}
