import '@testing-library/jest-dom/vitest'
import '@/test/contextControls'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubUpdateAllError, type UpdateAllOutcome } from '@/api/hubNative'
import { partialUpdate, updateInput, updateOutcome } from '@/api/hubNativeUpdateAll.fixtures'
import { useAppStore } from '@/store/appStore'
import { hubContext, installed, installedPage, projectA, projectB, selectHubProject } from './hubTestFixtures'
import ProjectArtifactsPage from './ProjectArtifactsPage'
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
beforeEach(() => {
  hubContext(); useAppStore.setState({ apiBase: '', webMode: true })
  vi.spyOn(hubNative, 'installed').mockImplementation(async query => ({ ...installedPage([]), scope: query.scope }))
  vi.spyOn(hubNative, 'updateAll').mockImplementation(async input => updateOutcome(input))
})
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals() })
async function open(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Update all remote installations' })); return screen.getByRole('dialog')
}
async function apply(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' }))
}
it('confirms the entire captured scope independently of filters and preserves keyboard/focus without mutation on review', async () => {
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await screen.findByText('No installed FILE artifacts')
  await user.type(screen.getByRole('textbox', { name: 'Exact artifact ID (optional)' }), 'visible-only'); await user.click(screen.getByRole('button', { name: 'Apply filters' }))
  const trigger = screen.getByRole('button', { name: 'Update all remote installations' }), dialog = await open(user)
  expect(dialog).toHaveTextContent(projectA + ' @ feature/a'); expect(dialog).toHaveTextContent('Kiro V3'); expect(dialog).toHaveTextContent('filters and pages do not limit'); expect(hubNative.updateAll).not.toHaveBeenCalled()
  expect(within(dialog).queryByRole('textbox')).toBeNull(); await user.keyboard('{Escape}'); expect(screen.queryByRole('dialog')).toBeNull(); expect(trigger).toHaveFocus()
  await open(user); await apply(user); await screen.findByText('Remote installations updated')
  expect(hubNative.updateAll).toHaveBeenCalledWith(updateInput(), expect.any(AbortSignal)); expect(hubNative.updateAll).toHaveBeenCalledOnce()
  await waitFor(() => expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'visible-only', scope: updateInput().scope }), expect.any(AbortSignal)))
  const result = screen.getByRole('region', { name: 'Collective update outcome' }); expect(result).toHaveTextContent('release/one'); expect(result).toHaveTextContent('release/latest'); expect(result).toHaveTextContent('Physical(Updated)')
})
it('sends GlobalCache without binding, keeps Native Global absent, and renders an empty success neutrally', async () => {
  vi.mocked(hubNative.updateAll).mockImplementation(async input => updateOutcome(input, [])); render(<ProjectArtifactsPage />); const user = userEvent.setup()
  await user.click(screen.getByRole('combobox', { name: 'Inspect installations in' })); expect(screen.queryByRole('option', { name: 'Native Global' })).toBeNull(); await user.click(screen.getByRole('option', { name: 'Installation cache' }))
  await open(user); await apply(user); await screen.findByText('No remote installations to update')
  expect(hubNative.updateAll).toHaveBeenCalledWith({ scope: { kind: 'global_cache' } }, expect.any(AbortSignal)); expect(screen.queryByText('Remote installations updated')).toBeNull(); expect(screen.queryByRole('progressbar')).toBeNull()
})
it.each(['project', 'binding', 'scope'] as const)('keeps an in-flight original scope after %s changes without reloading the new target', async boundary => {
  let finish!: (value: UpdateAllOutcome) => void
  vi.mocked(hubNative.updateAll).mockImplementationOnce(() => new Promise(resolve => { finish = resolve })); render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const [captured, signal] = vi.mocked(hubNative.updateAll).mock.calls[0]; await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }))
  if (boundary === 'project') act(() => selectHubProject(projectB, 'other/version')); else if (boundary === 'binding') act(() => useAppStore.setState({ activeAgent: 'qwen' })); else { await user.click(screen.getByRole('combobox', { name: 'Inspect installations in' })); await user.click(screen.getByRole('option', { name: 'Installation cache' })) }
  await waitFor(() => expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining(boundary === 'project' ? { scope: { kind: 'project', project_id: projectB, project_version: 'other/version' } } : boundary === 'binding' ? { binding: 'qwen' } : { scope: { kind: 'global_cache' } }), expect.any(AbortSignal)))
  const calls = vi.mocked(hubNative.installed).mock.calls.length; expect(signal?.aborted).toBe(false)
  await act(async () => finish(updateOutcome(captured)))
  expect(screen.getByRole('region', { name: 'Collective update outcome' })).toHaveTextContent(projectA + ' @ feature/a'); expect(hubNative.installed).toHaveBeenCalledTimes(calls); expect(hubNative.updateAll).toHaveBeenCalledOnce()
})
it.each(['identity', 'logout', 'apiBase'] as const)('aborts and discards a collective result after %s changes', async boundary => {
  let finish!: (value: UpdateAllOutcome) => void
  vi.mocked(hubNative.updateAll).mockImplementationOnce(() => new Promise(resolve => { finish = resolve })); render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const [captured, signal] = vi.mocked(hubNative.updateAll).mock.calls[0]
  act(() => { if (boundary === 'identity') useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'next-user' } }); else if (boundary === 'logout') useAppStore.setState({ webIdentity: { state: 'anonymous' } }); else useAppStore.setState({ apiBase: '/other-api' }) })
  expect(signal?.aborted).toBe(true); await act(async () => finish(updateOutcome(captured))); expect(screen.queryByRole('region', { name: 'Collective update outcome' })).toBeNull(); expect(hubNative.updateAll).toHaveBeenCalledOnce()
})
it('retains partial and not_attempted effects, inspects every page of the original scope and never retries automatically', async () => {
  vi.mocked(hubNative.updateAll).mockImplementationOnce(async input => { throw new HubUpdateAllError(409, 'installation_conflict', partialUpdate(input), null, false, input) })
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const outcome = screen.getByRole('region', { name: 'Collective update outcome' }); expect(outcome).toHaveTextContent('409'); expect(outcome).toHaveTextContent('installation_changed'); expect(outcome).toHaveTextContent('not_attempted'); expect(outcome).toHaveTextContent('Physical(Updated)'); expect(outcome).toHaveTextContent('Unknown')
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' })).toBeDisabled(); await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }))
  act(() => selectHubProject(projectB, 'other/version')); await waitFor(() => expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ scope: { kind: 'project', project_id: projectB, project_version: 'other/version' } }), expect.any(AbortSignal)))
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage(query.offset === 0 ? [installed] : []), scope: query.scope, next_offset: query.offset === 0 ? 20 : null }))
  await user.click(within(outcome).getByRole('button', { name: 'Inspect captured target' })); await screen.findByText('Target inspection')
  const reads = vi.mocked(hubNative.installed).mock.calls.slice(-2)
  expect(reads.map(([query]) => query.offset)).toEqual([0, 20]); for (const [query] of reads) expect(query).toEqual({ scope: updateInput().scope, binding: 'kiro-v3', offset: query.offset, limit: 20 })
  expect(hubNative.updateAll).toHaveBeenCalledOnce(); expect(screen.getByRole('button', { name: 'Update all remote installations' })).toBeEnabled(); expect(outcome).toHaveTextContent('not_attempted')
})
it('retains413 with no item list and does not infer an empty or successful collective result', async () => {
  vi.mocked(hubNative.updateAll).mockImplementationOnce(async input => { throw new HubUpdateAllError(413, 'remote_file_too_large', null, null, true, input) }); render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const outcome = screen.getByRole('region', { name: 'Collective update outcome' }); expect(outcome).toHaveTextContent('413'); expect(outcome).toHaveTextContent('A complete item list was not observed'); expect(outcome).toHaveTextContent('Inspection required'); expect(screen.queryByText('No eligible remote installations')).toBeNull(); expect(hubNative.updateAll).toHaveBeenCalledOnce()
})
it('does not let an older inventory inspection release a newer uncertain collective result', async () => {
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user); await screen.findByText('Remote installations updated'); await waitFor(() => expect(hubNative.installed).toHaveBeenCalledTimes(2))
  let finish!: (value: Awaited<ReturnType<typeof hubNative.installed>>) => void
  vi.mocked(hubNative.installed).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  await user.click(within(screen.getByRole('region', { name: 'Collective update outcome' })).getByRole('button', { name: 'Inspect captured target' })); const oldSignal = vi.mocked(hubNative.installed).mock.calls.at(-1)![1]
  vi.mocked(hubNative.updateAll).mockImplementationOnce(async input => { throw new HubUpdateAllError(413, 'remote_file_too_large', null, null, true, input) }); await open(user); await apply(user)
  expect(oldSignal?.aborted).toBe(true); await act(async () => finish(installedPage([]))); expect(screen.queryByText('Target inspection')).toBeNull(); expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' })).toBeDisabled(); expect(hubNative.updateAll).toHaveBeenCalledTimes(2)
})

