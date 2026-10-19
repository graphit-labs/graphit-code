import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubRequestError, type FileOutcome } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import ProjectArtifactsPage from './ProjectArtifactsPage'
import { completed, hubContext, installedPage, projectA, projectB, selectHubProject } from './hubTestFixtures'

vi.mock('@/api/hubNative', async original => ({ ...await original<typeof import('@/api/hubNative')>(), hubNative: { authoredList: vi.fn(), authoredReview: vi.fn(), authoredContent: vi.fn(), installed: vi.fn(), mutate: vi.fn(), publish: vi.fn(), search: vi.fn(), content: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
const resource = { artifact_type: 'skill' as const, id: 'vendor/local-reviewer', version: 'artifact/local', name: null }
const source = { project_id: projectA, project_version: 'feature/a', artifact_type: resource.artifact_type, id: resource.id, version: resource.version }
const request = () => ({ source, scope: { kind: 'project' as const, project_id: projectA, project_version: 'feature/a' }, binding: 'kiro-v3' as const })
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([]), scope: query.scope }))
  vi.mocked(hubNative.authoredList).mockImplementation(async query => ({ status: 'ok', project_id: query.project_id, project_version: query.project_version, entries: [resource], next_cursor: null }))
  vi.mocked(hubNative.authoredReview).mockImplementation(async query => ({ ...query, status: 'ok', source: 'Resources', metadata: null, entries: [{ path: 'SKILL.md', kind: 'File', bytes: 15 }], next_offset: null }))
  vi.mocked(hubNative.mutate).mockImplementation(async (operation, input) => completed(operation, input))
})
afterEach(cleanup)
async function selectResource(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Browse project resources' }))
  await user.click(await screen.findByRole('button', { name: /vendor\/local-reviewer/ }))
  await screen.findByRole('button', { name: 'Read SKILL.md' })
}
async function open(user: ReturnType<typeof userEvent.setup>) { await selectResource(user); await user.click(screen.getByRole('button', { name: 'Install local resource' })); return screen.getByRole('dialog') }
async function mode(user: ReturnType<typeof userEvent.setup>, label: string) { await user.click(within(screen.getByRole('dialog')).getByRole('combobox', { name: 'Installation mode' })); await user.click(screen.getByRole('option', { name: label })) }

it.each(['agent', 'skill', 'command', 'workflow'] as const)('local install consumer captures selected %s source and current PROJECT/binding, omitting optional intent', async artifact_type => {
  vi.mocked(hubNative.authoredList).mockImplementation(async query => ({ status: 'ok', project_id: query.project_id, project_version: query.project_version, entries: [{ ...resource, artifact_type }], next_cursor: null }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  expect(dialog).toHaveTextContent('Local Resources'); expect(dialog).toHaveTextContent(projectA + ' @ feature/a')
  expect(within(dialog).queryByRole('textbox', { name: /Version/ })).not.toBeInTheDocument()
  expect(dialog).not.toHaveTextContent('managed Hub cache')
  await user.click(within(dialog).getByRole('combobox', { name: 'Installation mode' }))
  expect(screen.queryByRole('option', { name: /Link/ })).not.toBeInTheDocument()
  await user.keyboard('{Escape}')
  expect(within(dialog).getByRole('combobox', { name: 'Installation mode' })).toHaveFocus()
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' }))
  await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledOnce()
  expect(hubNative.mutate).toHaveBeenCalledWith('install_local', { ...request(), source: { ...source, artifact_type } }, expect.any(AbortSignal))
  expect(screen.getByRole('region', { name: 'Operation outcome' })).toHaveTextContent('Default Copy (omitted)')
  expect(hubNative.publish).not.toHaveBeenCalled(); expect(hubNative.content).not.toHaveBeenCalled()
})

it.each(['rule', 'mcp', 'language', 'power', 'ast', 'knowledge'] as const)('local install consumer keeps %s readable without exposing unsupported local installation', async artifact_type => {
  vi.mocked(hubNative.authoredList).mockImplementation(async query => ({ status: 'ok', project_id: query.project_id, project_version: query.project_version, entries: [{ ...resource, artifact_type }], next_cursor: null }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await selectResource(user)
  expect(screen.queryByRole('button', { name: 'Install local resource' })).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Read SKILL.md' })).toBeEnabled()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it.each([['Copy', 'copy']] as const)('local install consumer sends %s and explicit names without fabricating observed mode', async (label, value) => {
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await mode(user, label)
  await user.type(within(dialog).getByRole('textbox', { name: 'Local alias (optional)' }), 'local-alias')
  await user.type(within(dialog).getByRole('textbox', { name: 'Entry name (optional)' }), 'LOCAL.md')
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledWith('install_local', { ...request(), mode: value, alias: 'local-alias', entry_name: 'LOCAL.md' }, expect.any(AbortSignal))
  const outcome = screen.getByRole('region', { name: 'Operation outcome' })
  expect(outcome).toHaveTextContent('Requested installation mode'); expect(outcome).toHaveTextContent(label)
  expect(outcome).toHaveTextContent('Requested alias'); expect(outcome).toHaveTextContent('local-alias')
  expect(outcome).not.toHaveTextContent('Link to Hub cache')
})

it('local install consumer preserves literal invalid names and Unsupported choice for explicit correction without fallback', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (_operation, input) => { throw new HubRequestError(400, 'invalid_arguments', null, false, null, input) }).mockImplementationOnce(async (_operation, input) => { throw new HubRequestError(501, 'native_operation_failed', null, false, null, input) })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await mode(user, 'Copy')
  const alias = within(dialog).getByRole('textbox', { name: 'Local alias (optional)' })
  await user.type(alias, ' bad/name ')
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await within(dialog).findByText('Check the supplied selection')
  expect(hubNative.mutate).toHaveBeenLastCalledWith('install_local', { ...request(), mode: 'copy', alias: ' bad/name ' }, expect.any(AbortSignal))
  expect(alias).toHaveValue(' bad/name ')
  await user.clear(alias); await user.type(alias, 'retained-alias')
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await within(dialog).findByText('Operation unavailable')
  expect(alias).toHaveValue('retained-alias'); expect(within(dialog).getByRole('combobox', { name: 'Installation mode' })).toHaveTextContent('Copy')
  expect(hubNative.mutate).toHaveBeenCalledTimes(2)
  await mode(user, 'Copy'); await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledTimes(3)
  expect(hubNative.mutate).toHaveBeenLastCalledWith('install_local', { ...request(), mode: 'copy', alias: 'retained-alias' }, expect.any(AbortSignal))
})

it('local install consumer retains the original source and target after a deferred PROJECT/binding transition', async () => {
  let finish!: (value: FileOutcome) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' }))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  act(() => { selectHubProject(projectB, 'other/data', '/other'); useAppStore.setState({ activeAgent: 'opencode' }) })
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  await waitFor(() => expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ scope: { kind: 'project', project_id: projectB, project_version: 'other/data' }, binding: 'opencode' }), expect.any(AbortSignal)))
  const reads = vi.mocked(hubNative.installed).mock.calls.length
  await act(async () => finish(completed('install_local', captured)))
  expect(signal?.aborted).toBe(false); expect(captured).toEqual(request())
  expect(hubNative.installed).toHaveBeenCalledTimes(reads)
  expect(screen.getByRole('region', { name: 'Operation outcome' })).toHaveTextContent(projectA + ' @ feature/a')
})

