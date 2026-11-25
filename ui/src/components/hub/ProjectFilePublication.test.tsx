import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubPublicationError, type FilePublicationInput, type FilePublicationOutcome } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import ProjectArtifactsPage from './ProjectArtifactsPage'
import { catalog, hubContext, selectHubProject, installedPage, projectA, projectB } from './hubTestFixtures'
vi.mock('@/api/hubNative', async original => ({ ...await original<typeof import('@/api/hubNative')>(), hubNative: { authoredList: vi.fn(), authoredReview: vi.fn(), publish: vi.fn(), search: vi.fn(), installed: vi.fn(), content: vi.fn(), mutate: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
const outcome = (input: FilePublicationInput): FilePublicationOutcome => ({ status:'published',project_metadata:'published',publication:{producer:input.project_id,artifact_type:input.publication.artifact_type,id:input.publication.id,version:input.publication.version,archive_sha256:'a'.repeat(64),payload:'published',catalog:'published'} })
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.authoredList).mockImplementation(async query => ({ status: 'ok', project_id: query.project_id, project_version: query.project_version, entries: [{ artifact_type: 'skill', id: 'vendor/reviewer', version: 'release/one', name: null }], next_cursor: null }))
  vi.mocked(hubNative.authoredReview).mockImplementation(async query => ({ ...query, status: 'ok', source: 'Resources', metadata: null, entries: [{ path: 'SKILL.md', kind: 'File', bytes: 10 }], next_offset: null }))
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([]),scope:query.scope }))
  vi.mocked(hubNative.publish).mockImplementation(async input => outcome(input))
  vi.mocked(hubNative.search).mockResolvedValue({ ...catalog, producer:projectA })
})
afterEach(cleanup)
async function draft(user: ReturnType<typeof userEvent.setup>) {
  if (!screen.queryByRole('button', { name: 'Find project resources' })) await user.click(screen.getByRole('button', { name: 'Browse project resources' }))
  await user.click(await screen.findByRole('button', { name: /vendor\/reviewer/ }))
  await user.click(await screen.findByRole('button', { name: 'Publish project resources' }))
  await user.type(screen.getByRole('textbox', { name: 'Publication name' }), 'Reviewer')
}
async function submit(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button',{name:'Review publication'}))
  await user.click(screen.getByRole('button',{name:'Publish artifact'}))
}
it('reviews all actual metadata and dependencies, preserves literal versions and uses ModalPortal keyboard/focus', async () => {
  const user=userEvent.setup();render(<ProjectArtifactsPage />)
  await draft(user); const open=screen.getByRole('button',{name:'Publish project resources'})
  await user.click(screen.getByText('Optional publication metadata'))
  await user.type(screen.getByRole('textbox',{name:'Description'}),'Resource description')
  await user.type(screen.getByRole('textbox',{name:'Tags (one per line)'}),'rust\nreview')
  await user.selectOptions(screen.getByRole('combobox',{name:'Dependency handling'}),'replace')
  await user.click(screen.getByRole('button',{name:'Add dependency'}))
  await user.type(screen.getByRole('textbox',{name:'Dependency artifact ID'}),'tools/helper')
  await user.type(screen.getByRole('textbox',{name:'Dependency version selector'}),'release/helper')
  await user.click(screen.getByRole('checkbox',{name:'Set latest to this concrete version'}))
  await user.click(screen.getByRole('button',{name:'Review publication'}))
  expect(hubNative.publish).not.toHaveBeenCalled()
  expect(screen.getByRole('dialog')).toHaveTextContent('tools/helper @ release/helper')
  await user.click(screen.getByRole('button',{name:'Edit metadata'}))
  expect(screen.getByRole('textbox',{name:'Publication name'})).toHaveValue('Reviewer')
  await submit(user)
  await screen.findByText('Publication confirmed')
  expect(hubNative.publish).toHaveBeenCalledWith({project_id:projectA,project_version:'feature/a',publication:{provider:'corporate',artifact_type:'skill',id:'vendor/reviewer',version:'release/one',name:'Reviewer',description:'Resource description',tags:['rust','review'],dependencies:[{artifact_type:'skill',id:'tools/helper',selector:'release/helper'}],set_latest:true}},expect.any(AbortSignal))
  await user.click(open);await user.keyboard('{Escape}')
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(open).toHaveFocus()
})
it('retains the submitted original tuple after PROJECT/version/binding/cache switches without reloading the new inventory', async () => {
  let finish!: (value: FilePublicationOutcome)=>void
  vi.mocked(hubNative.publish).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user)
  const [captured,signal]=vi.mocked(hubNative.publish).mock.calls[0]
  act(()=>{selectHubProject(projectB,'feature/b','/b');useAppStore.setState({activeAgent:'opencode'})})
  await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  await user.click(screen.getByRole('combobox',{name:'Inspect installations in'}));await user.click(screen.getByRole('option',{name:'Installation cache'}))
  const count=vi.mocked(hubNative.installed).mock.calls.length
  await act(async()=>finish(outcome(captured)))
  const region=screen.getByRole('region',{name:'Publication outcome'})
  expect(region).toHaveTextContent(projectA+' @ feature/a');expect(region).toHaveTextContent('release/one')
  expect(signal?.aborted).toBe(false);expect(hubNative.publish).toHaveBeenCalledTimes(1)
  expect(hubNative.installed).toHaveBeenCalledTimes(count)
})
it.each(['identity','logout','apiBase'] as const)('aborts publication and discards private results after %s',async boundary=>{
  let finish!: (value: FilePublicationOutcome)=>void
  vi.mocked(hubNative.publish).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user)
  const [captured,signal]=vi.mocked(hubNative.publish).mock.calls[0]
  act(()=>useAppStore.setState(boundary==='logout'?{webIdentity:{state:'anonymous'}}:boundary==='apiBase'?{apiBase:'/another-api'}:{webIdentity:{state:'authenticated',provider:'corporate',subject:'another-user'}}))
  await waitFor(()=>expect(signal?.aborted).toBe(true))
  await act(async()=>finish(outcome(captured)))
  expect(screen.queryByRole('region',{name:'Publication outcome'})).not.toBeInTheDocument()
})
it('keeps failed drafts, partial effects and replacement intent;413 inspects only the original remote catalogue before explicit retry', async()=>{
  vi.mocked(hubNative.publish).mockImplementationOnce(async input=>{throw new HubPublicationError(413,null,{project_metadata:'published',publication:{payload:'published',catalog:'uncertain'},stage:'catalog_write'},true,input)})
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user)
  expect(screen.getByRole('dialog')).toHaveTextContent('Publication needs review')
  expect(screen.getByRole('dialog')).toHaveTextContent('uncertain')
  await user.click(screen.getByRole('button',{name:'Edit metadata'}))
  expect(screen.getByRole('dialog')).toHaveTextContent('release/one')
  expect(screen.getByRole('button',{name:'Review publication'})).toBeDisabled()
  act(()=>{selectHubProject(projectA,'feature/b')})
  await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  await user.click(screen.getByRole('button',{name:'Inspect original publication'}))
  await screen.findByText('Publication catalogue inspected')
  expect(hubNative.search).toHaveBeenCalledWith({transport:'broker',provider:'corporate',producer:projectA,artifact_type:'skill',id:'vendor/reviewer',version:'release/one',limit:20},expect.any(AbortSignal))
  expect(hubNative.publish).toHaveBeenCalledTimes(1)
  await draft(user);await submit(user);await screen.findByText('Publication confirmed')
  expect(hubNative.publish).toHaveBeenLastCalledWith(expect.objectContaining({project_version:'feature/b'}),expect.any(AbortSignal))
})
it('does not let a previous catalogue inspection release a later uncertain result',async()=>{
  let oldRead!: (value: Awaited<ReturnType<typeof hubNative.search>>)=>void
  vi.mocked(hubNative.search).mockImplementationOnce(()=>new Promise(resolve=>{oldRead=resolve}))
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user);await screen.findByText('Publication confirmed')
  await user.click(screen.getByRole('button',{name:'Inspect original publication'}))
  vi.mocked(hubNative.publish).mockImplementationOnce(async input=>{throw new HubPublicationError(413,null,null,true,input)})
  await draft(user);await submit(user)
  const signal=vi.mocked(hubNative.search).mock.calls[0][1];expect(signal?.aborted).toBe(true)
  await act(async()=>oldRead({...catalog,producer:projectA}))
  expect(screen.queryByText('Publication catalogue inspected')).not.toBeInTheDocument()
  expect(within(screen.getByRole('dialog')).getByRole('button',{name:'Publish artifact'})).toBeDisabled()
  expect(hubNative.publish).toHaveBeenCalledTimes(2)
})

