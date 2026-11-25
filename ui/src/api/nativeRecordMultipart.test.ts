import { afterEach, expect, it, vi } from 'vitest'
import { nativeRecordMultipartResponse, nativeRecordResponse } from './nativeRecordRead'

const fixture = vi.hoisted(() => ({ base: '' }))
vi.mock('@/lib/utils', () => ({ getApiBase: () => fixture.base }))
afterEach(() => { vi.unstubAllGlobals(); fixture.base = '' })

// These are opaque client-transport bytes, not a qualified server ZIP/package.
function multipart() {
  const file = new File([new Uint8Array([0, 255, 1, 128])], 'opaque.zip', { type: 'application/zip' })
  const buffer = vi.fn(() => { throw new Error('Client must not buffer the file') })
  Object.defineProperty(file, 'arrayBuffer', { value: buffer })
  const body = new FormData()
  body.append('metadata', JSON.stringify({ fixture: 'opaque-client-body' }))
  body.append('file', file)
  return { body, file, buffer }
}

it.each([
  ['', '/api/hub/upload'],
  ['/api', '/api/hub/upload'],
  ['/proxy', '/proxy/api/hub/upload'],
  ['/proxy/api/', '/proxy/api/hub/upload'],
])('multipart native transport uses the current base %s and preserves the browser body and boundary', async (base, expected) => {
  fixture.base = base
  const { body, file, buffer } = multipart()
  const signal = new AbortController().signal
  const response = new Response('actual body', { status: 201, headers: { 'X-Outcome': 'retained' } })
  const fetcher = vi.fn(async () => response); vi.stubGlobal('fetch', fetcher)
  await expect(nativeRecordMultipartResponse('/hub/upload', body, signal)).resolves.toBe(response)
  expect(fetcher).toHaveBeenCalledOnce()
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toBe(expected)
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal })
  expect(init.body).toBe(body)
  const headers = new Headers(init.headers)
  expect(headers.get('X-Graphit-Request')).toBe('ui')
  expect(headers.has('Content-Type')).toBe(false)
  expect(headers.has('Authorization')).toBe(false)
  expect([...body.keys()]).toEqual(['metadata', 'file'])
  expect(body.get('file')).toBe(file)
  expect(file.size).toBe(4)
  expect(buffer).not.toHaveBeenCalled()
})

it('multipart transport retains actual 413 status, headers and response stream for the caller decoder', async () => {
  const { body } = multipart()
  const response = new Response('bounded failure', { status: 413, headers: { 'X-Graphit-Inspection-Required': 'request-selection' } })
  vi.stubGlobal('fetch', vi.fn(async () => response))
  const actual = await nativeRecordMultipartResponse('/hub/upload', body)
  expect(actual).toBe(response)
  expect(actual.bodyUsed).toBe(false)
  expect(actual.headers.get('X-Graphit-Inspection-Required')).toBe('request-selection')
  expect(await actual.text()).toBe('bounded failure')
})

it.each([new TypeError('Network unavailable'), new DOMException('Request aborted', 'AbortError')])('multipart transport preserves causal fetch failures without retry or fallback: %s', async error => {
  const { body } = multipart(), controller = new AbortController()
  if (error.name === 'AbortError') controller.abort()
  const fetcher = vi.fn().mockRejectedValue(error); vi.stubGlobal('fetch', fetcher)
  await expect(nativeRecordMultipartResponse('/hub/upload', body, controller.signal)).rejects.toBe(error)
  expect(fetcher).toHaveBeenCalledOnce()
  expect(fetcher.mock.calls[0][1].signal).toBe(controller.signal)
})

it('shared native POST retains exact JSON NativeInteger serialization, content type and original response', async () => {
  fixture.base = '/proxy/api/'
  const signal = new AbortController().signal, response = new Response('{}')
  const fetcher = vi.fn(async () => response); vi.stubGlobal('fetch', fetcher)
  await expect(nativeRecordResponse('/hub/contexts', { count: 9007199254740993n }, signal)).resolves.toBe(response)
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toBe('/proxy/api/hub/contexts')
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin', signal, body: '{"count":9007199254740993}' })
  expect(new Headers(init.headers).get('Content-Type')).toBe('application/json')
  expect(new Headers(init.headers).get('X-Graphit-Request')).toBe('ui')
})
