import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubRequestError, type FileOutcome } from '@/api/hubNative'
import { projectVersionsApi } from '@/api/projectVersions'
import { useAppStore } from '@/store/appStore'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import ProjectArtifactsPage from './ProjectArtifactsPage'
import { completed, hubContext, selectHubProject, installed, installedPage, projectA, projectB } from './hubTestFixtures'
vi.mock('@/api/hubNative', async importOriginal => ({ ...await importOriginal<typeof import('@/api/hubNative')>(), hubNative: { projects: vi.fn(), search: vi.fn(), installed: vi.fn(), content: vi.fn(), mutate: vi.fn() } }))
vi.mock('@/api/projectVersions', () => ({ projectVersionsApi: { list: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage(query.scope.kind === 'global_cache' ? [{ ...installed, target: { kind: 'global_cache' } }] : [installed]), scope: query.scope }))
  vi.mocked(hubNative.mutate).mockImplementation(async (operation, input) => completed(operation, input))
})
afterEach(cleanup)
async function inspect(user: ReturnType<typeof userEvent.setup>) { await user.click(await screen.findByRole('button', { name: /Installed reviewer/ })) }

it.each([
  ['remove', 'Remove artifact', 'Remove artifact', 'Physical(Removed)'],
  ['recover', 'Recover target', 'Recover artifact', 'Recovered'],
  ['forget', 'Forget logical ownership', 'Forget artifact', 'Forgotten'],
] as const)('local lifecycle %s consumes origin:null without remote reads or source authority', async (operation, action, confirmation, change) => {
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([{ ...installed, source_origin: 'LocalLink', origin: null }]), scope: query.scope }))
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (selectedOperation, request) => ({
    ...completed(selectedOperation, request), producer: installed.producer, binding: request.binding,
    publication: { change, cleanup_pending: false },
    possible_parents: ['/project/.agents'],
    forget_preview: null,
  }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  expect(screen.getByText('LocalLink')).toBeInTheDocument()
  expect(screen.getByText(installed.alias!)).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Update artifact' })).not.toBeInTheDocument()
  expect(screen.queryByRole('textbox', { name: 'Relative file path' })).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: action }))
  const dialog = screen.getByRole('dialog')
  expect(within(dialog).getByText(projectA + ' @ feature/a')).toBeInTheDocument()
  expect(within(dialog).getByText('Kiro V3')).toBeInTheDocument()
  if (operation === 'forget') expect(within(dialog).getByText(/Physical files and residues remain/)).toBeInTheDocument()
  await user.click(within(dialog).getByRole('button', { name: confirmation }))
  await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledOnce()
  expect(hubNative.mutate).toHaveBeenCalledWith(operation, {
    scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' },
    artifact_type: installed.artifact_type, id: installed.id, version: installed.version, binding: 'kiro-v3',
  }, expect.any(AbortSignal))
  expect(screen.getByText(change)).toBeInTheDocument()
  expect(screen.getByText('/project/.agents')).toBeInTheDocument()
  expect(hubNative.content).not.toHaveBeenCalled()
  expect(hubNative.search).not.toHaveBeenCalled()
})

