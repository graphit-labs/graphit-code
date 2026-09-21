import '@/test/contextControls'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Link, MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { WorkspaceSelectors } from './WorkspaceSelectors'
import { WorkspaceRefreshProvider, usePageRefresh, refreshAll } from './WorkspaceRefresh'

const deferred = () => {
  let resolve!: () => void
  const promise = new Promise<void>(done => { resolve = done })
  return { promise, resolve }
}
function Sources({ list, detail }: { list: () => Promise<unknown>; detail: () => Promise<unknown> }) {
  usePageRefresh(list)
  usePageRefresh(detail)
  return <input aria-label="Draft" defaultValue="Keep my draft" />
}
function mount(list: () => Promise<unknown>, detail: () => Promise<unknown>) {
  render(<MemoryRouter initialEntries={['/one']}><WorkspaceRefreshProvider>
    <WorkspaceSelectors /><Link to="/two">Another page</Link>
    <Routes><Route path="/one" element={<Sources list={list} detail={detail} />} /><Route path="/two" element={<p>Second page</p>} /></Routes>
  </WorkspaceRefreshProvider></MemoryRouter>)
}
beforeEach(() => useAppStore.setState({ activeProjectDir: '/one', activeAgent: 'codex', projectsError: '', projectsLoaded: true, projects: [], supportedAgents: [], loadProjects: vi.fn(async () => {}) }))

describe('Combined header refresh', () => {
  it('loads global context and every page source once, waits for all and preserves drafts', async () => {
    const user = userEvent.setup(), catalog = deferred(), first = deferred(), second = deferred()
    const global = vi.fn(() => catalog.promise), list = vi.fn(() => first.promise), detail = vi.fn(() => second.promise)
    useAppStore.setState({ loadProjects: global }); mount(list, detail)
    const refresh = screen.getByRole('button', { name: 'Refresh' }) as HTMLButtonElement
    await user.click(refresh); await user.click(refresh)
    expect(global).toHaveBeenCalledOnce(); expect(list).not.toHaveBeenCalled(); expect(refresh.disabled).toBe(true)
    await act(async () => catalog.resolve())
    expect(list).toHaveBeenCalledOnce(); expect(detail).toHaveBeenCalledOnce()
    await act(async () => first.resolve()); expect(refresh.disabled).toBe(true)
    await act(async () => second.resolve()); expect(refresh.disabled).toBe(false)
    expect((screen.getByLabelText('Draft') as HTMLInputElement).value).toBe('Keep my draft')
  })

  it.each(['route', 'project'])('does not invoke old callbacks after a %s change during global refresh', async kind => {
    const user = userEvent.setup(), catalog = deferred(), list = vi.fn(async () => {}), detail = vi.fn(async () => {})
    useAppStore.setState({ loadProjects: () => catalog.promise }); mount(list, detail)
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    if (kind === 'route') await user.click(screen.getByRole('link', { name: 'Another page' }))
    else act(() => useAppStore.setState({ activeProjectDir: '/two' }))
    await act(async () => catalog.resolve())
    expect(list).not.toHaveBeenCalled(); expect(detail).not.toHaveBeenCalled()
  })

  it('waits for remaining sources after a failure and allows another attempt', async () => {
    const user = userEvent.setup(), pending = deferred()
    const list = vi.fn().mockRejectedValueOnce(new Error('Catalogue unavailable')).mockResolvedValue(undefined)
    const detail = vi.fn(() => pending.promise); mount(list, detail)
    const refresh = screen.getByRole('button', { name: 'Refresh' }) as HTMLButtonElement
    await user.click(refresh); expect(refresh.disabled).toBe(true)
    await act(async () => pending.resolve())
    expect(screen.getByRole('alert').textContent).toContain('Catalogue unavailable')
    await user.click(refresh)
    await waitFor(() => expect(refresh.disabled).toBe(false))
    expect(list).toHaveBeenCalledTimes(2); expect(screen.queryByRole('alert')).toBeNull()
  })

  it('still refreshes page sources when the global catalogue fails', async () => {
    const user = userEvent.setup(), list = vi.fn(async () => {}), detail = vi.fn(async () => {})
    useAppStore.setState({ loadProjects: vi.fn(async () => { throw new Error('Projects unavailable') }) })
    mount(list, detail); await user.click(screen.getByRole('button', { name: 'Refresh' }))
    expect(list).toHaveBeenCalledOnce(); expect(detail).toHaveBeenCalledOnce()
    expect(screen.getByRole('alert').textContent).toContain('Projects unavailable')
  })
})

it('waits for a nested source after its sibling fails, and retains typed results', async () => {
  const pending = deferred(), failure = new Error('First source failed')
  let settled = false
  const result = refreshAll([Promise.reject(failure), pending.promise] as const).catch(error => { settled = true; return error })
  await Promise.resolve(); await Promise.resolve()
  expect(settled).toBe(false)
  pending.resolve(); expect(await result).toBe(failure)
  expect(await refreshAll([Promise.resolve('schema'), Promise.resolve(2)] as const)).toEqual(['schema', 2])
})
