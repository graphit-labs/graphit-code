import '@testing-library/jest-dom/vitest'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { act, cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { daemonApi, type DaemonStatus } from '@/api/daemon'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { useAppStore } from '@/store/appStore'
import DaemonDashboard from './DaemonDashboard'
vi.mock('@/api/daemon', () => ({ daemonApi: { getStatus: vi.fn() } }))
vi.mock('@/components/layout/WorkspaceRefresh', () => ({ usePageRefresh: vi.fn() }))
beforeEach(() => {
  vi.resetAllMocks()
  useAppStore.setState({ apiBase: '/api', webIdentity: { state: 'authenticated', provider: 'test', subject: 'first' } })
  vi.mocked(daemonApi.getStatus).mockResolvedValue({ pid: 123, state: 'running' })
})
afterEach(() => { cleanup(); vi.useRealTimers() })
function deferred() {
  let resolve!: (value: DaemonStatus) => void
  const promise = new Promise<DaemonStatus>(done => { resolve = done })
  return { promise, resolve }
}
async function refresh() {
  await act(async () => { await vi.mocked(usePageRefresh).mock.calls.at(-1)![0]() })
}
it.each(['running', 'stopping'] as const)('renders real %s state and portable stdio configuration', async state => {
  vi.mocked(daemonApi.getStatus).mockResolvedValue({ pid: 123, state })
  render(<DaemonDashboard />)
  expect(await screen.findByText(state === 'running' ? 'Running' : 'Stopping')).toBeTruthy()
  expect(screen.getByText('123')).toBeTruthy()
  const config = JSON.parse(screen.getByText(/"mcpServers"/).textContent!)
  expect(config).toEqual({ mcpServers: { graphit: { command: 'graphit', args: ['mcp', '--stdio'] } } })
  expect(screen.queryByText(/bearer|scheduler|uptime|PID file|Recent service logs/i)).toBeNull()
  expect(screen.queryByRole('button', { name: /stop|copy/i })).toBeNull()
})
it('exposes a causal authorization failure and recovers through explicit retry', async () => {
  vi.mocked(daemonApi.getStatus).mockRejectedValueOnce(new Error('HTTP 401'))
  const user = userEvent.setup()
  render(<DaemonDashboard />)
  expect(await screen.findByRole('alert')).toHaveTextContent('HTTP 401')
  expect(screen.getByText('Daemon status unavailable')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Try again' }))
  expect(await screen.findByText('Running')).toBeTruthy()
  expect(screen.queryByRole('alert')).toBeNull()
})
it('clears a prior process result when a subsequent read is denied', async () => {
  render(<DaemonDashboard />)
  await screen.findByText('Running')
  vi.mocked(daemonApi.getStatus).mockRejectedValueOnce(new Error('HTTP 401'))
  await refresh()
  expect(screen.getByRole('alert')).toHaveTextContent('HTTP 401')
  expect(screen.queryByText('Running')).toBeNull()
  expect(screen.queryByText('123')).toBeNull()
})
it('skips polling while pending and lets refresh supersede and discard a late read', async () => {
  vi.useFakeTimers()
  const old = deferred()
  vi.mocked(daemonApi.getStatus).mockReturnValueOnce(old.promise)
  render(<DaemonDashboard />)
  await act(async () => { await vi.advanceTimersByTimeAsync(0) })
  const signal = vi.mocked(daemonApi.getStatus).mock.calls[0][0]!
  await act(async () => { await vi.advanceTimersByTimeAsync(15000) })
  expect(daemonApi.getStatus).toHaveBeenCalledTimes(1)
  vi.mocked(daemonApi.getStatus).mockResolvedValueOnce({ pid: 456, state: 'stopping' })
  await refresh()
  expect(signal.aborted).toBe(true)
  expect(screen.getByText('Stopping')).toBeTruthy()
  await act(async () => old.resolve({ pid: 999, state: 'running' }))
  expect(screen.queryByText('999')).toBeNull()
  await act(async () => { await vi.advanceTimersByTimeAsync(5000) })
  expect(daemonApi.getStatus).toHaveBeenCalledTimes(3)
})
it.each(['identity', 'API base'] as const)('aborts and hides previous private results when %s changes', async change => {
  render(<DaemonDashboard />)
  await screen.findByText('Running')
  const old = deferred(), next = deferred()
  vi.mocked(daemonApi.getStatus).mockReturnValueOnce(old.promise).mockReturnValueOnce(next.promise)
  // Start a refresh without waiting for its response.
  let refreshPromise!: void | Promise<unknown>
  act(() => { refreshPromise = vi.mocked(usePageRefresh).mock.calls.at(-1)![0]() })
  const signal = vi.mocked(daemonApi.getStatus).mock.calls[1][0]!
  act(() => useAppStore.setState(change === 'identity' ? { webIdentity: { state: 'anonymous' } } : { apiBase: '/other/api' }))
  expect(signal.aborted).toBe(true)
  expect(screen.queryByText('123')).toBeNull()
  await act(async () => { old.resolve({ pid: 999, state: 'running' }); await refreshPromise })
  expect(screen.queryByText('999')).toBeNull()
  await act(async () => next.resolve({ pid: 456, state: 'stopping' }))
  expect(await screen.findByText('456')).toBeTruthy()
})
it('aborts pending work and clears polling on unmount', async () => {
  vi.useFakeTimers()
  const old = deferred()
  vi.mocked(daemonApi.getStatus).mockReturnValueOnce(old.promise)
  const view = render(<DaemonDashboard />)
  await act(async () => { await vi.advanceTimersByTimeAsync(0) })
  const signal = vi.mocked(daemonApi.getStatus).mock.calls[0][0]!
  view.unmount()
  expect(signal.aborted).toBe(true)
  await act(async () => { old.resolve({ pid: 999, state: 'running' }); await vi.advanceTimersByTimeAsync(10000) })
  expect(daemonApi.getStatus).toHaveBeenCalledTimes(1)
})