it('local lifecycle keeps a submitted local-source removal on its captured PROJECT after a header change', async () => {
  let finish!: (value: FileOutcome) => void
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([{ ...installed, source_origin: 'LocalLink', origin: null }]), scope: query.scope }))
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Remove artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove artifact' }))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  act(() => { selectHubProject(projectB, 'feature/b', '/b'); useAppStore.setState({ activeAgent: 'opencode' }) })
  await waitFor(() => expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ scope: { kind: 'project', project_id: projectB, project_version: 'feature/b' }, binding: 'opencode' }), expect.any(AbortSignal)))
  await screen.findByRole('button', { name: /Installed reviewer/ })
  const reads = vi.mocked(hubNative.installed).mock.calls.length
  await act(async () => finish({ ...completed('remove', captured), publication: { change: 'Physical(Removed)', cleanup_pending: false } }))
  expect(hubNative.installed).toHaveBeenCalledTimes(reads)
  expect(signal?.aborted).toBe(false)
  expect(hubNative.mutate).toHaveBeenCalledOnce()
  expect(captured).toEqual({ scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' }, artifact_type: installed.artifact_type, id: installed.id, version: installed.version, binding: 'kiro-v3' })
  const outcome = screen.getByRole('region', { name: 'Operation outcome' })
  expect(within(outcome).getByText('Operation completed')).toBeInTheDocument()
  expect(within(outcome).getByText(projectA + ' @ feature/a')).toBeInTheDocument()
  expect(within(outcome).getByText('Kiro V3')).toBeInTheDocument()
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(hubNative.content).not.toHaveBeenCalled()
})

it('uses concrete PROJECT/version/binding and supports keyboard detail focus and Escape restoration', async () => {
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  const row = await screen.findByRole('button', { name: /Installed reviewer/ })
  row.focus(); await user.keyboard('{Enter}')
  expect(screen.getByRole('article', { name: 'Artifact details' })).toHaveFocus()
  expect(hubNative.installed).toHaveBeenCalledWith({ scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' }, binding: 'kiro-v3', offset: 0, limit: 20 }, expect.any(AbortSignal))
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('article', { name: 'Artifact details' })).not.toBeInTheDocument()
  expect(row).toHaveFocus()
})

it('retains an owned mutation and its original receipt across PROJECT/version/binding changes', async () => {
  let resolve!: (value: FileOutcome) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(result => { resolve = result }))
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage(), scope: query.scope, entries: [{ ...installed, display: { ...installed.display!, name: query.scope.kind === 'project' && query.scope.project_id === projectB ? 'Current installation' : 'Installed reviewer' } }] }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Update artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update artifact' }))
  await waitFor(() => expect(hubNative.mutate).toHaveBeenCalledTimes(1))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  act(() => { selectHubProject(projectB, 'feature/b', '/b'); useAppStore.setState({ activeAgent: 'opencode' }) })
  await screen.findByRole('button', { name: /Current installation/ })
  const reads = vi.mocked(hubNative.installed).mock.calls.length
  await act(async () => resolve(completed('update', captured)))
  expect(signal?.aborted).toBe(false)
  expect(captured.scope).toEqual({ kind: 'project', project_id: projectA, project_version: 'feature/a' })
  expect(hubNative.installed).toHaveBeenCalledTimes(reads)
  const outcome = screen.getByRole('region', { name: 'Operation outcome' })
  expect(within(outcome).getByText('Operation completed')).toBeInTheDocument()
  expect(within(outcome).getByText(projectA + ' @ feature/a')).toBeInTheDocument()
  expect(within(outcome).getByText('Kiro V3')).toBeInTheDocument()
  expect(within(outcome).getByText(projectB)).toBeInTheDocument()
  expect(within(outcome).getByText('broker / corporate')).toBeInTheDocument()
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

it('retains the original PROJECT target after switching to GlobalCache and inspects it before another operation', async () => {
  let finish!: (error: unknown) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise((_resolve, reject) => { finish = reject }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Remove artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove artifact' }))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }))
  await user.click(screen.getByRole('combobox', { name: 'Inspect installations in' }))
  await user.click(screen.getByRole('option', { name: 'Installation cache' }))
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  expect(signal?.aborted).toBe(false)
  expect(within(screen.getByRole('region', { name: 'Operation outcome' })).getByText(projectA + ' @ feature/a')).toBeInTheDocument()
  await act(async () => finish(new HubRequestError(413, null, null, true, { publication: { committed: true, hub_pending: true } }, captured)))
  await screen.findByText('Inspection required')
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button', { name: /Installed reviewer/ }))
  expect(screen.getByRole('button', { name: 'Remove artifact' })).toBeDisabled()
  await user.click(screen.getByRole('button', { name: 'Inspect captured target' }))
  await screen.findByText('Captured target inspected')
  expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ scope: captured.scope, binding: 'kiro-v3', id: installed.id, version: 'release-1' }), expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button', { name: 'Remove artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove artifact' }))
  await waitFor(() => expect(hubNative.mutate).toHaveBeenCalledTimes(2))
  const [, next] = vi.mocked(hubNative.mutate).mock.calls[1]
  expect(next.scope).toEqual({ kind: 'global_cache' })
  expect(next.binding).toBeUndefined()
})

