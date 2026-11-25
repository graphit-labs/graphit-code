import { afterEach, expect, it, vi } from 'vitest'
import { installedLibraries } from './installedLibraries'
import { NativeRecordReadError } from './nativeRecordRead'
import { useAppStore } from '@/store/appStore'

afterEach(() => vi.unstubAllGlobals())
it('posts the explicit global scope without borrowing the selected project or Native binding', async () => {
  useAppStore.setState({ activeProjectDir: '/private/project', activeProjectId: 'project-id', activeProjectVersion: 'feature-branch', activeAgent: 'codex' })
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ scope: 'global_installation', entries: [], next_offset: null })))
  vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  await installedLibraries({ offset: 0, limit: 30, artifact_type: 'knowledge', version: 'release-1' }, controller.signal)
  const [url, init] = fetcher.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toMatch(/\/api\/hub\/installed\/native$/)
  expect(init.credentials).toBe('same-origin'); expect(init.signal).toBe(controller.signal)
  expect(JSON.parse(init.body as string)).toEqual({ scope: 'global_installation', offset: 0, limit: 30, artifact_type: 'knowledge', version: 'release-1' })
  expect(JSON.stringify(init)).not.toContain('/private/project')
})
it('keeps forbidden and unavailable responses as errors rather than empty inventory', async () => {
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ error: { code: 'permission_denied' } }), { status: 403 }))
  vi.stubGlobal('fetch', fetcher)
  await expect(installedLibraries({ offset: 0, limit: 30 })).rejects.toMatchObject({ status: 403, code: 'permission_denied' })
  fetcher.mockImplementation(async () => new Response(JSON.stringify({ error: { code: 'hub_inventory_unavailable' } }), { status: 500 }))
  await expect(installedLibraries({ offset: 0, limit: 30 })).rejects.toBeInstanceOf(NativeRecordReadError)
})
it('refuses a mismatched scope or nonadvancing pagination cursor', async () => {
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ scope: 'project', entries: [], next_offset: null })))
  vi.stubGlobal('fetch', fetcher)
  await expect(installedLibraries({ offset: 0, limit: 30 })).rejects.toThrow('Invalid installed library response')
  fetcher.mockImplementation(async () => new Response(JSON.stringify({ scope: 'global_installation', entries: [], next_offset: 30 })))
  await expect(installedLibraries({ offset: 30, limit: 30 })).rejects.toThrow('Invalid installed library response')
})
