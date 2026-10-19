import { beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi, TaskReadError } from '@/api/task'
import { parseNativeJson, stringifyNativeJson } from '@/api/nativeJson'
import { taskAuditExport } from './taskAuditExport'
import { before, commentsPage, context, detail, first, historyPage } from './task.test-fixtures'

vi.mock('@/api/task', async original => ({ ...await original<typeof import('@/api/task')>(), taskApi: { get: vi.fn() } }))
beforeEach(() => vi.clearAllMocks())

describe('explicit Task audit export', () => {
 it('walks both pinned section cursors and serializes all pages with exact native integers', async () => {
  const last = { ...first, specification: { ...first.specification, revision: 18446744073709551615n } }
  const lastEvent = { ...historyPage.events[0], record: last }
  const comment = { ...commentsPage.comments[0], id: 'comment2', sequence: 9007199254740993n }
  vi.mocked(taskApi.get).mockImplementation(async (_, __, options) => ({
   ...detail,
   history: options?.history ? options.history.cursor === detail.history_start_cursor
    ? { ...historyPage, next_cursor: 'history-more' }
    : { before: first, events: [lastEvent], next_cursor: null } : null,
   comments: options?.comments ? options.comments.cursor === detail.comments_start_cursor
    ? { ...commentsPage, next_cursor: 'comments-more' }
    : { comments: [comment], next_cursor: null } : null,
  }))
  const signal = new AbortController().signal
  const parts = await taskAuditExport(detail, context, signal)
  const json = parts.join('')
  const exported = parseNativeJson(json)
  expect(exported).toEqual({ ...detail, history: { before, events: [...historyPage.events, lastEvent], next_cursor: null }, comments: { comments: [...commentsPage.comments, comment], next_cursor: null } })
  expect(json).toContain('18446744073709551615')
  expect(json).toContain('9007199254740993')
  expect(vi.mocked(taskApi.get).mock.calls).toEqual([
   [context, 'task1', { signal, history: { page_size: 20, cursor: detail.history_start_cursor } }],
   [context, 'task1', { signal, history: { page_size: 20, cursor: 'history-more' } }],
   [context, 'task1', { signal, comments: { page_size: 20, cursor: detail.comments_start_cursor } }],
   [context, 'task1', { signal, comments: { page_size: 20, cursor: 'comments-more' } }],
  ])
  expect(stringifyNativeJson(detail)).toContain('"history":null')
 })

 it('propagates current authorization denial instead of returning a partial export', async () => {
  vi.mocked(taskApi.get).mockResolvedValueOnce({ ...detail, history: historyPage }).mockRejectedValueOnce(new TaskReadError(403))
  await expect(taskAuditExport(detail, context, new AbortController().signal)).rejects.toMatchObject({ status: 403 })
  expect(taskApi.get).toHaveBeenCalledTimes(2)
 })

 it('stops after cancellation even when an in-flight transport resolves late', async () => {
  let resolve!: (value: typeof detail) => void
  vi.mocked(taskApi.get).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const controller = new AbortController()
  const exported = taskAuditExport(detail, context, controller.signal)
  controller.abort()
  resolve({ ...detail, history: historyPage })
  await expect(exported).rejects.toMatchObject({ name: 'AbortError' })
  expect(taskApi.get).toHaveBeenCalledTimes(1)
 })
})
