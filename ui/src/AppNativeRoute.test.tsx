import '@testing-library/jest-dom/vitest'
import '@/test/contextControls'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { hubNative } from '@/api/hubNative'
import { catalog, installedPage, projectA, projectB } from '@/components/hub/hubTestFixtures'
import { taskApi } from '@/api/task'
import * as nativeRead from '@/api/nativeRecordRead'
import { context } from '@/components/task/task.test-fixtures'
import { stopTaskWorkspace } from '@/components/task/taskWorkspace'
import { useAppStore } from '@/store/appStore'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import App from './App'

const readers = vi.hoisted(() => ({ list: vi.fn(async () => {}), detail: vi.fn(async () => {}) }))
function Reader() {
  const [draft, setDraft] = useState('Keep my USER draft')
  usePageRefresh(readers.list); usePageRefresh(readers.detail)
  return <input aria-label="USER draft" value={draft} onChange={event => setDraft(event.target.value)} />
}
vi.mock('./components/system/WorkspacePage', () => ({ default: Reader }))
vi.mock('./components/hub/InstalledLibrariesPage', () => ({ default: Reader }))
vi.mock('./components/memory/MemoryExplorerPage', () => ({ default: Reader }))
vi.mock('./components/wiki/WikiExplorerPage', () => ({ default: Reader }))
vi.mock('./components/wiki/WikiContextsPage', () => ({ default: Reader }))
vi.mock('./components/layout/Sidebar', () => ({ Sidebar: () => null, MobileSidebar: () => null }))
vi.mock('./components/layout/AuthIdentity', () => ({ AuthIdentity: () => <span>Signed-in identity</span> }))
beforeEach(() => {
  vi.clearAllMocks()
  stopTaskWorkspace()
  vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
  window.history.replaceState(null, '', '/memory/explorer/user/memory1?scope_id=literal-user-scope')
  useAppStore.setState({ activeProjectDir: '/project', activeProjectVersion: 'project-version', activeAgent: 'codex', projectsLoaded: true, projectsError: '', projects: [], supportedAgents: [], loadProjects: vi.fn(async () => {}) })
})

