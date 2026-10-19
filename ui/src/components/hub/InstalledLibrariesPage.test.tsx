import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import InstalledLibrariesPage from './InstalledLibrariesPage'
import { useAppStore } from '@/store/appStore'
import type { InstalledLibrary, InstalledLibraryPage } from '@/api/installedLibraries'
const entry = (version: string, name = 'Published ' + version): InstalledLibrary => ({ artifact_type: 'knowledge', id: 'vendor/library', version, producer: '01KSH1CRFFG8Z74B5ZS78WW808', target: 'global_native', origin: 'HubCache', provider: 'broker', alias: null, entry_name: null, display: { name, description: '**Reusable** project knowledge.', tags: ['rust'] } })
const page = (entries: InstalledLibrary[], next_offset: number | null = null): InstalledLibraryPage => ({ scope: 'global_installation', entries, next_offset })
const reply = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status })
beforeEach(() => {
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
  useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'broker', subject: 'first-user' }, activeProjectKey: 'workspace:a', activeProjectId: 'a', activeProjectOrigin: 'workspace', activeProjectVersion: 'main', activeAgent: 'kiro-v3' })
})
afterEach(() => { cleanup(); vi.unstubAllGlobals() })
it('renders exact versions and author metadata, supports keyboard details, and has no pretend mutations', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => reply(page([entry('release-1'), entry('release-2')]))))
  const user = userEvent.setup(); render(<InstalledLibrariesPage />)
  const first = await screen.findByRole('button', { name: /Published release-1/ })
  first.focus(); await user.keyboard('{Enter}')
  const details = screen.getByRole('region', { name: 'Library details' })
  expect(details).toHaveFocus(); expect(within(details).getByText('release-1')).toBeTruthy()
  expect(within(details).getByText('Reusable').tagName).toBe('STRONG')
  expect(within(details).getByText('Published library')).toBeTruthy()
  await user.keyboard('{Escape}'); expect(screen.queryByRole('region', { name: 'Library details' })).toBeNull(); expect(first).toHaveFocus()
  expect(screen.queryByRole('button', { name: /^(install|remove|update|publish|refresh)(\s|$)/i })).toBeNull()
})
it('applies typed filters and server pagination without an invented total', async () => {
  const fetcher = vi.fn(async (_url: string, init: RequestInit) => {
    const query = JSON.parse(init.body as string)
    return reply(query.offset === 0 ? page([entry('release-1')], 1) : page([entry('release-2')]))
  }); vi.stubGlobal('fetch', fetcher)
  const user = userEvent.setup(); render(<InstalledLibrariesPage />)
  await screen.findByRole('button', { name: /Published release-1/ }); await user.click(screen.getByRole('button', { name: 'Next' }))
  await screen.findByRole('button', { name: /Published release-2/ }); expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled()
  await user.type(screen.getByRole('textbox', { name: 'Artifact identifier' }), 'vendor/library')
  await user.type(screen.getByRole('textbox', { name: 'Version' }), 'release-1')
  await user.click(screen.getByRole('button', { name: 'Apply filters' }))
  await waitFor(() => expect(JSON.parse(fetcher.mock.calls.at(-1)![1].body as string)).toMatchObject({ scope: 'global_installation', offset: 0, id: 'vendor/library', version: 'release-1' }))
})
it('waits for identity, discards previous accounts and retains global inventory across project changes', async () => {
  useAppStore.setState({ webIdentity: { state: 'loading' } })
  let first!: (value: Response) => void
  const fetcher = vi.fn().mockImplementationOnce(() => new Promise<Response>(resolve => { first = resolve })).mockImplementation(async () => reply(page([entry('current')])) )
  vi.stubGlobal('fetch', fetcher); render(<InstalledLibrariesPage />)
  expect(fetcher).not.toHaveBeenCalled()
  act(() => useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'broker', subject: 'first-user' } }))
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1))
  act(() => useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'other-broker', subject: 'second-user' } }))
  await screen.findByRole('button', { name: /Published current/ })
  await act(async () => { first(reply(page([entry('private-old-account')]))); await Promise.resolve() })
  expect(screen.queryByText(/private-old-account/)).toBeNull()
  act(() => useAppStore.setState({ activeProjectKey: 'hub:other', activeProjectId: 'other', activeProjectOrigin: 'hub', activeProjectVersion: 'release', activeAgent: 'codex' }))
  await act(async () => { await Promise.resolve() })
  expect(fetcher).toHaveBeenCalledTimes(2)
  act(() => useAppStore.setState({ webIdentity: { state: 'anonymous' } }))
  expect(screen.queryByRole('button', { name: /Published current/ })).toBeNull(); expect(screen.getByText('Sign in to inspect installed libraries')).toBeTruthy()
})
it('distinguishes forbidden and failed reads from a successful empty inventory', async () => {
  const fetcher = vi.fn(async () => reply({ error: { code: 'permission_denied' } }, 403)); vi.stubGlobal('fetch', fetcher)
  const { unmount } = render(<InstalledLibrariesPage />)
  expect(await screen.findByRole('alert')).toHaveTextContent('Library access unavailable'); expect(screen.queryByText('No installed libraries')).toBeNull()
  unmount(); fetcher.mockImplementation(async () => reply({ error: { code: 'hub_inventory_unavailable' } }, 500))
  const second = render(<InstalledLibrariesPage />); expect(await screen.findByRole('alert')).toHaveTextContent('could not be loaded'); second.unmount()
  fetcher.mockImplementation(async () => reply(page([])))
  render(<InstalledLibrariesPage />); expect(await screen.findByText('No installed libraries')).toBeTruthy()
})
it('distinguishes a filtered empty result from an empty installation', async () => {
  const fetcher = vi.fn(async () => reply(page([]))); vi.stubGlobal('fetch', fetcher)
  const user=userEvent.setup(); render(<InstalledLibrariesPage />); await screen.findByText('No installed libraries')
  await user.type(screen.getByRole('textbox', { name: 'Artifact identifier' }), 'vendor/long-library'); await user.click(screen.getByRole('button', { name: 'Apply filters' }))
  expect(await screen.findByText('No libraries match these filters')).toBeTruthy()
})

it('keeps long artifact identity literal in the list and inspector', async () => {
  const id='vendor/'+ 'long-library-'.repeat(12)
  const item={...entry('release-1'),id,display:null}
  vi.stubGlobal('fetch',vi.fn(async()=>reply(page([item]))))
  const user=userEvent.setup();render(<InstalledLibrariesPage />)
  await user.click(await screen.findByRole('button',{name:new RegExp(id)}))
  const details=screen.getByRole('region',{name:'Library details'})
  expect(within(details).getByText(id,{selector:'dd'})).toHaveTextContent(id)
  expect(within(details).queryByRole('link')).toBeNull()
})
