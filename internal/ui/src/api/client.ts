import { getApiBase } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * Adds the selected project version to a request that names the active project.
 *
 * Every store behind the API is kept per project version, so a request about the active
 * project must say which version it is about. The project is named by `project_dir` or
 * `project_id` in the query or in a JSON body; a body-only scope is copied to the query,
 * where the server selects the version before the handler runs. A request about another
 * project — a cluster neighbour, a Hub artifact — is left untouched.
 */
export function scopeToProjectVersion(url: string, body?: unknown): string {
  const { activeProjectVersion, activeProjectDir, activeProjectId, activeProjectOrigin } = useAppStore.getState()
  if (!activeProjectVersion) return url
  const queryAt = url.indexOf('?')
  const path = queryAt < 0 ? url : url.slice(0, queryAt)
  const params = new URLSearchParams(queryAt < 0 ? '' : url.slice(queryAt + 1))
  if (params.has('project_version')) return url
  let fields: Record<string, unknown> = {}
  if (typeof body === 'string') {
    try { fields = JSON.parse(body) ?? {} } catch { fields = {} }
  } else if (typeof FormData !== 'undefined' && body instanceof FormData) {
    fields = { project_dir: body.get('project_dir'), project_id: body.get('project_id') }
  } else if (body && typeof body === 'object') {
    fields = body as Record<string, unknown>
  }
  const named = (key: 'project_dir' | 'project_id') => {
    const fromQuery = params.get(key)
    if (fromQuery) return fromQuery
    const fromBody = fields[key]
    if (typeof fromBody === 'string' && fromBody) {
      params.set(key, fromBody)
      return fromBody
    }
    return ''
  }
  const remote = activeProjectOrigin === 'hub'
  const dir = named('project_dir')
  const id = named('project_id')
  const active = remote ? Boolean(id) && id === activeProjectId && !dir : Boolean(dir) && dir === activeProjectDir
  if (!active) return url
  params.set('project_version', activeProjectVersion)
  return `${path}?${params}`
}

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const base = getApiBase()

  const fullBase = base.endsWith('/api') ? base : `${base}/api`
  const cleanPath = path.startsWith('/api/') ? path.slice(4) : path

  const url = scopeToProjectVersion(`${fullBase}${cleanPath}`, options.body)

  if (options.method && options.method !== 'GET') {
    options.headers = { ...(options.headers ?? {}), 'X-Graphit-Request': 'ui' }
  }

  useAppStore.getState().incrementLoading()
  try {
    const res = await fetch(url, { credentials: 'same-origin', ...options })
    if (!res.ok) {
      let msg = `HTTP ${res.status}`
      try { msg = (await res.json()).error ?? msg } catch { /* ignored */ }
      throw new ApiError(res.status, msg)
    }
    return await res.json() as T
  } finally {
    useAppStore.getState().decrementLoading()
  }
}

export const api = {
  get: <T>(path: string, options?: RequestInit) => request<T>(path, options),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  patch: <T>(path: string, body: unknown) =>
    request<T>(path, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
}

export type { ApiError }

export function openAPIStream(path: string, body: unknown, signal?: AbortSignal): Promise<Response> {
  const base = getApiBase();
  const fullBase = base.endsWith('/api') ? base : `${base}/api`;
  const cleanPath = path.startsWith('/api/') ? path.slice(4) : path;
  return fetch(scopeToProjectVersion(`${fullBase}${cleanPath}`, body), {
    method: 'POST', signal, credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream', 'X-Graphit-Request': 'ui' },
    body: JSON.stringify(body),
  });
}
