import { act, cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi } from '@/api/task'
import { TaskComments, TaskHistory } from './TaskAuditSections'
import { before, commentsPage, context, detail, first, historyPage } from './task.test-fixtures'
import { useTaskWorkspace } from './taskWorkspaceState'

vi.mock('@/api/task', async original => ({ ...await original<typeof import('@/api/task')>(), taskApi: { get: vi.fn() } }))
beforeEach(() => {
 vi.resetAllMocks()
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
 useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready' })
})
afterEach(() => { cleanup(); vi.unstubAllGlobals() })

describe('paged Task audit', () => {
 it('distinguishes unloaded, loading, failed and loaded-empty comments with keyboard retry', async () => {
  let reject!: (cause: Error) => void
  vi.mocked(taskApi.get).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail }))
   .mockResolvedValueOnce({ ...detail, comments: { comments: [], next_cursor: null } })
  const user = userEvent.setup()
  render(<TaskComments detail={detail} selection={context} />)
  expect(taskApi.get).not.toHaveBeenCalled()
  expect(screen.getByText('Comments have not been loaded.')).toBeTruthy()
  expect(screen.queryByText('No comments on this page.')).toBeNull()
  screen.getByRole('button', { name: 'Load comments' }).focus()
  await user.keyboard('{Enter}')
  expect(screen.getByText('Loading comments…')).toBeTruthy()
  expect(screen.queryByText('No comments on this page.')).toBeNull()
  await act(async () => reject(new Error('Comments temporarily unavailable')))
  expect(screen.getByRole('alert').textContent).toContain('Comments temporarily unavailable')
  screen.getByRole('button', { name: 'Retry comments' }).focus()
  await user.keyboard('{Enter}')
  expect(await screen.findByText('No comments on this page.')).toBeTruthy()
  for (const call of vi.mocked(taskApi.get).mock.calls) expect(call).toEqual([context, 'task1', { comments: { page_size: 20, cursor: detail.comments_start_cursor }, signal: expect.any(AbortSignal) }])
 })

 it('replaces history pages and uses the supplied predecessor across page boundaries and back navigation', async () => {
  const predecessor = { ...first, specification: { ...first.specification, revision: 3, description: 'Page boundary predecessor.' } }
  const revised = { ...first, specification: { ...first.specification, revision: 4, description: 'Fourth specification.' } }
  const secondPage = { before: predecessor, events: [{ actor: 'reviewer', observed_at: 14, kind: { revised: { reason: 'Second page revision.' } }, record: revised }], next_cursor: null }
  vi.mocked(taskApi.get).mockImplementation(async (_, __, options) => ({ ...detail, history: options?.history?.cursor === 'history-page-2' ? secondPage : { ...historyPage, next_cursor: 'history-page-2' } }))
  const user = userEvent.setup()
  render(<TaskHistory detail={detail} selection={context} />)
  expect(screen.queryByText('No specification revisions on this page.')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  await screen.findByText('rev 2 · revised')
  await user.click(screen.getByRole('button', { name: 'Next history page' }))
  await user.click(await screen.findByText('rev 4 · revised'))
  expect(screen.queryByText('rev 2 · revised')).toBeNull()
  expect(screen.getByText('Before').parentElement?.textContent).toContain('Page boundary predecessor.')
  expect(screen.queryByText('Initial specification.')).toBeNull()
  expect(screen.getByRole('button', { name: 'Next history page' })).toHaveProperty('disabled', true)
  await user.click(screen.getByRole('button', { name: 'Previous history page' }))
  expect(await screen.findByText('rev 2 · revised')).toBeTruthy()
  expect(screen.queryByText('rev 4 · revised')).toBeNull()
  expect(vi.mocked(taskApi.get).mock.calls.map(call => call[2]?.history?.cursor)).toEqual([detail.history_start_cursor, 'history-page-2', detail.history_start_cursor])
 })

 it('keeps the current comments page on next-page failure and retries its exact cursor', async () => {
  vi.mocked(taskApi.get).mockResolvedValueOnce({ ...detail, comments: { ...commentsPage, next_cursor: 'comments-next' } })
   .mockRejectedValueOnce(new Error('Next page unavailable'))
   .mockResolvedValueOnce({ ...detail, comments: { comments: [{ ...commentsPage.comments[0], id: 'comment2', sequence: 2, body: 'Second page only.' }], next_cursor: null } })
  const user = userEvent.setup()
  render(<TaskComments detail={detail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load comments' }))
  expect(await screen.findByText('native producer')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Next comments page' }))
  expect(await screen.findByText('Next page unavailable')).toBeTruthy()
  expect(screen.getByText('native producer')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry comments' }))
  expect(await screen.findByText('Second page only.')).toBeTruthy()
  expect(screen.queryByText('native producer')).toBeNull()
  expect(vi.mocked(taskApi.get).mock.calls.slice(1).map(call => call[2]?.comments?.cursor)).toEqual(['comments-next', 'comments-next'])
 })

 it('aborts a pending audit on unmount and ignores its late denial', async () => {
  let reject!: (cause: Error) => void
  vi.mocked(taskApi.get).mockImplementation(() => new Promise((_, fail) => { reject = fail }))
  const user = userEvent.setup()
  const view = render(<TaskHistory detail={detail} selection={context} />)
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  const signal = vi.mocked(taskApi.get).mock.calls[0][2]!.signal!
  view.unmount()
  expect(signal.aborted).toBe(true)
  await act(async () => reject(new Error('Late denial')))
  expect(useTaskWorkspace.getState().phase).toBe('ready')
 })

 it('shows loaded-empty history distinctly and retains creation as evidence', () => {
  render(<TaskHistory detail={{ ...detail, history: { before, events: [], next_cursor: null } }} selection={context} />)
  expect(screen.getByText('Created')).toBeTruthy()
  expect(screen.getByText('No lifecycle events on this page.')).toBeTruthy()
  expect(screen.getByText('No specification revisions on this page.')).toBeTruthy()
  expect(within(screen.getByRole('navigation', { name: 'history pages' })).getByRole('status').textContent).toBe('Page 1')
  expect(taskApi.get).not.toHaveBeenCalled()
 })
})
