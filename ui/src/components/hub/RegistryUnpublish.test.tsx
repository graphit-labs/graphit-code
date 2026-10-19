import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubUnpublishError, type HubUnpublishInput, type HubUnpublishOutcome } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import { useTaskWorkspace } from '@/components/task/taskWorkspace'
import RegistryPage from './RegistryPage'
import { catalog, hubContext, projectB, selectHubProject } from './hubTestFixtures'
vi.mock('@/api/hubNative', async importOriginal => ({ ...await importOriginal<typeof import('@/api/hubNative')>(), hubNative: { projects: vi.fn(), search: vi.fn(), installed: vi.fn(), content: vi.fn(), mutate: vi.fn(), unpublish: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.projects).mockResolvedValue({ status:'ok',projects:[],next_cursor:null })
  vi.mocked(hubNative.search).mockResolvedValue(catalog)
  vi.mocked(hubNative.unpublish).mockImplementation(async request => outcome(request))
})
afterEach(cleanup)
function outcome(request: HubUnpublishInput): HubUnpublishOutcome { return { status:'unpublished', retraction:{ producer:request.producer,artifact_type:request.artifact_type,id:request.id,targeted_versions:request.target.kind === 'version' ? [request.target.version] : ['release-1','release-2','concurrent/3'],desired_latest:null,catalog:request.target.kind === 'artifact' ? 'removed' : 'updated' } } }
async function browse(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByRole('combobox',{name:'Publisher project ID'}),projectB)
  await user.click(screen.getByRole('button',{name:'Find artifacts'}))
  await user.click(await screen.findByRole('button',{name:/Engineering reviewer/}))
}
async function open(user: ReturnType<typeof userEvent.setup>,whole=false) {
  await browse(user)
  if (!whole) { await user.click(screen.getByRole('combobox',{name:'Exact version'}));await user.click(screen.getByRole('option',{name:'release-1'})) }
  await user.click(screen.getByRole('button',{name:whole?'Unpublish artifact':'Unpublish version'}))
  return screen.getByRole('dialog')
}
async function confirm(user: ReturnType<typeof userEvent.setup>,dialog: HTMLElement,whole=false) {
  await user.click(within(dialog).getByRole('checkbox'))
  await user.click(within(dialog).getByRole('button',{name:whole?'Unpublish whole artifact':'Unpublish version'}))
}
it('requires an exact version and explicit confirmation without a local PROJECT or Agent gate',async()=>{
  act(()=>{ useTaskWorkspace.setState({selection:null,contexts:[]});useAppStore.setState({activeAgent:''}) })
  const user=userEvent.setup();render(<RegistryPage/>);await browse(user)
  expect(screen.getByRole('button',{name:'Unpublish version'})).toBeDisabled()
  expect(screen.getByRole('button',{name:'Install artifact'})).toBeDisabled()
  await user.click(screen.getByRole('combobox',{name:'Exact version'}));await user.click(screen.getByRole('option',{name:'release-1'}))
  await user.click(screen.getByRole('button',{name:'Unpublish version'}));const dialog=screen.getByRole('dialog')
  expect(within(dialog).getByRole('button',{name:'Unpublish version'})).toBeDisabled()
  expect(within(dialog).getByText(/latest becomes absent/)).toBeInTheDocument()
  await confirm(user,dialog)
  await screen.findByText('Artifact unpublished')
  expect(hubNative.unpublish).toHaveBeenCalledWith({provider:'corporate',producer:projectB,artifact_type:'skill',id:'vendor/reviewer',target:{kind:'version',version:'release-1'}},expect.any(AbortSignal))
  expect(screen.getByText('Not declared')).toBeInTheDocument()
  await waitFor(()=>expect(hubNative.search).toHaveBeenCalledTimes(2))
  await waitFor(()=>expect(screen.getByRole('region',{name:'Unpublication result'})).toHaveFocus())
  expect(hubNative.installed).not.toHaveBeenCalled()
})
it('unpublishes a whole Native artifact only through its separate all-versions confirmation',async()=>{
  vi.mocked(hubNative.search).mockResolvedValue({...catalog,artifacts:[{...catalog.artifacts[0],artifact_type:'ast'}]})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user,true)
  expect(within(dialog).getByRole('button',{name:'Unpublish whole artifact'})).toBeDisabled()
  expect(within(dialog).getByRole('checkbox',{name:/whole artifact and every published version/})).toBeInTheDocument()
  expect(within(dialog).queryByRole('textbox',{name:/SHA256/})).not.toBeInTheDocument()
  await confirm(user,dialog,true);await screen.findByText('Artifact unpublished')
  expect(hubNative.unpublish).toHaveBeenCalledWith(expect.objectContaining({artifact_type:'ast',target:{kind:'artifact'}}),expect.any(AbortSignal))
  expect(screen.getByText('release-1, release-2, concurrent/3')).toBeInTheDocument()
})
it('retains a hash draft after a rejected request and retries only explicitly',async()=>{
  vi.mocked(hubNative.unpublish).mockImplementationOnce(async request=>{throw new HubUnpublishError(400,'invalid_arguments',{retraction:{catalog:'not_attempted'}},false,request)})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user)
  await user.type(within(dialog).getByRole('textbox',{name:'Expected archive SHA256 (optional)'}),'a'.repeat(64))
  await confirm(user,dialog);await screen.findByText(/Your entered values are retained/)
  expect(within(dialog).getByRole('textbox',{name:/SHA256/})).toHaveValue('a'.repeat(64))
  expect(hubNative.unpublish).toHaveBeenCalledTimes(1)
  await user.click(within(dialog).getByRole('button',{name:'Unpublish version'}));await screen.findByText('Artifact unpublished')
  expect(hubNative.unpublish).toHaveBeenLastCalledWith(expect.objectContaining({target:{kind:'version',version:'release-1',expected_sha256:'a'.repeat(64)}}),expect.any(AbortSignal))
})
it('keeps an original request/result after PROJECT/Agent changes and closing the inspector; never reloads a different remote tuple',async()=>{
  let finish!:(value:HubUnpublishOutcome)=>void
  vi.mocked(hubNative.unpublish).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  act(()=>{selectHubProject(projectB,'other/data','/b');useAppStore.setState({activeAgent:'kimi'})})
  expect(screen.getByRole('dialog')).toBeInTheDocument()
  await user.click(within(dialog).getByRole('button',{name:'Close'}))
  await user.click(await screen.findByRole('button',{name:'Close details'}))
  const calls=vi.mocked(hubNative.search).mock.calls.length,request=vi.mocked(hubNative.unpublish).mock.calls[0][0]
  await act(async()=>finish(outcome(request)));await screen.findByText('Artifact unpublished')
  expect(hubNative.search).toHaveBeenCalledTimes(calls)
  expect(screen.getAllByText('release-1').length).toBeGreaterThan(0)
  expect(hubNative.unpublish).toHaveBeenCalledTimes(1)
})
it.each(['identity','api','logout'] as const)('aborts and discards private pending results after %s change',async(change)=>{
  let finish!:(value:HubUnpublishOutcome)=>void
  vi.mocked(hubNative.unpublish).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  const [request,signal]=vi.mocked(hubNative.unpublish).mock.calls[0]
  act(()=>useAppStore.setState(change==='identity'?{webIdentity:{state:'authenticated',provider:'corporate',subject:'second'}}:change==='api'?{apiBase:'https://new.example.test/api'}:{webIdentity:{state:'anonymous'}}))
  await waitFor(()=>expect(signal?.aborted).toBe(true));await act(async()=>finish(outcome(request)))
  expect(screen.queryByText('Artifact unpublished')).not.toBeInTheDocument()
  expect(screen.queryByText('Unpublication result')).not.toBeInTheDocument()
})
it('retains post-effect HTTP failure receipt and requires inspection for uncertain413 before another mutation',async()=>{
  vi.mocked(hubNative.unpublish).mockImplementationOnce(async request=>{throw new HubUnpublishError(413,'hub_unpublish_too_large',{retraction:{...outcome(request).retraction,catalog:'uncertain'},stage:'catalog_update',cause:'deadline'},true,request)})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  await screen.findByText('uncertain');expect(within(dialog).getByRole('button',{name:'Unpublish version'})).toBeDisabled()
  await user.click(within(dialog).getByRole('button',{name:'Cancel'}))
  await user.click(screen.getByRole('button',{name:'Inspect original catalogue'}))
  await screen.findByText(/Original catalogue versions: release-1, release-2. Declared latest: not declared/)
  expect(hubNative.search).toHaveBeenLastCalledWith({transport:'broker',provider:'corporate',producer:projectB,artifact_type:'skill',id:'vendor/reviewer',limit:20},expect.any(AbortSignal))
  expect(hubNative.installed).not.toHaveBeenCalled()
  expect(hubNative.unpublish).toHaveBeenCalledTimes(1)
  expect(screen.getByRole('button',{name:'Unpublish version'})).toBeEnabled()
})
it('closes an incompatible remote-source dialog permanently and disables Standalone unpublication',async()=>{
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user,true)
  await user.keyboard('{Escape}');expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(dialog).not.toBeInTheDocument()
  expect(screen.getByRole('button',{name:'Unpublish artifact'})).toHaveFocus()
  await user.click(screen.getByRole('combobox',{name:'Source'}));await user.click(screen.getByRole('option',{name:'Standalone storage'}))
  await user.click(screen.getByRole('button',{name:'Find artifacts'}))
  await user.click(await screen.findByRole('button',{name:/Engineering reviewer/}))
  expect(screen.getByRole('button',{name:'Unpublish artifact'})).toBeDisabled()
  expect(hubNative.unpublish).not.toHaveBeenCalled()
})
it('cancels the original pending operation and preserves an unobserved result for catalogue inspection',async()=>{
  vi.mocked(hubNative.unpublish).mockImplementationOnce((_request,signal)=>new Promise((_resolve,reject)=>signal?.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')))))
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  await user.click(within(dialog).getByRole('button',{name:'Cancel unpublication'}))
  await screen.findByText(/result could not be observed/)
  expect(vi.mocked(hubNative.unpublish).mock.calls[0][1]?.aborted).toBe(true)
  expect(hubNative.unpublish).toHaveBeenCalledTimes(1)
  expect(within(dialog).getByRole('button',{name:'Unpublish version'})).toBeDisabled()
})

it('does not let an earlier inspection unlock a later uncertain result',async()=>{
  const user=userEvent.setup();render(<RegistryPage/>);const first=await open(user);await confirm(user,first);await screen.findByText('Artifact unpublished')
  let finish!:(value:typeof catalog)=>void
  vi.mocked(hubNative.search).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  await user.click(screen.getByRole('button',{name:'Inspect original catalogue'}))
  const oldSignal=vi.mocked(hubNative.search).mock.calls.at(-1)?.[1]
  vi.mocked(hubNative.unpublish).mockImplementationOnce(async request=>{throw new HubUnpublishError(413,'hub_unpublish_too_large',null,true,request)})
  await user.click(screen.getByRole('button',{name:'Unpublish artifact'}))
  const second=screen.getByRole('dialog');await confirm(user,second,true);await screen.findByText(/full result exceeded/)
  expect(oldSignal?.aborted).toBe(true)
  await act(async()=>finish(catalog))
  expect(within(second).getByRole('button',{name:'Unpublish whole artifact'})).toBeDisabled()
  expect(screen.queryByText(/Original catalogue versions:/)).not.toBeInTheDocument()
  expect(hubNative.unpublish).toHaveBeenCalledTimes(2)
})
it('preserves post-effect401 as a failure with a confirmed catalogue receipt',async()=>{
  vi.mocked(hubNative.unpublish).mockImplementationOnce(async request=>{throw new HubUnpublishError(401,'unauthorized',outcome(request),false,request)})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  await screen.findByText('Unpublication not completed')
  expect(screen.getByText('updated')).toBeInTheDocument()
  expect(screen.queryByText('Artifact unpublished')).not.toBeInTheDocument()
  expect(screen.getByText(/Recorded catalogue effects still apply/)).toBeInTheDocument()
})

it('keeps a crossed error receipt without displaying its effects as the original target',async()=>{
  vi.mocked(hubNative.unpublish).mockImplementationOnce(async request=>{throw new HubUnpublishError(503,'unavailable',{retraction:{...outcome(request).retraction,id:'another/private-target',targeted_versions:['another/private-version']}},true,request,false)})
  const user=userEvent.setup();render(<RegistryPage/>);const dialog=await open(user);await confirm(user,dialog)
  await screen.findByText(/returned receipt does not match/)
  expect(screen.queryByText('another/private-version')).not.toBeInTheDocument()
  expect(screen.queryByText('updated')).not.toBeInTheDocument()
  expect(within(dialog).getByRole('button',{name:'Unpublish version'})).toBeDisabled()
})
