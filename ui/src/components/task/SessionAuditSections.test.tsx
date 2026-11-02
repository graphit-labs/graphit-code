import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { sessionApi } from '@/api/taskSession'
import { SessionAuditSections } from './SessionAuditSections'
import { context, sessionDetail, sessionHistoryPage } from './task.test-fixtures'
import { useTaskWorkspace } from './taskWorkspaceState'
vi.mock('@/api/taskSession', async original => ({ ...await original<typeof import('@/api/taskSession')>(), sessionApi: { get: vi.fn() } }))
beforeEach(() => {
 vi.resetAllMocks()
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
 useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready' })
})
afterEach(() => { cleanup(); vi.unstubAllGlobals() })
describe('explicit Session history pages', () => {
 it('keeps unloaded distinct from empty, shares one page across sections and supports keyboard loading', async () => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { before: sessionHistoryPage.before, events: [], next_cursor: null } })
  render(<SessionAuditSections detail={sessionDetail} selection={context} />)
  expect(sessionApi.get).not.toHaveBeenCalled()
  expect(screen.getByText('Session history has not been loaded.')).toBeTruthy()
  expect(screen.queryByText('No checkpoints on this page.')).toBeNull()
  screen.getByRole('button', { name: 'Load history' }).focus(); await userEvent.setup().keyboard('{Enter}')
  expect(await screen.findByText('No lifecycle events on this page.')).toBeTruthy()
  expect(screen.getByText('No checkpoints on this page.')).toBeTruthy()
  expect(screen.getByText('No specification revisions on this page.')).toBeTruthy()
  expect(screen.getByText('Created')).toBeTruthy()
  expect(sessionApi.get).toHaveBeenCalledTimes(1)
  expect(sessionApi.get).toHaveBeenCalledWith(context, 'session1', { signal: expect.any(AbortSignal), history: { page_size: 20, cursor: sessionDetail.history_start_cursor } })
 })
 it('replaces the page, uses the native predecessor for its first revision and rereads back cursors', async () => {
  const predecessor = { ...sessionDetail.session, revision: 5, input: { ...sessionDetail.session.input, description: 'Boundary request.' } }
  const revised = { ...predecessor, revision: 6, input: { ...predecessor.input, description: 'New request only.' } }
  const secondPage = { before: predecessor, events: [{ actor: 'reviewer', kind: { revised: { reason: 'Page boundary revision.' } }, observed_at: 14, record: revised }], next_cursor: null }
  vi.mocked(sessionApi.get).mockImplementation(async (_, __, options) => ({ ...sessionDetail, history: options?.history?.cursor === 'history-next' ? secondPage : { ...sessionHistoryPage, next_cursor: 'history-next' } }))
  const user = userEvent.setup(); render(<SessionAuditSections detail={sessionDetail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load history' })); await screen.findByText('rev 4 · revised')
  await user.click(screen.getByRole('button', { name: 'Next history page' })); await user.click(await screen.findByText('rev 6 · revised'))
  expect(screen.getByText('Before').parentElement?.textContent).toContain('Boundary request.')
  expect(screen.queryByText('Initial request.')).toBeNull()
  expect(screen.queryByText('rev 4 · revised')).toBeNull()
  expect(screen.getByRole('button', { name: 'Next history page' })).toHaveProperty('disabled', true)
  await user.click(screen.getByRole('button', { name: 'Previous history page' })); await screen.findByText('rev 4 · revised')
  expect(screen.queryByText('rev 6 · revised')).toBeNull()
  expect(vi.mocked(sessionApi.get).mock.calls.map(call => call[2]?.history?.cursor)).toEqual([sessionDetail.history_start_cursor, 'history-next', sessionDetail.history_start_cursor])
 })
 it('retains the visible page after failure and retries the exact failed cursor', async () => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { ...sessionHistoryPage, next_cursor: 'failed-cursor' } }).mockRejectedValueOnce(new Error('History unavailable')).mockResolvedValueOnce({ ...sessionDetail, history: { before: sessionDetail.session, events: [], next_cursor: null } })
  const user = userEvent.setup(); render(<SessionAuditSections detail={sessionDetail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load history' })); await screen.findByText('rev 4 · revised')
  await user.click(screen.getByRole('button', { name: 'Next history page' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('History unavailable')
  expect(screen.getByText('rev 4 · revised')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry history' })); await screen.findByText('No lifecycle events on this page.')
  expect(vi.mocked(sessionApi.get).mock.calls.slice(1).map(call => call[2]?.history?.cursor)).toEqual(['failed-cursor', 'failed-cursor'])
 })
 it.each(['missing', 'unloaded'] as const)('exposes a causal error when an explicit page returns %s', async kind => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce(kind === 'missing' ? null : sessionDetail)
  const user = userEvent.setup(); render(<SessionAuditSections detail={sessionDetail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  expect(await screen.findByRole('alert')).toHaveTextContent(kind === 'missing' ? 'no longer available' : 'page was not returned')
  expect(screen.queryByText('No checkpoints on this page.')).toBeNull()
 })
 it.each(['cancel', 'unmount'] as const)('aborts history on %s and discards a late response', async stop => {
  let resolve!: (value: typeof sessionDetail) => void
  vi.mocked(sessionApi.get).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const user = userEvent.setup(); const view = render(<SessionAuditSections detail={sessionDetail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  const signal = vi.mocked(sessionApi.get).mock.calls[0][2]!.signal!
  if (stop === 'cancel') await user.click(screen.getByRole('button', { name: 'Cancel history' })); else view.unmount()
  expect(signal.aborted).toBe(true)
  await act(async () => resolve({ ...sessionDetail, history: sessionHistoryPage }))
  expect(screen.queryByText('rev 4 · revised')).toBeNull()
  if (stop === 'cancel') expect(screen.getByText('History read stopped.')).toBeTruthy()
 })
})
