import { getApiBase } from '@/lib/utils'
import { parseNativeJson, stringifyNativeJson } from './nativeJson'
import type { TaskContexts } from './task'

export type ProjectReadModule = 'task' | 'memory' | 'knowledge' | 'ast' | 'now' | 'hub'
export type NativeRecordContexts = TaskContexts
export const nativeRecordContexts = (module: Exclude<ProjectReadModule, 'task'>, signal?: AbortSignal) =>
  nativeRecordRead<NativeRecordContexts>(module === 'now' ? '/now/web/contexts' : '/' + module + '/contexts', module === 'ast' || module === 'now' || module === 'hub' ? {} : { input: {} }, signal)

export class NativeRecordReadError extends Error {
  constructor(public readonly status: number, public readonly code: string | null = null) { super('Record request failed (HTTP ' + status + (code ? ', ' + code : '') + ')') }
}

export async function nativeRecordRead<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
  const response = await nativeRecordResponse(path, body, signal)
  if (!response.ok) {
    let code: string | null = null
    try { const error = parseNativeJson<{ error?: { code?: string } }>(await response.text()); code = error.error?.code ?? null } catch { /* Preserve the actual HTTP status when no JSON error is available. */ }
    throw new NativeRecordReadError(response.status, code)
  }
  return parseNativeJson<T>(await response.text())
}

export function nativeRecordResponse(path: string, body: unknown, signal?: AbortSignal): Promise<Response> {
  return nativeRecordPost(path, stringifyNativeJson(body), { 'Content-Type': 'application/json' }, signal)
}

export function nativeRecordMultipartResponse(path: string, body: FormData, signal?: AbortSignal): Promise<Response> {
  return nativeRecordPost(path, body, {}, signal)
}

function nativeRecordPost(path: string, body: BodyInit, headers: Record<string, string>, signal?: AbortSignal): Promise<Response> {
  const base = getApiBase().replace(/\/$/, '')
  return fetch(base + (base.endsWith('/api') ? '' : '/api') + path, {
    method: 'POST', credentials: 'same-origin', signal,
    headers: { ...headers, 'X-Graphit-Request': 'ui' }, body,
  })
}