describe('USER Memory route and header boundary', () => {
  it.each([401, 403, 503])('retains authentication and chrome on own-module HTTP %s without rendering a reader', async status => {
    window.history.replaceState(null, '', '/memory/explorer/project')
    const own = vi.spyOn(nativeRead, 'nativeRecordContexts').mockRejectedValue(new nativeRead.NativeRecordReadError(status))
    render(<App />)
    await screen.findByText(status === 401 ? 'Sign in to read Memory' : status === 403 ? 'Working context access denied' : 'Unable to read working contexts')
    expect(screen.getByText('Signed-in identity')).toBeTruthy()
    expect(screen.getByRole('group', { name: 'Working context' })).toBeTruthy()
    expect(screen.queryByLabelText('USER draft')).toBeNull()
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    own.mockRestore()
  })
  it.each([['memory', '/memory/explorer/project'], ['knowledge', '/knowledge/explorer'], ['now', '/workspace']] as const)('bootstraps %s with its own grant and no Task/global discovery', async (module, path) => {
    window.history.replaceState(null, '', path)
    const contexts = vi.spyOn(taskApi, 'contexts').mockRejectedValue(new Error('Task is denied'))
    const own = vi.spyOn(nativeRead, 'nativeRecordContexts').mockResolvedValue({ contexts: [context], current: context })
    render(<App />); await screen.findByLabelText('USER draft')
    expect(own).toHaveBeenCalledWith(module, expect.any(AbortSignal))
    expect(screen.getByRole('combobox', { name: 'Project' })).toBeTruthy(); expect(screen.getByRole('combobox', { name: 'Version' })).toBeTruthy(); expect(screen.queryByRole('combobox', { name: 'Agent' })).toBeNull()
    expect(contexts).not.toHaveBeenCalled(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(own).toHaveBeenCalledTimes(2))
    expect(readers.list).toHaveBeenCalledOnce(); expect(readers.detail).toHaveBeenCalledOnce()
    expect(contexts).not.toHaveBeenCalled(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    contexts.mockRestore(); own.mockRestore()
  })
  it.each(['/knowledge/explorer?library=imported', '/knowledge/contexts'])('keeps imported Knowledge independent from PROJECT and Task during bootstrap and refresh: %s', async path => {
    window.history.replaceState(null, '', path)
    const contexts = vi.spyOn(taskApi, 'contexts').mockRejectedValue(new Error('Task must not be requested'))
    const own = vi.spyOn(nativeRead, 'nativeRecordContexts').mockRejectedValue(new Error('PROJECT must not be requested'))
    render(<App />); await screen.findByLabelText('USER draft')
    expect(screen.getByRole('group', { name: 'Imported Knowledge context' })).toBeTruthy()
    expect(screen.queryByLabelText('Project')).toBeNull(); expect(screen.queryByLabelText('Version')).toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(readers.list).toHaveBeenCalledOnce())
    expect(readers.detail).toHaveBeenCalledOnce()
    expect(own).not.toHaveBeenCalled(); expect(contexts).not.toHaveBeenCalled(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    contexts.mockRestore(); own.mockRestore()
  })
  it('omits PROJECT/Task bootstrap and selectors, refreshes own readers and retains USER state after PROJECT changes', async () => {
    const contexts = vi.spyOn(taskApi, 'contexts').mockRejectedValue(new Error('Task access must not be requested'))
    const user = userEvent.setup(); render(<App />)
    await screen.findByLabelText('USER draft')
    expect(screen.getByRole('group', { name: 'User memory context' })).toBeTruthy()
    expect(screen.queryByLabelText('Project')).toBeNull(); expect(screen.queryByLabelText('Version')).toBeNull()
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled(); expect(contexts).not.toHaveBeenCalled()
    await user.type(screen.getByLabelText('USER draft'), ' literal')
    act(() => useAppStore.setState({ activeProjectDir: '/different-project', activeProjectVersion: 'different-version', activeAgent: 'other-agent' }))
    expect((screen.getByLabelText('USER draft') as HTMLInputElement).value).toBe('Keep my USER draft literal')
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(readers.list).toHaveBeenCalledOnce())
    expect(readers.detail).toHaveBeenCalledOnce()
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled(); expect(contexts).not.toHaveBeenCalled()
    contexts.mockRestore()
  })

  it('waits for every USER reader after a causal failure and permits Retry without catalog IO', async () => {
    let finish!: () => void
    const pending = new Promise<void>(done => { finish = done })
    readers.list.mockRejectedValueOnce(new Error('USER history unavailable')).mockResolvedValue(undefined)
    readers.detail.mockImplementationOnce(() => pending)
    const user = userEvent.setup(); render(<App />); await screen.findByLabelText('USER draft')
    const refresh = screen.getByRole('button', { name: 'Refresh' }) as HTMLButtonElement
    await user.click(refresh); expect(refresh.disabled).toBe(true)
    await act(async () => finish())
    expect(screen.getByRole('alert').textContent).toContain('USER history unavailable')
    await user.click(refresh); await waitFor(() => expect(refresh.disabled).toBe(false))
    expect(readers.list).toHaveBeenCalledTimes(2); expect(readers.detail).toHaveBeenCalledTimes(2)
    expect(screen.queryByRole('alert')).toBeNull(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
  })
})

it('keeps installed Hub global across project changes and refreshes only its own reader', async () => {
  window.history.replaceState(null, '', '/hub/installed')
  const contexts = vi.spyOn(taskApi, 'contexts').mockRejectedValue(new Error('Task must not be requested'))
  const own = vi.spyOn(nativeRead, 'nativeRecordContexts').mockRejectedValue(new Error('PROJECT must not be requested'))
  const user = userEvent.setup(); render(<App />); await screen.findByLabelText('USER draft')
  expect(screen.getByRole('group', { name: 'Installed library context' })).toBeTruthy()
  expect(screen.queryByLabelText('Project')).toBeNull(); expect(screen.queryByLabelText('Version')).toBeNull(); expect(screen.queryByLabelText('Agent')).toBeNull()
  await user.type(screen.getByLabelText('USER draft'), ' literal')
  act(() => useAppStore.setState({ activeProjectDir: '/new', activeProjectVersion: 'changed', activeAgent: 'kiro-v3' }))
  expect((screen.getByLabelText('USER draft') as HTMLInputElement).value).toBe('Keep my USER draft literal')
  await user.click(screen.getByRole('button', { name: 'Refresh' }))
  await waitFor(() => expect(readers.list).toHaveBeenCalledOnce())
  expect(readers.detail).toHaveBeenCalledOnce(); expect(contexts).not.toHaveBeenCalled(); expect(own).not.toHaveBeenCalled(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
  contexts.mockRestore(); own.mockRestore()
})

describe('FILE Hub real App routes', () => {
  beforeEach(() => {
    useAppStore.setState({ webMode: true, apiBase: '/api', activeProjectOrigin: 'hub', activeProjectKey: 'hub:' + projectA,
      activeProjectId: projectA, activeProjectDir: '', activeProjectVersion: 'release/a', activeAgent: 'kiro-v3',
      webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'route-user' } })
    vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
      if (!url.endsWith('/api/hub/contexts') || init?.method !== 'POST' || init.body !== '{}') throw new Error('Unexpected bootstrap request: ' + url)
      return new Response(JSON.stringify({ contexts: [{ ...context, project_id: projectA, project_version: 'authorized/a' }, { ...context, project_id: projectA, project_version: 'authorized/b' }], current: { ...context, project_id: projectA, project_version: 'authorized/a' } }))
    }))
    vi.spyOn(hubNative, 'projects').mockResolvedValue({ status: 'ok', projects: [{ id: projectB, name: 'Publisher' }], next_cursor: null })
    vi.spyOn(hubNative, 'search').mockResolvedValue(catalog)
    vi.spyOn(hubNative, 'installed').mockImplementation(async query => ({ ...installedPage(), scope: query.scope }))
  })
  afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals() })

  it.each([['/hub/registry', 'Hub registry'], ['/hub/local', 'Installed artifacts']])('opens %s using authorized Hub contexts and the real header', async (path, title) => {
    window.history.replaceState(null, '', path)
    render(<App />)
    await screen.findByRole('heading', { name: title }, { timeout: 5000 })
    expect(screen.queryByText('Local workspace required')).toBeNull()
    expect(screen.getByText('Signed-in identity')).toBeTruthy()
    expect(screen.getByRole('combobox', { name: 'Project' })).toBeTruthy()
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    expect(screen.getByRole('combobox', { name: 'Version' })).toBeTruthy()
    expect(screen.getByRole('combobox', { name: 'Agent' })).toBeTruthy()
    expect(fetch).toHaveBeenCalledWith('/api/hub/contexts', expect.objectContaining({ method: 'POST', body: '{}' }))
    if (path === '/hub/local') await waitFor(() => expect(hubNative.installed).toHaveBeenCalledWith(expect.objectContaining({ scope: { kind: 'project', project_id: projectA, project_version: 'authorized/a' } }), expect.any(AbortSignal)))
    else await waitFor(() => expect(hubNative.projects).toHaveBeenCalled())
  })

  it('preserves a real Registry source draft when the captured PROJECT version changes', async () => {
    window.history.replaceState(null, '', '/hub/registry')
    const user = userEvent.setup(); render(<App />)
    const publisher = await screen.findByRole('combobox', { name: 'Publisher project ID' })
    await user.type(publisher, 'source-draft')
    await user.click(screen.getByRole('combobox', { name: 'Version' })); await user.click(await screen.findByRole('option', { name: 'authorized/b' }))
    expect(screen.getByRole('combobox', { name: 'Publisher project ID' })).toBe(publisher)
    expect((publisher as HTMLInputElement).value).toBe('source-draft')
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
  })


  it.each([401,403,503])('keeps Hub bootstrap HTTP%s distinct from an empty installation and never asks Task or legacy discovery',async status=>{
    window.history.replaceState(null,'','/hub/local')
    vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({error:{code:'unavailable'}}),{status})))
    const task=vi.spyOn(taskApi,'contexts').mockRejectedValue(new Error('Task must not be requested'))
    render(<App />)
    await screen.findByText(status===401?'Sign in to read Hub':status===403?'Working context access denied':'Unable to read working contexts')
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled();expect(task).not.toHaveBeenCalled()
    expect(hubNative.installed).not.toHaveBeenCalled()
  })
  it('keeps GlobalCache and catalogue usable with no authorized PROJECT while publication stays unavailable',async()=>{
    window.history.replaceState(null,'','/hub/local')
    vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({contexts:[],current:null}))))
    const user=userEvent.setup();render(<App />)
    await screen.findByRole('heading',{name:'Installed artifacts'})
    expect(screen.getByRole('combobox',{name:'Project'})).toBeDisabled()
    expect(screen.getByRole('combobox',{name:'Agent'})).toBeEnabled()
    expect(screen.queryByRole('button',{name:'Publish project resources'})).toBeNull()
    expect(screen.getByText('Choose an available project and data version in the header to inspect maintained resources.')).toBeTruthy()
    expect(hubNative.installed).not.toHaveBeenCalled()
    await user.click(screen.getByRole('combobox',{name:'Inspect installations in'}));await user.click(await screen.findByRole('option',{name:'Installation cache'}))
    await waitFor(()=>expect(hubNative.installed).toHaveBeenCalledWith(expect.objectContaining({scope:{kind:'global_cache'}}),expect.any(AbortSignal)))
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
  })
  it('keeps a pending publication owner and original receipt across a real header version selection',async()=>{
    window.history.replaceState(null,'','/hub/local')
    let finish!: (value: Awaited<ReturnType<typeof hubNative.publish>>)=>void
    vi.spyOn(hubNative,'publish').mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
    vi.spyOn(hubNative,'authoredList').mockImplementation(async input=>({...input,status:'ok',entries:[{artifact_type:'skill',id:'fixture/web-publication',version:'release/ui',name:null}],next_cursor:null}))
    vi.spyOn(hubNative,'authoredReview').mockImplementation(async input=>({...input,status:'ok',source:'Resources',metadata:null,entries:[{path:'SKILL.md',kind:'File',bytes:10}],next_offset:null}))
    const user=userEvent.setup();render(<App />)
    await user.click(await screen.findByRole('button',{name:'Browse project resources'}))
    await user.click(await screen.findByRole('button',{name:/fixture\/web-publication/}))
    await user.click(await screen.findByRole('button',{name:'Publish project resources'}))
    await user.type(screen.getByRole('textbox',{name:'Publication name'}),'Web publication')
    await user.click(screen.getByRole('button',{name:'Review publication'}));await user.click(screen.getByRole('button',{name:'Publish artifact'}))
    const [captured,signal]=vi.mocked(hubNative.publish).mock.calls[0]
    await user.click(within(screen.getByRole('dialog')).getByRole('button',{name:'Close'}))
    await user.click(screen.getByRole('combobox',{name:'Version'}));await user.click(await screen.findByRole('option',{name:'authorized/b'}))
    await waitFor(()=>expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({scope:{kind:'project',project_id:projectA,project_version:'authorized/b'}}),expect.any(AbortSignal)))
    expect(signal?.aborted).toBe(false)
    await act(async()=>finish({status:'published',project_metadata:'published',publication:{producer:projectA,artifact_type:'skill',id:'fixture/web-publication',version:'release/ui',archive_sha256:'a'.repeat(64),payload:'published',catalog:'published'}}))
    expect(screen.getByRole('region',{name:'Publication outcome'})).toHaveTextContent(projectA+' @ authorized/a')
    expect(captured.project_version).toBe('authorized/a');expect(hubNative.publish).toHaveBeenCalledTimes(1)
  })

  it('requires an explicit Project choice when Hub current is null and keeps all literal binding options',async()=>{
    window.history.replaceState(null,'','/hub/local')
    const selected={...context,project_id:projectA,project_version:'authorized/a'}
    vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({contexts:[selected],current:null}))))
    const user=userEvent.setup();render(<App />);await screen.findByRole('heading',{name:'Installed artifacts'})
    expect(hubNative.installed).not.toHaveBeenCalled()
    expect(screen.queryByRole('button',{name:'Publish project resources'})).toBeNull()
    expect(screen.queryByRole('button',{name:'Browse project resources'})).toBeNull()
    expect(screen.getByText('Choose an available project and data version in the header to inspect maintained resources.')).toBeInTheDocument()
    await user.click(screen.getByRole('combobox',{name:'Project'}));await user.click(await screen.findByRole('option',{name:projectA}))
    await waitFor(()=>expect(hubNative.installed).toHaveBeenCalledWith(expect.objectContaining({scope:{kind:'project',project_id:projectA,project_version:'authorized/a'}}),expect.any(AbortSignal)))
    await user.click(screen.getByRole('combobox',{name:'Agent'}))
    expect(screen.getAllByRole('option')).toHaveLength(8)
    await user.click(screen.getByRole('option',{name:'Qwen'}))
    await waitFor(()=>expect(hubNative.installed).toHaveBeenLastCalledWith(expect.objectContaining({binding:'qwen'}),expect.any(AbortSignal)))
  })
  it.each(['identity','apiBase'] as const)('discards a deferred Hub bootstrap after %s changes',async boundary=>{
    window.history.replaceState(null,'','/hub/local')
    let finish!: (value: Response)=>void
    const next={...context,project_id:projectB,project_version:'authorized/new'}
    const fetcher=vi.fn().mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve})).mockImplementation(async()=>new Response(JSON.stringify({contexts:[next],current:next})))
    vi.stubGlobal('fetch',fetcher);render(<App />)
    await waitFor(()=>expect(fetcher).toHaveBeenCalledTimes(1))
    act(()=>useAppStore.setState(boundary==='apiBase'?{apiBase:'/other-api'}:{webIdentity:{state:'authenticated',provider:'corporate',subject:'new-user'}}))
    await waitFor(()=>expect(hubNative.installed).toHaveBeenCalledWith(expect.objectContaining({scope:{kind:'project',project_id:projectB,project_version:'authorized/new'}}),expect.any(AbortSignal)))
    const before=vi.mocked(hubNative.installed).mock.calls.length
    await act(async()=>finish(new Response(JSON.stringify({contexts:[context],current:context}))))
    expect(hubNative.installed).toHaveBeenCalledTimes(before)
    expect(vi.mocked(fetcher).mock.calls[0][1].signal.aborted).toBe(true)
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
  })
  it('opens Upload with authorized Hub contexts and no local workspace guard', async () => {
    window.history.replaceState(null, '', '/hub/upload')
    render(<App />)
    await screen.findByRole('heading', { name: 'Choose what you are sharing' })
    expect(screen.queryByText('Local workspace required')).toBeNull()
    expect(screen.getByRole('combobox', { name: 'Project' })).toBeTruthy()
    expect(screen.getByRole('combobox', { name: 'Version' })).toBeTruthy()
    expect(fetch).toHaveBeenCalledWith('/api/hub/contexts', expect.objectContaining({ method: 'POST', body: '{}' }))
    expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
    expect(hubNative.projects).not.toHaveBeenCalled()
    expect(hubNative.installed).not.toHaveBeenCalled()
  })
})