it.each([{project_metadata:'uncertain'}, {project_metadata:'published',owned_temporary_residue:'owned/staging'}])('requires original catalogue inspection for metadata uncertainty or owned cleanup: %j',async effects=>{
  vi.mocked(hubNative.publish).mockImplementationOnce(async input=>{throw new HubPublicationError(504,'deadline',effects,false,input)})
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user)
  expect(within(screen.getByRole('dialog')).getByRole('button',{name:'Publish artifact'})).toBeDisabled()
  expect(hubNative.publish).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button',{name:'Inspect original publication'}));await screen.findByText('Publication catalogue inspected')
  expect(within(screen.getByRole('dialog')).getByRole('button',{name:'Publish artifact'})).toBeEnabled()
})
it('keeps a validation failure draft available for explicit correction without automatic retry',async()=>{
  vi.mocked(hubNative.publish).mockImplementationOnce(async input=>{throw new HubPublicationError(400,'invalid_arguments',null,false,input)})
  const user=userEvent.setup();render(<ProjectArtifactsPage />);await draft(user);await submit(user)
  expect(hubNative.publish).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button',{name:'Edit metadata'}))
  expect(screen.getByRole('dialog')).toHaveTextContent('vendor/reviewer')
  expect(screen.getByRole('dialog')).toHaveTextContent('release/one')
  expect(screen.getByRole('button',{name:'Review publication'})).toBeEnabled()
})