it.each(['identity', 'apiBase', 'logout'] as const)('aborts owned mutation and discards its result on %s change', async boundary => {
  let finish!: (value: FileOutcome) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Update artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update artifact' }))
  const [, captured, signal] = vi.mocked(hubNative.mutate).mock.calls[0]
  act(() => useAppStore.setState(boundary === 'apiBase' ? { apiBase: '/other-instance' }
    : boundary === 'logout' ? { webIdentity: { state: 'anonymous' } }
      : { webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'another-user' } }))
  await waitFor(() => expect(signal?.aborted).toBe(true))
  await act(async () => finish(completed('update', captured)))
  expect(screen.queryByText('Operation completed')).not.toBeInTheDocument()
  expect(screen.queryByRole('region', { name: 'Operation outcome' })).not.toBeInTheDocument()
})

it('retains partial effects after413, inspects the original target and never automatically repeats a mutation', async () => {
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (_operation, request) => { throw new HubRequestError(413, null, null, true, { publication: { committed: true, hub_pending: true } }, structuredClone(request)) })
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage(query.id ? [] : [installed]), scope: query.scope }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Remove artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove artifact' }))
  await screen.findByText(/Inspect the captured target in the operation outcome/)
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }))
  expect(screen.getByText('Inspection required')).toBeInTheDocument()
  expect(screen.getByText('Confirmed')).toBeInTheDocument()
  expect(screen.queryByText('Operation completed')).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Recover target' })).toBeDisabled()
  await user.click(screen.getByRole('button', { name: 'Inspect captured target' }))
  await screen.findByText(/The captured selection is not listed/)
  expect(hubNative.installed).toHaveBeenLastCalledWith({ scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' }, artifact_type: 'skill', id: installed.id, version: 'release-1', binding: 'kiro-v3', offset: 0, limit: 20 }, expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button', { name: 'Recover target' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Recover artifact' }))
  await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenLastCalledWith('recover', expect.objectContaining({ id: installed.id, version: 'release-1', binding: 'kiro-v3' }), expect.any(AbortSignal))
})

it.each([403,500])('keeps an HTTP%s failure distinct from an authorized empty inventory', async status => {
  vi.mocked(hubNative.installed).mockRejectedValueOnce(new HubRequestError(status, status === 403 ? 'permission_denied' : 'file_read_unavailable', null, false, null, {}))
  render(<ProjectArtifactsPage />)
  await screen.findByText(status === 403 ? 'Access unavailable' : 'Hub request failed')
  expect(screen.queryByText('No installed FILE artifacts')).not.toBeInTheDocument()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it('distinguishes installation-empty from filtered-empty using the actual installed query', async () => {
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([]), scope: query.scope }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  await screen.findByText('No installed FILE artifacts')
  await user.type(screen.getByRole('textbox', { name: 'Exact artifact ID (optional)' }), 'missing/library')
  await user.click(screen.getByRole('button', { name: 'Apply filters' }))
  await screen.findByText('No installations match these filters')
})

it('waits for an authorized Hub selection and never resolves a legacy PROJECT version', async () => {
  useAppStore.setState({ activeProjectVersion: 'untrusted/legacy' })
  useTaskWorkspace.setState({ contexts: [], selection: null })
  render(<ProjectArtifactsPage />)
  expect(hubNative.installed).not.toHaveBeenCalled()
  expect(screen.queryByRole('button', { name: 'Browse project resources' })).not.toBeInTheDocument()
  act(() => { selectHubProject(projectB, 'authorized/branch', '/b') })
  await screen.findByRole('button', { name: /Installed reviewer/ })
  expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ scope: { kind: 'project', project_id: projectB, project_version: 'authorized/branch' } }), expect.any(AbortSignal))
  expect(projectVersionsApi.list).not.toHaveBeenCalled()
})

