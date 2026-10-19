import '@/test/contextControls'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi, type TaskCataloguePage } from '@/api/task'
import { useTaskWorkspace } from './taskWorkspace'
import { WorkspaceRefreshProvider } from '@/components/layout/WorkspaceRefreshProvider'
import { TaskWorkspaceSelectors } from './TaskWorkspace'
import TaskGanttPage from './TaskGanttPage'
import { context, cursor, first, second, summary } from './task.test-fixtures'
vi.mock('@/api/task', async importOriginal => ({ ...await importOriginal<typeof import('@/api/task')>(), taskApi: { contexts: vi.fn(), context: vi.fn(), catalogue: vi.fn(), get: vi.fn() } }))
beforeEach(() => {
 vi.clearAllMocks(); useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready', error: '' })
 vi.mocked(taskApi.context).mockImplementation(async selection => ({ ...context, ...selection }))
 vi.mocked(taskApi.catalogue).mockImplementation(async options => options.cursor ? { results: [summary(second)], next_cursor: null } : { results: [summary(first)], next_cursor: cursor })
})
function Location() { return <output>{useLocation().pathname}</output> }
function page() { return render(<MemoryRouter><Routes><Route path="/" element={<TaskGanttPage />} /><Route path="/task/explorer/:id" element={<Location />} /></Routes></MemoryRouter>) }
describe('native Task Gantt', () => {
 it('awaits the current calendar through the single keyboard-operated header refresh', async () => {
  let resolve!: (value: TaskCataloguePage) => void
  const pending = new Promise<TaskCataloguePage>(done => { resolve = done })
  const cloned = { ...context, project_dir: context.project_dir }
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [cloned], current: cloned })
  vi.mocked(taskApi.catalogue).mockResolvedValueOnce({ results: [summary(first)], next_cursor: null })
  vi.mocked(taskApi.catalogue).mockImplementation(({ signal }) => new Promise((done, fail) => {
   void pending.then(done)
   signal?.addEventListener('abort', () => fail(new DOMException('Aborted', 'AbortError')), { once: true })
  }))
  const user = userEvent.setup()
  render(<MemoryRouter initialEntries={['/task/gantt']}><WorkspaceRefreshProvider>
   <TaskWorkspaceSelectors /><TaskGanttPage />
  </WorkspaceRefreshProvider></MemoryRouter>)
  await screen.findByText('3 days inclusive')
  const buttons = screen.getAllByRole('button', { name: /^Refresh$/ })
  expect(buttons).toHaveLength(1)
  const refresh = buttons[0] as HTMLButtonElement
  refresh.focus(); await user.keyboard('{Enter}')
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenCalledTimes(2))
  const signal = vi.mocked(taskApi.catalogue).mock.calls[1][0].signal!
  await act(async () => { await new Promise(done => window.setTimeout(done, 260)) })
  expect(taskApi.catalogue).toHaveBeenCalledTimes(2)
  expect(signal.aborted).toBe(false)
  expect(refresh.disabled).toBe(true)
  expect(refresh.getAttribute('aria-busy')).toBe('true')
  expect(screen.queryByText('3 days inclusive')).toBeNull()
  await act(async () => resolve({ results: [{ ...summary(first), schedule: { start_date: '2028-03-02', end_date: '2028-03-02' } }], next_cursor: null }))
  expect(await screen.findByText('1 day inclusive')).toBeTruthy()
  await waitFor(() => expect(refresh.disabled).toBe(false))
  expect(refresh.getAttribute('aria-busy')).toBe('false')
  expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ selection: context }))
  expect(taskApi.get).not.toHaveBeenCalled()
 })
 it('shows lightweight actual dates, pages undated dependencies and opens task without N+1 audits', async () => {
  const user = userEvent.setup(); page()
  expect(await screen.findByText('3 days inclusive')).toBeTruthy()
  expect(taskApi.get).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: 'Load more' }))
  expect(await screen.findByRole('region', { name: 'Without dates' })).toBeTruthy()
  expect(screen.getByText('2 tasks in loaded pages')).toBeTruthy()
  screen.getByRole('button', { name: 'Second task' }).focus(); await user.keyboard('{Enter}')
  expect(screen.getByText('/task/explorer/task2')).toBeTruthy()
 })
 it('renders malformed calendar data as errors and preserves valid rows', async () => {
  vi.mocked(taskApi.catalogue).mockResolvedValue({ results: [summary(first), { ...summary(second), schedule: { start_date: '2023-02-29' } }], next_cursor: null })
  page()
  expect(await screen.findByRole('alert')).toBeTruthy()
  expect(screen.getByRole('region', { name: 'Invalid task dates' })).toBeTruthy()
  expect(screen.getByText('3 days inclusive')).toBeTruthy()
 })
 it('keeps denial explicit, retries, and cancels previous version requests', async () => {
  vi.mocked(taskApi.context).mockRejectedValueOnce(new Error('HTTP 403'))
  const user = userEvent.setup(); page()
  expect(await screen.findByText('HTTP 403')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry' }))
  await screen.findByText('3 days inclusive')
  const signal = vi.mocked(taskApi.context).mock.calls[vi.mocked(taskApi.context).mock.calls.length - 1]?.[1]
  act(() => useTaskWorkspace.setState({ selection: { ...context, project_version: 'feature' } }))
  await waitFor(() => expect(taskApi.context).toHaveBeenLastCalledWith({ ...context, project_version: 'feature' }, expect.any(AbortSignal)))
  expect(signal?.aborted).toBe(true)
 })
 it('requests global blocker/flag conjunctions and resets paging instead of filtering loaded rows', async () => {
  vi.mocked(taskApi.catalogue).mockImplementation(async options => {
   if (options.filter?.blocked === true) return { results: [summary(second)], next_cursor: null }
   return options.cursor ? { results: [summary(second)], next_cursor: null } : { results: [summary(first)], next_cursor: cursor }
  })
  const user = userEvent.setup(); page()
  await screen.findByText('3 days inclusive')
  const initialSignal = vi.mocked(taskApi.catalogue).mock.calls.at(-1)?.[0].signal
  await user.click(screen.getByRole('combobox', { name: 'Filter task blockers' }))
  await user.click(screen.getByRole('option', { name: 'Blocked open tasks' }))
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ filter: { blocked: true } })))
  expect(await screen.findByRole('button', { name: 'Second task' })).toBeTruthy()
  expect(screen.queryByText('3 days inclusive')).toBeNull()
  expect(initialSignal?.aborted).toBe(true)
  expect(vi.mocked(taskApi.catalogue).mock.calls.at(-1)?.[0].cursor).toBeUndefined()
  await user.click(screen.getByRole('combobox', { name: 'Filter task flags' }))
  await user.click(screen.getByRole('option', { name: 'Not flagged' }))
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ filter: { blocked: true, flagged: false } })))
  await user.click(screen.getByRole('combobox', { name: 'Filter task blockers' }))
  await user.click(screen.getByRole('option', { name: 'Not blocked' }))
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ filter: { blocked: false, flagged: false } })))
  await screen.findByText('3 days inclusive')
  await user.click(screen.getByRole('button', { name: 'Load more' }))
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ cursor, filter: { blocked: false, flagged: false } })))
 })

})