it.each(['identity', 'logout', 'apiBase'] as const)('local install consumer discards private deferred results after %s', async boundary => {
  let finish!: (value: FileOutcome) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' }))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  act(() => useAppStore.setState(boundary === 'logout' ? { webIdentity: { state: 'anonymous' } } : boundary === 'apiBase' ? { apiBase: '/another-api' } : { webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'second-user' } }))
  await waitFor(() => expect(signal?.aborted).toBe(true))
  await act(async () => finish(completed('install_local', captured)))
  expect(screen.queryByRole('region', { name: 'Operation outcome' })).not.toBeInTheDocument()
})

it('local install consumer inspects the captured destination after413 without retrying or moving to a new source', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (_operation, input) => { throw new HubRequestError(413, null, null, true, { cache: 'not_attempted', publication: { change: 'Unknown', committed: null }, observation_failed: true }, input) })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await within(dialog).findByText('The response exceeded its limit')
  await user.click(within(dialog).getByRole('button', { name: 'Close' }))
  act(() => { selectHubProject(projectB, 'other/data', '/other'); useAppStore.setState({ activeAgent: 'opencode' }) })
  await user.click(screen.getByRole('button', { name: 'Inspect captured target' })); await screen.findByText('Captured target inspected')
  expect(hubNative.installed).toHaveBeenLastCalledWith({ scope: request().scope, artifact_type: 'skill', id: resource.id, version: resource.version, binding: 'kiro-v3', offset: 0, limit: 20 }, expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledOnce()
})

it('local install consumer never sends a GlobalCache target and preserves shared modal keyboard/focus', async () => {
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  const select = within(dialog).getByRole('combobox', { name: 'Installation mode' })
  select.focus(); await user.keyboard('{Enter}{ArrowDown}{End}{Enter}')
  expect(select).toHaveTextContent('Copy')
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Install local resource' })).toHaveFocus()
  await user.click(screen.getByRole('combobox', { name: 'Inspect installations in' })); await user.click(screen.getByRole('option', { name: 'Installation cache' }))
  await selectResource(user)
  expect(screen.getByRole('button', { name: 'Install local resource' })).toBeDisabled()
  expect(screen.getByText('Choose Project selected in the header as the installation scope.')).toBeInTheDocument()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it('local install consumer retains a confirmed post-effect publication and native residues without claiming completed cleanup', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (operation, input) => {
    const outcome: FileOutcome = { ...completed(operation, input), status: 'error', cleanup_pending: true, publication: 'Physical(Created)', possible_parents: ["/ÿ"] }
    throw new HubRequestError(500, 'native_operation_failed', outcome, false, null, input)
  })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); const dialog = await open(user)
  await user.click(within(dialog).getByRole('button', { name: 'Install local resource' })); await within(dialog).findByText('Hub request failed')
  await user.click(within(dialog).getByRole('button', { name: 'Close' }))
  expect(screen.queryByText('Operation completed')).not.toBeInTheDocument()
  const outcome = screen.getByRole('region', { name: 'Operation outcome' })
  expect(outcome).toHaveTextContent('Physical(Created)')
  expect(outcome).toHaveTextContent('Publication cleanup remains pending.')
  expect(outcome).toHaveTextContent('/ÿ')
  expect(outcome).toHaveTextContent('Inspection required')
  expect(hubNative.mutate).toHaveBeenCalledOnce()
})
