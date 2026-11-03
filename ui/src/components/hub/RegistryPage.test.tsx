import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubRequestError, type ArtifactPage } from '@/api/hubNative'
import { projectVersionsApi } from '@/api/projectVersions'
import { useAppStore } from '@/store/appStore'
import RegistryPage from './RegistryPage'
import { catalog, completed, hubContext, selectHubProject, projectB } from './hubTestFixtures'
vi.mock('@/api/hubNative', async importOriginal => ({ ...await importOriginal<typeof import('@/api/hubNative')>(), hubNative: { projects: vi.fn(), search: vi.fn(), installed: vi.fn(), content: vi.fn(), mutate: vi.fn() } }))
vi.mock('@/api/projectVersions', () => ({ projectVersionsApi: { list: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.projects).mockResolvedValue({ status: 'ok', projects: [{ id: projectB, name: 'Publisher' }], next_cursor: null })
  vi.mocked(hubNative.search).mockResolvedValue(catalog)
  vi.mocked(hubNative.mutate).mockImplementation(async (operation, input) => completed(operation, input))
})
afterEach(cleanup)
it('native path install renders Rust parent residues through the real Registry consumer', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (operation, request) => ({
    ...completed(operation, request), producer: projectB, binding: request.binding,
    publication: 'Physical(Created)',
    possible_parents: ['/project/.agents/skills'],
  }))
  const user = userEvent.setup(); render(<RegistryPage />)
  const dialog = await openInstall(user)
  await user.type(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' }), 'ui-mode-copy')
  await user.click(within(dialog).getByRole('button', { name: 'Install artifact' }))
  await screen.findByText('Operation completed')
  expect(screen.getByText('/project/.agents/skills')).toBeInTheDocument()
  expect(screen.getByText('Physical(Created)')).toBeInTheDocument()
  expect(screen.getByText('Copy')).toBeInTheDocument()
  expect(hubNative.mutate).toHaveBeenCalledOnce()
  expect(hubNative.mutate).toHaveBeenCalledWith('install', expect.objectContaining({
    alias: 'ui-mode-copy', binding: 'kiro-v3',
    selection: expect.objectContaining({ producer: projectB, id: 'vendor/reviewer', version: 'release-1' }),
  }), expect.any(AbortSignal))
  expect(screen.getByRole('button', { name: 'Inspect captured target' })).toBeEnabled()
})
async function search(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByRole('combobox', { name: 'Publisher project ID' }), projectB)
  await user.click(screen.getByRole('button', { name: 'Find artifacts' }))
}
async function openInstall(user: ReturnType<typeof userEvent.setup>) {
  await search(user)
  await user.click(await screen.findByRole('button', { name: /Engineering reviewer/ }))
  await user.click(screen.getByRole('combobox', { name: 'Exact version' }))
  await user.click(screen.getByRole('option', { name: 'release-1' }))
  await user.click(screen.getByRole('button', { name: 'Install artifact' }))
  return screen.getByRole('dialog')
}

it('dismisses the captured install dialog on a PROJECT/version/binding change without sending a mutation', async () => {
  const user = userEvent.setup(); render(<RegistryPage />)
  await openInstall(user)
  act(() => { selectHubProject(projectB, 'feature/b', '/b'); useAppStore.setState({ activeAgent: 'antigravity' }) })
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  expect(hubNative.mutate).not.toHaveBeenCalled()
  expect(useAppStore.getState().activeAgent).toBe('antigravity')
  await act(async () => hubContext())
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

it('aborts and ignores deferred metadata from an earlier Web identity', async () => {
  let resolve!: (value: ArtifactPage) => void
  vi.mocked(hubNative.search).mockImplementationOnce(() => new Promise(result => { resolve = result }))
  const user = userEvent.setup(); render(<RegistryPage />)
  await search(user)
  await waitFor(() => expect(hubNative.search).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(hubNative.search).mock.calls[0][1]
  act(() => useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'second-user' } }))
  vi.mocked(hubNative.search).mockResolvedValue({ ...catalog, artifacts: [{ ...catalog.artifacts[0], name: 'Current artifact' }] })
  await search(user)
  await screen.findByRole('button', { name: /Current artifact/ })
  await act(async () => resolve({ ...catalog, artifacts: [{ ...catalog.artifacts[0], name: 'Stale private artifact' }] }))
  expect(signal?.aborted).toBe(true)
  expect(screen.queryByText('Stale private artifact')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: /Current artifact/ })).toBeInTheDocument()
})

