import '@/test/contextControls'
import { readFile, writeFile } from 'node:fs/promises'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { afterEach, expect, it, vi } from 'vitest'
import type { TaskContext, TaskContexts, TaskDetail, TaskSelection } from '@/api/task'
import type { SessionDetail } from '@/api/taskSession'
import TaskGanttPage from './TaskGanttPage'
import TaskExplorerPage from './TaskExplorerPage'
import SessionExplorerPage from './SessionExplorerPage'
import { TaskWorkspaceBootstrap } from './TaskWorkspace'
import { stopTaskWorkspace, useTaskWorkspace } from './taskWorkspaceState'
const fixture = vi.hoisted(() => ({ enabled: process.env.GRAPHIT_QUALIFY_TASK_BROWSER === '1', origin: process.env.GRAPHIT_TASK_BROWSER_ORIGIN, cookie: process.env.GRAPHIT_TASK_BROWSER_COOKIE, project: process.env.GRAPHIT_TASK_BROWSER_PROJECT_ID }))
vi.mock('@/lib/utils', async original => ({ ...await original<typeof import('@/lib/utils')>(), getApiBase: () => fixture.origin ?? '' }))
vi.mock('@/components/shared/RecordReferences', () => ({ RecordReferences: () => <span>Record links are qualified separately</span> }))
afterEach(() => { cleanup(); stopTaskWorkspace(); vi.restoreAllMocks(); vi.unstubAllGlobals() })
it.skipIf(!fixture.enabled)('reads live Rust cookie/PROJECT sockets into Gantt, native Task detail and Session', async () => {
 if (!fixture.origin || !fixture.cookie || !fixture.project) {
  throw new Error('Task Browser qualification requires GRAPHIT_TASK_BROWSER_ORIGIN, GRAPHIT_TASK_BROWSER_COOKIE and GRAPHIT_TASK_BROWSER_PROJECT_ID')
 }
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
 let actualTask: TaskDetail | undefined
 let actualSession: SessionDetail | undefined
 let actualContexts: TaskContexts | undefined
 const actualDetails = new Map<string, TaskDetail>()
 const reads: { operation: string; selection: TaskSelection; signal?: AbortSignal | null }[] = []
 const nativeFetch = globalThis.fetch
 vi.stubGlobal('fetch', async (input: RequestInfo | URL, init?: RequestInit) => {
  const headers = new Headers(init?.headers); headers.set('Origin', fixture.origin!); headers.set('Cookie', fixture.cookie!)
  const response = await nativeFetch(input, { ...init, headers })
  const operation = new URL(String(input)).pathname.split('/').at(-1)!
  reads.push({ operation, selection: (JSON.parse(String(init?.body)) as { selection: TaskSelection }).selection, signal: init?.signal })
  if (response.ok && operation === 'contexts') actualContexts = await response.clone().json() as TaskContexts
  if (response.ok && operation === 'get') {
   const record = await response.clone().json() as TaskDetail
   if (record && !record.history && !record.comments) { actualTask = record; actualDetails.set(record.task.specification.id, record) }
  }
  if (response.ok && String(input).endsWith('/api/tasks/session')) actualSession = await response.clone().json() as SessionDetail
  return response
 })
 stopTaskWorkspace()
 const user = userEvent.setup()
 const page = render(<MemoryRouter initialEntries={['/task/gantt']}><TaskWorkspaceBootstrap><Routes><Route path="/task/gantt" element={<TaskGanttPage />} /><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /></Routes></TaskWorkspaceBootstrap></MemoryRouter>)
 expect(await screen.findByText('3 days inclusive')).toBeTruthy()
 expect(actualContexts).toBeDefined()
 expect(actualContexts!.current?.project_id).toBe(fixture.project)
 expect(useTaskWorkspace.getState().selection).toEqual(actualContexts!.current)
 expect(reads[0].operation).toBe('contexts')
 expect(reads.some(read => read.operation === 'context' && JSON.stringify(read.selection) === JSON.stringify(actualContexts!.current))).toBe(true)
 expect(screen.getByRole('region', { name: 'Without dates' }).textContent).toContain('Browser dependent')
 expect(screen.getAllByText('2028-03-02').length).toBeGreaterThan(0)
 if (process.env.GRAPHIT_TASK_BROWSER_VISUAL_HTML) {
  const built = await readFile('dist/index.html', 'utf8')
  const styles = [...new DOMParser().parseFromString(built, 'text/html').querySelectorAll('style')].map(style => style.textContent).join('\n')
  await writeFile(process.env.GRAPHIT_TASK_BROWSER_VISUAL_HTML, '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>' + styles + '</style></head><body>' + document.body.innerHTML + '</body></html>')
 }


 await user.click(screen.getByRole('combobox', { name: 'Filter task blockers' }))
 await user.click(screen.getByRole('option', { name: 'Blocked open tasks' }))
 expect(await screen.findByRole('button', { name: 'Browser dependent' })).toBeTruthy()
 expect(screen.queryByText('3 days inclusive')).toBeNull()
 await user.click(screen.getByRole('combobox', { name: 'Filter task flags' }))
 await user.click(screen.getByRole('option', { name: 'Flagged' }))
 expect(await screen.findByText('No tasks match this view.')).toBeTruthy()
 await user.click(screen.getByRole('combobox', { name: 'Filter task blockers' }))
 await user.click(screen.getByRole('option', { name: 'Not blocked' }))
 expect(await screen.findByText('3 days inclusive')).toBeTruthy()
 expect(screen.queryByRole('button', { name: 'Browser dependent' })).toBeNull()
 await user.click(screen.getByRole('combobox', { name: 'Filter task flags' }))
 await user.click(screen.getByRole('option', { name: 'Not flagged' }))
 expect(await screen.findByText('No tasks match this view.')).toBeTruthy()
 await user.click(screen.getByRole('combobox', { name: 'Filter task flags' }))
 await user.click(screen.getByRole('option', { name: 'All flags' }))
 await user.click(screen.getByRole('combobox', { name: 'Filter task blockers' }))
 await user.click(screen.getByRole('option', { name: 'All dependency states' }))
 await screen.findByText('3 days inclusive')

 await user.click(screen.getByRole('button', { name: 'Browser calendar' }))
 expect(await screen.findByRole('heading', { name: 'Specification' })).toBeTruthy()
 expect(screen.getByText('Persisted scheduled task')).toBeTruthy()
 expect(screen.queryByText('native calendar')).toBeNull()
 await user.click(screen.getByRole('button', { name: 'Load comments' }))
 expect(await screen.findByText('native calendar')).toBeTruthy()
 expect(actualTask).toBeDefined()
 expect(screen.getAllByText(new Date(Number(actualTask!.task.created_at)).toLocaleString()).length).toBeGreaterThan(0)
 expect(actualTask!.task.ownership.lease_expires_at).not.toBeNull()
 expect(screen.getAllByText(new Date(Number(actualTask!.task.ownership.lease_expires_at!)).toLocaleString()).length).toBeGreaterThan(0)
 expect(screen.getAllByText(/3 calendar days/).length).toBeGreaterThan(0)
 const scheduled = actualTask!
 let downloaded: Blob | undefined
 const createObjectURL = vi.fn((blob: Blob) => { downloaded = blob; return 'blob:native-tasks' })
 const revokeObjectURL = vi.fn()
 vi.stubGlobal('URL', Object.assign(class extends URL {}, { createObjectURL, revokeObjectURL }))
 let filename: string | undefined
 const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { filename = this.download })
 await user.click(screen.getByRole('button', { name: 'Export all tasks' }))
 await waitFor(() => expect(createObjectURL).toHaveBeenCalledOnce())
 expect(downloaded!.type).toBe('application/json')
 const json = await new Promise<string>((resolve, reject) => {
  const reader = new FileReader()
  reader.onload = () => resolve(String(reader.result))
  reader.onerror = () => reject(reader.error)
  reader.readAsText(downloaded!)
 })
 const exported = JSON.parse(json) as { selection: Pick<TaskContext, 'project_id' | 'project_version'>; tasks: Omit<TaskDetail, 'history_start_cursor' | 'comments_start_cursor'>[] }
 expect(exported.selection).toEqual({ project_id: actualContexts!.current!.project_id, project_version: actualContexts!.current!.project_version })
 expect(exported.tasks).toHaveLength(2)
 for (const record of exported.tasks) {
  const current = actualDetails.get(record.task.specification.id)!
  expect(record.task).toEqual(current.task)
  expect(record.generation).toEqual(current.generation)
  expect(record).not.toHaveProperty('history_start_cursor')
  expect(record).not.toHaveProperty('comments_start_cursor')
  expect(record.creation).toEqual(current.creation)
  expect(record.history?.next_cursor).toBeNull()
  expect(record.comments?.next_cursor).toBeNull()
 }
 const calendar = exported.tasks.find(record => record.task.specification.id === scheduled.task.specification.id)!
 const dependent = exported.tasks.find(record => record.task.specification.title === 'Browser dependent')!
 expect(calendar.task).toEqual(scheduled.task)
 expect(calendar.task.specification.description).toBe('Persisted scheduled task')
 expect(calendar.task.specification.schedule).toEqual({ start_date: '2028-02-28', end_date: '2028-03-01', due_date: '2028-03-02' })
 expect(calendar.comments!.comments[0].body).toBe('Use the **native calendar**.')
 expect(calendar.history!.events.length).toBeGreaterThan(0)
 expect(dependent.task.depends_on).toEqual([calendar.task.specification.id])
 expect(dependent.task.specification.schedule).toBeNull()
 expect(/"(?:claim_token|access_token|refresh_token|token)"\s*:/.test(json)).toBe(false)
 expect(json.includes(fixture.cookie!)).toBe(false)
 expect(click).toHaveBeenCalledOnce()
 expect(filename).toBe('graphit-tasks.json')
 expect(revokeObjectURL).toHaveBeenCalledWith('blob:native-tasks')
 click.mockRestore()
 await user.click(screen.getByRole('link', { name: 'Open session' }))
 expect(await screen.findByRole('heading', { name: 'Request' })).toBeTruthy()
 expect(screen.getByText('Persisted browser corpus')).toBeTruthy()
 expect(actualSession).toBeDefined()
 expect(screen.getAllByText(new Date(Number(actualSession!.session.created_at)).toLocaleString()).length).toBeGreaterThan(0)
 expect(screen.getByRole('link', { name: 'Browser dependent' })).toBeTruthy()
 page.unmount()
 expect(useTaskWorkspace.getState().selection).toBeNull()
 expect(useTaskWorkspace.getState().phase).toBe('loading')
 expect(reads.find(read => read.operation === 'contexts')!.signal!.aborted).toBe(true)
}, 15000)
