import { beforeEach, describe, expect, it, vi } from 'vitest'
import { sessionApi } from '@/api/taskSession'
import { TaskReadError } from '@/api/task'
import { parseNativeJson, stringifyNativeJson } from '@/api/nativeJson'
import { sessionAuditExport } from './sessionAuditExport'
import { context, sessionDetail, sessionHistoryPage } from './task.test-fixtures'
vi.mock('@/api/taskSession', async original => ({ ...await original<typeof import('@/api/taskSession')>(), sessionApi: { get: vi.fn() } }))
beforeEach(() => vi.resetAllMocks())
describe('complete explicit Session audit export', () => {
 it('walks every pinned page to EOF and serializes native integers without keeping snapshots', async () => {
  const last = { ...sessionHistoryPage.events[0], record: { ...sessionDetail.session, revision: 18446744073709551615n } }
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { ...sessionHistoryPage, next_cursor: 'next-native-page' } }).mockResolvedValueOnce({ ...sessionDetail, history: { before: sessionDetail.session, events: [last], next_cursor: null } })
  const signal = new AbortController().signal
  const parts = await sessionAuditExport(sessionDetail, context, signal)
  const json = parts.join('')
  expect(parseNativeJson(json)).toEqual({ ...sessionDetail, history: { before: sessionHistoryPage.before, events: [...sessionHistoryPage.events, last], next_cursor: null } })
  expect(json).toContain('18446744073709551615')
  expect(vi.mocked(sessionApi.get).mock.calls).toEqual([
   [context, 'session1', { signal, history: { page_size: 20, cursor: sessionDetail.history_start_cursor } }],
   [context, 'session1', { signal, history: { page_size: 20, cursor: 'next-native-page' } }],
  ])
  expect(stringifyNativeJson(sessionDetail)).toContain('"history":null')
 })
 it('exports a completed empty page distinctly from an unloaded section', async () => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { before: sessionHistoryPage.before, events: [], next_cursor: null } })
  const exported = parseNativeJson((await sessionAuditExport(sessionDetail, context, new AbortController().signal)).join(''))
  expect(exported).toEqual({ ...sessionDetail, history: { before: sessionHistoryPage.before, events: [], next_cursor: null } })
 })
 it('rejects a denied later page instead of returning a partial download', async () => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { ...sessionHistoryPage, next_cursor: 'denied-page' } }).mockRejectedValueOnce(new TaskReadError(403))
  await expect(sessionAuditExport(sessionDetail, context, new AbortController().signal)).rejects.toMatchObject({ status: 403 })
 })
 it.each(['missing', 'unloaded'] as const)('rejects a %s explicit page', async kind => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce(kind === 'missing' ? null : sessionDetail)
  await expect(sessionAuditExport(sessionDetail, context, new AbortController().signal)).rejects.toThrow(kind === 'missing' ? 'no longer available' : 'page was not returned')
 })
 it('stops traversal on cancellation even if the pending response arrives late', async () => {
  let resolve!: (value: typeof sessionDetail) => void
  vi.mocked(sessionApi.get).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const controller = new AbortController(), pending = sessionAuditExport(sessionDetail, context, controller.signal)
  controller.abort(); resolve({ ...sessionDetail, history: sessionHistoryPage })
  await expect(pending).rejects.toMatchObject({ name: 'AbortError' })
  expect(sessionApi.get).toHaveBeenCalledTimes(1)
 })
})