it('renders authored Markdown outside the record selection button and keeps latest uninferred', async () => {
  const user = userEvent.setup(); render(<RegistryPage />); await search(user)
  expect((await screen.findByText('Reusable context')).tagName).toBe('STRONG')
  expect(screen.getByText('contracts').closest('li')).not.toBeNull()
  expect(screen.getByRole('link', { name: 'Read guide' }).closest('button')).toBeNull()
  await user.click(screen.getByRole('button', { name: /Engineering reviewer/ }))
  expect(screen.getByRole('button', { name: 'Install artifact' })).toBeDisabled()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it('retains an alias draft after validation failure and submits the exact captured target on explicit retry', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (operation, request) => {
    throw new HubRequestError(400, 'invalid_arguments', { status: 'error', operation, scope: request.scope, cache: 'not_attempted', publication: 'not_attempted' }, false, null, request)
  })
  const user = userEvent.setup(); render(<RegistryPage />)
  const dialog = await openInstall(user)
  await user.type(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' }), 'bad/name')
  await user.click(within(dialog).getByRole('button', { name: 'Install artifact' }))
  await screen.findByText(/your entered values are retained/)
  expect(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' })).toHaveValue('bad/name')
  await user.clear(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' }))
  await user.type(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' }), 'team-reviewer')
  await user.click(within(dialog).getByRole('button', { name: 'Install artifact' }))
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  expect(hubNative.mutate).toHaveBeenLastCalledWith('install', expect.objectContaining({ scope: { kind: 'project', project_id: useAppStore.getState().activeProjectId, project_version: 'feature/a' }, binding: 'kiro-v3', alias: 'team-reviewer', selection: expect.objectContaining({ producer: projectB, version: 'release-1', exact: true }) }), expect.any(AbortSignal))
  expect(screen.getByText('Operation completed')).toBeInTheDocument()
})

it('keeps a cursor conflict distinct from empty and restarts only after an explicit action', async () => {
  vi.mocked(hubNative.search).mockRejectedValueOnce(new HubRequestError(409, 'catalog_restart_required', null, false, null, {}))
  const user = userEvent.setup(); render(<RegistryPage />); await search(user)
  await screen.findByText('The catalog changed')
  expect(screen.queryByText('No published artifacts')).not.toBeInTheDocument()
  expect(hubNative.search).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button', { name: 'Restart from first page' }))
  await screen.findByRole('button', { name: /Engineering reviewer/ })
  expect(hubNative.search).toHaveBeenCalledTimes(2)
})

it('uses the authorized Hub data version and never a legacy version client', async () => {
  act(() => { selectHubProject(projectB, 'actual/checkout', '/b'); useAppStore.setState({ activeProjectVersion: 'untrusted/legacy' }) })
  const user = userEvent.setup(); render(<RegistryPage />)
  const dialog = await openInstall(user)
  expect(within(dialog).getByText(/actual\/checkout/)).toBeInTheDocument()
  expect(projectVersionsApi.list).not.toHaveBeenCalled()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it('refreshes the current catalog query when an earlier mutation completes', async () => {
  let finish!: (value: Awaited<ReturnType<typeof hubNative.mutate>>) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<RegistryPage />)
  const dialog = await openInstall(user)
  await user.click(within(dialog).getByRole('button', { name: 'Install artifact' }))
  const [, captured] = vi.mocked(hubNative.mutate).mock.calls[0]
  await user.click(within(dialog).getByRole('button', { name: 'Close' }))
  await user.type(screen.getByRole('textbox', { name: 'Exact artifact ID (optional)' }), 'current/query')
  await user.click(screen.getByRole('button', { name: 'Find artifacts' }))
  await waitFor(() => expect(hubNative.search).toHaveBeenCalledTimes(2))
  await act(async () => finish(completed('install', captured)))
  await waitFor(() => expect(hubNative.search).toHaveBeenCalledTimes(3))
  expect(hubNative.search).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'current/query' }), expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  expect(captured).toMatchObject({ selection: { id: 'vendor/reviewer', version: 'release-1' } })
  expect(screen.getByText('Operation completed')).toBeInTheDocument()
})

const publisherDenied = () => new HubRequestError(403, 'permission_denied', null, false, null, {})
async function standalone(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('combobox', { name: 'Source' }))
  await user.click(screen.getByRole('option', { name: 'Standalone storage' }))
}
it('clears publisher discovery from the previous source without hiding a current denial', async () => {
  vi.mocked(hubNative.projects).mockRejectedValue(publisherDenied())
  const user = userEvent.setup(); render(<RegistryPage />)
  await screen.findByText('Access unavailable')
  await standalone(user)
  expect(screen.queryByText('Access unavailable')).not.toBeInTheDocument()
  expect(hubNative.projects).toHaveBeenCalledTimes(1)
  await user.type(screen.getByRole('textbox', { name: 'Provider' }), '-storage')
  await user.type(screen.getByRole('textbox', { name: 'Storage profile (optional)' }), 'catalog-keys')
  await user.click(screen.getByRole('button', { name: 'Browse publishers' }))
  await screen.findByText('Access unavailable')
  expect(hubNative.projects).toHaveBeenLastCalledWith({ transport: 'standalone', provider: 'corporate-storage', profile: 'catalog-keys', limit: 20 }, expect.any(AbortSignal))
  await search(user)
  await screen.findByRole('button', { name: /Engineering reviewer/ })
  expect(screen.getByText('Access unavailable')).toBeInTheDocument()
  expect(hubNative.projects).toHaveBeenCalledTimes(2)
  await user.type(screen.getByRole('textbox', { name: 'Provider' }), ' ')
  expect(screen.getByText('Access unavailable')).toBeInTheDocument()
  await user.type(screen.getByRole('textbox', { name: 'Provider' }), 'other')
  expect(screen.queryByText('Access unavailable')).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Browse publishers' }))
  await screen.findByText('Access unavailable')
  await user.type(screen.getByRole('textbox', { name: 'Storage profile (optional)' }), '-next')
  expect(screen.queryByText('Access unavailable')).not.toBeInTheDocument()
  expect(hubNative.projects).toHaveBeenCalledTimes(3)
})

it.each(['resolve', 'reject'] as const)('discards a deferred publisher %s after changing its source', async outcome => {
  let finish!: () => void
  vi.mocked(hubNative.projects).mockImplementationOnce(() => new Promise((resolve, reject) => { finish = () => outcome === 'resolve'
    ? resolve({ status: 'ok', projects: [{ id: projectB, name: 'Obsolete publisher' }], next_cursor: 'obsolete-cursor' })
    : reject(publisherDenied()) }))
  const user = userEvent.setup(); render(<RegistryPage />)
  await waitFor(() => expect(hubNative.projects).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(hubNative.projects).mock.calls[0][1]
  await standalone(user)
  expect(signal?.aborted).toBe(true)
  expect(screen.getByRole('button', { name: 'Browse publishers' })).toBeEnabled()
  await act(async () => finish())
  expect(screen.queryByText('Access unavailable')).not.toBeInTheDocument()
  expect(screen.queryByRole('option', { name: 'Obsolete publisher' })).not.toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Next publisher page' })).not.toBeInTheDocument()
  expect(hubNative.projects).toHaveBeenCalledTimes(1)
})

it('physical mode default install omits mode and GlobalCache never exposes or sends PROJECT-only fields',async()=>{
  const user=userEvent.setup();render(<RegistryPage/>);const first=await openInstall(user)
  expect(first).toHaveTextContent('copies it into the checkout')
  await user.click(within(first).getByRole('button',{name:'Install artifact'}));await screen.findByText('Operation completed')
  expect(vi.mocked(hubNative.mutate).mock.calls[0][1]).not.toHaveProperty('mode')
  await user.click(screen.getByRole('combobox',{name:'Installation destination'}));await user.click(screen.getByRole('option',{name:'Installation cache'}))
  await user.click(screen.getByRole('button',{name:'Install artifact'}));const cacheDialog=screen.getByRole('dialog')
  expect(within(cacheDialog).queryByRole('combobox',{name:'Installation mode'})).not.toBeInTheDocument()
  expect(within(cacheDialog).queryByRole('textbox',{name:'Local alias (optional)'})).not.toBeInTheDocument()
  await user.click(within(cacheDialog).getByRole('button',{name:'Install artifact'}))
  await waitFor(()=>expect(hubNative.mutate).toHaveBeenCalledTimes(2))
  const request=vi.mocked(hubNative.mutate).mock.calls[1][1]
  expect(request.scope).toEqual({kind:'global_cache'})
  for(const field of ['mode','binding','alias','entry_name'])expect(request).not.toHaveProperty(field)
})
it('unsupported installation retains the alias draft for explicit retry',async()=>{
  vi.mocked(hubNative.search).mockResolvedValue({ ...catalog, artifacts: [{ ...catalog.artifacts[0], artifact_type: 'rule' }] })
  vi.mocked(hubNative.mutate).mockImplementationOnce(async(operation,request)=>{throw new HubRequestError(501,'unsupported',{status:'error',operation,scope:request.scope,cache:'not_attempted',publication:'not_attempted'},false,null,request)})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await openInstall(user)
  await user.type(within(dialog).getByRole('textbox',{name:'Local alias (optional)'}),'team-copy')
  await user.click(within(dialog).getByRole('button',{name:'Install artifact'}))
  await within(dialog).findByText('Operation unavailable')
  expect(within(dialog).getByRole('textbox',{name:'Local alias (optional)'})).toHaveValue('team-copy')
  expect(screen.queryByText('Operation completed')).not.toBeInTheDocument()
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  await user.click(within(dialog).getByRole('button',{name:'Install artifact'}));await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenLastCalledWith('install',expect.objectContaining({alias:'team-copy'}),expect.any(AbortSignal))
})

it('Escape closes the project install dialog and restores focus',async()=>{
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await openInstall(user)
  within(dialog).getByRole('textbox',{name:'Local alias (optional)'}).focus()
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(screen.getByRole('button',{name:'Install artifact'})).toHaveFocus()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it.each(['agent', 'skill', 'command', 'workflow'] as const)('rendered %s installation offers Copy without Link', async artifact_type => {
  vi.mocked(hubNative.search).mockResolvedValue({ ...catalog, artifacts: [{ ...catalog.artifacts[0], artifact_type }] })
  const user = userEvent.setup(); render(<RegistryPage />); const dialog = await openInstall(user)
  expect(dialog).toHaveTextContent('Graphit renders this resource for the selected agent and copies it into the checkout.')
  expect(within(dialog).queryByRole('combobox', { name: 'Installation mode' })).not.toBeInTheDocument()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})