it('does not let inspection of an earlier result release a later uncertain mutation', async () => {
  let finishInspection!: (value: Awaited<ReturnType<typeof hubNative.installed>>) => void
  vi.mocked(hubNative.installed).mockImplementation(async query => query.id
    ? new Promise(resolve => { finishInspection = resolve })
    : { ...installedPage(), scope: query.scope })
  vi.mocked(hubNative.mutate).mockImplementationOnce(async (operation, request) => completed(operation, request))
    .mockImplementationOnce(async (_operation, request) => { throw new HubRequestError(413, null, null, true, { publication: { committed: null } }, request) })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Update artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update artifact' }))
  await screen.findByText('Operation completed')
  await user.click(screen.getByRole('button', { name: 'Inspect captured target' }))
  await waitFor(() => expect(finishInspection).toBeDefined())
  const inspectionSignal = vi.mocked(hubNative.installed).mock.calls.find(([query]) => query.id)?.[1]
  await user.click(await screen.findByRole('button', { name: 'Recover target' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Recover artifact' }))
  await screen.findByText('Inspection required')
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }))
  await act(async () => finishInspection(installedPage()))
  expect(inspectionSignal?.aborted).toBe(true)
  expect(screen.getByText('Inspection required')).toBeInTheDocument()
  expect(screen.queryByText('Captured target inspected')).not.toBeInTheDocument()
  expect(screen.queryByText(/The captured selection is listed/)).not.toBeInTheDocument()
  expect(screen.getByRole('button', { name: 'Recover target' })).toBeDisabled()
  expect(hubNative.mutate).toHaveBeenCalledTimes(2)
})

it('treats a confirmed GlobalCache no-op as complete without inventing an uncertain effect', async () => {
  vi.mocked(hubNative.mutate).mockImplementation(async (operation, request) => ({ ...completed(operation, request), publication: { change: 'VerifiedUnchanged', committed: false }, payload_preserved: true }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  await user.click(screen.getByRole('combobox', { name: 'Inspect installations in' }))
  await user.click(screen.getByRole('option', { name: 'Installation cache' }))
  await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Remove artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove artifact' }))
  await screen.findByText('Operation completed')
  expect(screen.queryByText('Inspection required')).not.toBeInTheDocument()
  expect(screen.getByText('Not committed')).toBeInTheDocument()
  expect(hubNative.mutate).toHaveBeenCalledWith('remove', expect.objectContaining({ scope: { kind: 'global_cache' }, version: 'release-1' }), expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
})

it('refreshes the current inventory filters when an earlier mutation completes', async () => {
  let finish!: (value: FileOutcome) => void
  vi.mocked(hubNative.mutate).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Update artifact' }))
  const dialog = screen.getByRole('dialog')
  await user.click(within(dialog).getByRole('button', { name: 'Update artifact' }))
  const [, captured] = vi.mocked(hubNative.mutate).mock.calls[0]
  await user.click(within(dialog).getByRole('button', { name: 'Close' }))
  await user.type(screen.getByRole('textbox', { name: 'Exact artifact ID (optional)' }), 'current/filter')
  await user.click(screen.getByRole('button', { name: 'Apply filters' }))
  await waitFor(() => expect(hubNative.installed).toHaveBeenCalledTimes(2))
  await act(async () => finish(completed('update', captured)))
  await waitFor(() => expect(hubNative.installed).toHaveBeenCalledTimes(3))
  expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'current/filter' }), expect.any(AbortSignal))
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
  expect(captured).toMatchObject({ selection: { id: 'vendor/reviewer', version: 'release-1' } })
  expect(screen.getByText('Operation completed')).toBeInTheDocument()
})

