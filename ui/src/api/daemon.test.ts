import { beforeEach, expect, it, vi } from 'vitest'
import { api } from './client'
import { daemonApi } from './daemon'
vi.mock('./client', () => ({ api: { get: vi.fn() } }))
beforeEach(() => vi.resetAllMocks())
it.each(['running', 'stopping'] as const)('reads the authenticated status contract with cancellation and no cache: %s', async state => {
  const controller = new AbortController()
  vi.mocked(api.get).mockResolvedValue({ pid: 4294967295, state })
  expect(await daemonApi.getStatus(controller.signal)).toEqual({ pid: 4294967295, state })
  expect(api.get).toHaveBeenCalledWith('/daemon/status', { signal: controller.signal, cache: 'no-store' })
})
it.each([null, {}, { pid: null, state: 'running' }, { pid: -1, state: 'running' }, { pid: 4294967296, state: 'running' }, { pid: 1.5, state: 'running' }, { pid: '123', state: 'running' }, { pid: 123, state: 'stopped' }, { pid: 123, running: true }])('rejects an invalid status without interpreting legacy fields: %j', async value => {
  vi.mocked(api.get).mockResolvedValue(value)
  await expect(daemonApi.getStatus()).rejects.toThrow('Invalid daemon status response.')
})
it('preserves transport and authorization failures', async () => {
  const denied = Object.assign(new Error('HTTP 401'), { status: 401 })
  vi.mocked(api.get).mockRejectedValue(denied)
  await expect(daemonApi.getStatus()).rejects.toBe(denied)
})