it.each([503, 504])('shows an early HTTP%s not_attempted summary without unnecessary inspection or retry', async status => {
  vi.mocked(hubNative.updateAll).mockRestore()
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ error: { code: status === 504 ? 'deadline' : 'unavailable' }, outcome: { status: 'error', operation: 'update', scope: updateInput().scope, cache: 'not_attempted', publication: 'not_attempted' } }), { status }))
  vi.stubGlobal('fetch', fetcher)
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const outcome = await screen.findByRole('region', { name: 'Collective update outcome' })
  await waitFor(() => expect(outcome).toHaveTextContent(String(status)))
  expect(outcome).toHaveTextContent('not_attempted'); expect(outcome).toHaveTextContent('No item outcomes were reported.')
  expect(outcome).not.toHaveTextContent('Inspection required'); expect(outcome).not.toHaveTextContent('No eligible remote installations')
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' })).toBeEnabled()
  expect(fetcher).toHaveBeenCalledOnce(); expect(hubNative.installed).toHaveBeenCalledOnce()
})

it.each([413, 503])('keeps explicit inspection signals despite known not_attempted effects, HTTP%s', async status => {
  vi.mocked(hubNative.updateAll).mockRestore()
  const fetcher = vi.fn(async () => new Response(JSON.stringify({ error: { code: status === 413 ? 'remote_file_too_large' : 'unavailable' }, outcome: { status: 'error', operation: 'update', scope: updateInput().scope, cache: 'not_attempted', publication: 'not_attempted' } }), { status, headers: status === 503 ? { 'X-Graphit-Inspection-Required': 'request-selection' } : undefined }))
  vi.stubGlobal('fetch', fetcher)
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const outcome = await screen.findByRole('region', { name: 'Collective update outcome' })
  await waitFor(() => expect(outcome).toHaveTextContent('Inspection required'))
  expect(outcome).toHaveTextContent('not_attempted')
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' })).toBeDisabled()
  expect(fetcher).toHaveBeenCalledOnce()
})

it('requires inspection for generic not_attempted effects without the complete scoped producer summary', async () => {
  vi.mocked(hubNative.updateAll).mockRestore()
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: { code: 'unavailable' }, outcome: { cache: 'not_attempted', publication: 'not_attempted' } }), { status: 503 })))
  render(<ProjectArtifactsPage />); const user = userEvent.setup(); await open(user); await apply(user)
  const outcome = await screen.findByRole('region', { name: 'Collective update outcome' })
  await waitFor(() => expect(outcome).toHaveTextContent('Inspection required'))
  expect(outcome).toHaveTextContent('not_attempted'); expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update remote installations' })).toBeDisabled()
})
