import '@/test/contextControls'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, expect, it, vi } from 'vitest'
import { nativeRecordRead } from '@/api/nativeRecordRead'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import { LocalProjectsDashboard } from './LocalProjectsDashboard'

vi.mock('@/api/nativeRecordRead', async original => ({
  ...await original<typeof import('@/api/nativeRecordRead')>(), nativeRecordRead: vi.fn(),
}))

afterEach(() => { cleanup(); vi.resetAllMocks(); vi.restoreAllMocks() })

it('lists registered offline projects, follows pagination and discovers checkouts by label', async () => {
  const first = { project_id: 'p1', project_dir: '/work/one', project_version: 'main' }
  const second = { project_id: 'p2', project_dir: '/work/two', project_version: 'next' }
  useTaskWorkspace.setState({ module: 'now', selection: first, contexts: [first, second], phase: 'ready' })
  vi.mocked(nativeRecordRead).mockResolvedValueOnce({ next_cursor: 'second', projects: [
    { project_id: 'p1', name: 'Project One', cluster: { team: ['compiler'] }, online: true, versions: ['main'], checkouts: [{ dir: first.project_dir, exists: true, online: true, current_version: 'main' }] },
    { project_id: 'p2', name: 'Project Two', cluster: { team: ['compiler'] }, online: true, versions: ['next'], checkouts: [{ dir: second.project_dir, exists: true, online: true, current_version: 'next' }] },
  ] }).mockResolvedValueOnce({ next_cursor: null, projects: [
    { project_id: 'p3', name: 'Offline Project', cluster: { team: ['docs'] }, online: false, versions: ['main'], checkouts: [{ dir: '/work/offline', exists: true, online: false, current_version: null }] },
  ] })
  const user = userEvent.setup()
  render(<MemoryRouter><LocalProjectsDashboard /></MemoryRouter>)
  const directory = within(await screen.findByLabelText('Local project directory', { selector: 'div' }))
  await user.click(await directory.findByRole('button', { name: 'Offline Project' }))
  expect(screen.getByText('Registered · offline')).toBeTruthy()
  expect(screen.queryByRole('button', { name: 'Use checkout' })).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Same Cluster' }))
  expect(directory.queryByRole('button', { name: 'Offline Project' })).toBeNull()
  await user.click(directory.getByRole('button', { name: 'Project Two' }))
  await user.click(screen.getByRole('button', { name: 'Use checkout' }))
  expect(useTaskWorkspace.getState().selection).toEqual(second)
  expect(nativeRecordRead).toHaveBeenNthCalledWith(2, '/projects/local/list', { cursor: 'second' }, expect.any(AbortSignal))
})

it('keeps a failed label draft, updates the catalogue on success and confirms removal', async () => {
  const current = { project_id: 'p1', project_dir: '/work/one', project_version: 'feature' }
  const project = { project_id: 'p1', name: 'Project One', online: true, versions: ['feature'], checkouts: [{ dir: current.project_dir, exists: true, online: true, current_version: 'feature' }] }
  const page = (cluster: Record<string, string[]>) => ({ projects: [{ ...project, cluster }], next_cursor: null })
  useTaskWorkspace.setState({ module: 'now', selection: current, contexts: [current], phase: 'ready' })
  vi.mocked(nativeRecordRead)
    .mockResolvedValueOnce(page({}))
    .mockRejectedValueOnce(new Error('Label write was denied.'))
    .mockResolvedValueOnce({ status: 'ok' }).mockResolvedValueOnce(page({ team: ['compiler'] }))
    .mockResolvedValueOnce({ status: 'ok' }).mockResolvedValueOnce(page({}))
  const confirmation = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true)
  const user = userEvent.setup()
  render(<MemoryRouter><LocalProjectsDashboard /></MemoryRouter>)
  const key = await screen.findByLabelText('Label key')
  const value = screen.getByLabelText('Label value')
  await user.type(key, 'team')
  await user.type(value, 'compiler')
  await user.click(screen.getByRole('button', { name: 'Add label' }))
  expect(await screen.findByText('Label write was denied.')).toBeTruthy()
  expect((value as HTMLInputElement).value).toBe('compiler')
  await user.click(screen.getByRole('button', { name: 'Add label' }))
  const remove = await screen.findByRole('button', { name: 'Remove label team = compiler' })
  await user.click(remove)
  expect(nativeRecordRead).toHaveBeenCalledTimes(4)
  await user.click(remove)
  await screen.findByText('Label removed.')
  expect(confirmation).toHaveBeenCalledTimes(2)
  expect(nativeRecordRead).toHaveBeenNthCalledWith(5, '/projects/local/labels', { ...current, operation: 'unset', key: 'team', value: 'compiler' })
  expect(nativeRecordRead).toHaveBeenNthCalledWith(3, '/projects/local/labels', { ...current, operation: 'set', key: 'team', value: 'compiler' })
})
