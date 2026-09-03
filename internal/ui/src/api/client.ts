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

  const token = getCookie('graphit_id_token')
  if (token && window.__WEB_MODE__) {
    options.headers = {
      ...(options.headers ?? {}),
      Authorization: `Bearer ${token}`,
    }
  }

  useAppStore.getState().incrementLoading()
  try {
    const res = await fetch(url, { ...options })
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

function getCookie(name: string): string | null {
  const match = document.cookie.match(new RegExp('(^| )' + name + '=([^;]+)'))
  return match ? match[2] : null
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
}

export type { ApiError }