async function chooseUpdateMode(user:ReturnType<typeof userEvent.setup>,name:string){
  await user.click(within(screen.getByRole('dialog')).getByRole('combobox',{name:'Installation mode'}))
  await user.click(screen.getByRole('option',{name}))
}
it('physical mode update preserves mode and existing names by actual omission',async()=>{
  const user=userEvent.setup();render(<ProjectArtifactsPage/>);await inspect(user)
  await user.click(screen.getByRole('button',{name:'Update artifact'}));const dialog=screen.getByRole('dialog')
  expect(within(dialog).getByRole('combobox',{name:'Installation mode'})).toHaveTextContent('Default (Copy)')
  await user.click(within(dialog).getByRole('button',{name:'Update artifact'}));await screen.findByText('Operation completed')
  const request=vi.mocked(hubNative.mutate).mock.calls[0][1]
  for(const field of ['mode','alias','entry_name'])expect(request).not.toHaveProperty(field)
  expect(within(screen.getByRole('region',{name:'Operation outcome'})).getByText('Default Copy (omitted)')).toBeInTheDocument()
})
it.each([['Copy','copy'],['Link to Hub cache','link']] as const)('physical mode update sends explicit %s with typed alias and entry name',async(label,mode)=>{
  if (mode === 'link') vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([{ ...installed, artifact_type: 'rule' }]), scope: query.scope }))
  const user=userEvent.setup();render(<ProjectArtifactsPage/>);await inspect(user)
  await user.click(screen.getByRole('button',{name:'Update artifact'}));const dialog=screen.getByRole('dialog')
  await chooseUpdateMode(user,label)
  await user.type(within(dialog).getByRole('textbox',{name:'Local alias (optional)'}),'team-review')
  await user.type(within(dialog).getByRole('textbox',{name:'Entry name (optional)'}),'review.md')
  await user.click(within(dialog).getByRole('button',{name:'Update artifact'}));await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledWith('update',expect.objectContaining({mode,alias:'team-review',entry_name:'review.md',selection:expect.objectContaining({transport:'broker',producer:projectB,version:'release-1'})}),expect.any(AbortSignal))
})
it('physical mode Rule Link stays attached to its original owner across PROJECT changes',async()=>{
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([{ ...installed, artifact_type: 'rule' }]), scope: query.scope }))
  let finish!:(value:FileOutcome)=>void
  vi.mocked(hubNative.mutate).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  const user=userEvent.setup();render(<ProjectArtifactsPage/>);await inspect(user)
  await user.click(screen.getByRole('button',{name:'Update artifact'}));const dialog=screen.getByRole('dialog')
  await chooseUpdateMode(user,'Link to Hub cache')
  await user.click(within(dialog).getByRole('button',{name:'Update artifact'}))
  const [,captured,signal]=vi.mocked(hubNative.mutate).mock.calls[0]
  act(()=>{selectHubProject(projectB,'feature/b','/b');useAppStore.setState({activeAgent:'kimi'})})
  await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  const reads=vi.mocked(hubNative.installed).mock.calls.length
  await act(async()=>finish(completed('update',captured)))
  await screen.findByText('Operation completed')
  expect(captured).toMatchObject({mode:'link',scope:{kind:'project',project_id:projectA,project_version:'feature/a'}})
  expect(signal?.aborted).toBe(false)
  expect(hubNative.installed).toHaveBeenCalledTimes(reads)
  expect(within(screen.getByRole('region',{name:'Operation outcome'})).getByText('Link to Hub cache')).toBeInTheDocument()
  expect(hubNative.mutate).toHaveBeenCalledTimes(1)
})
