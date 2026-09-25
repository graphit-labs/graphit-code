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

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const base = getApiBase()

  const fullBase = base.endsWith('/api') ? base : `${base}/api`
  const cleanPath = path.startsWith('/api/') ? path.slice(4) : path

  const url = `${fullBase}${cleanPath}`

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
  return fetch(`${fullBase}${cleanPath}`, {
    method: 'POST', signal, credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream', 'X-Graphit-Request': 'ui' },
    body: JSON.stringify(body),
  });
}
