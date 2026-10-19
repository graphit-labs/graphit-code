import '@/test/contextControls';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act,cleanup,render,screen,waitFor,within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { WorkspaceRefreshProvider } from '@/components/layout/WorkspaceRefreshProvider';
import { useAppStore } from '@/store/appStore';
import { loadLiveCatalog, type LiveCatalog } from '@/api/liveCatalog';
import LiveSearchPage from './LiveSearchPage';
import { getLiveSession, listLiveSessions } from '@/api/live';
import { LiveError } from '@/api/live';
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }));
vi.mock('@/api/liveCatalog',async original=>({...await original<typeof import('@/api/liveCatalog')>(),loadLiveCatalog:vi.fn()}));
vi.mock('@/api/live',async original=>{const actual=await original<typeof import('@/api/live')>();const listLiveSessions=vi.fn(async()=>[] as import('@/api/live').LiveSession[]);return {...actual,listLiveSessions,getLiveSession:vi.fn(),closeLiveSession:vi.fn(),subscribeLiveEvents:vi.fn(()=>({close:vi.fn()})),createLiveSession:vi.fn(),cancelLiveTurn:vi.fn(),sendLiveMessage:vi.fn(),getLiveKnowledgePage:vi.fn(),removeLiveSession:vi.fn()};});

import type { LiveSession, LiveOpening } from '@/api/live';
function liveSession(values: Pick<LiveSession, 'id' | 'title'> & Partial<LiveSession>): LiveSession {
 return {project:{project_id:'p',project_version:'v1'},binding:'codex',agent:{source:'hub',project_id:null,instance_id:null,project_kind:null,type:'agent',id:'codex',version:'v1'},artifacts:[],model:'model',state:'ready',cursor:{generation:'abcdef',sequence:'0'},created_at:'2026-10-15T12:00:00Z',updated_at:'2026-10-15T12:00:00Z',...values};
}
function opening(id:string):LiveOpening {return {session:liveSession({id,title:id,state:'running'}),after:{generation:'abcdef',sequence:'0'},terminal:null};}

beforeEach(()=>{vi.mocked(listLiveSessions).mockResolvedValue([]);vi.mocked(getLiveSession).mockImplementation(async id=>({session:(await listLiveSessions()).find(s=>s.id===id)!,events:[],terminal:null}));Element.prototype.scrollIntoView=vi.fn();useAppStore.setState({activeProjectDir:'/project',activeAgent:'codex',projectsLoaded:true});});
afterEach(()=>{cleanup();vi.resetAllMocks();});
const source={id:'docs',name:'Project docs',description:'Project evidence guide',project_name:'Payments display',type:'knowledge',sourceLabel:'Project',ref:{id:'docs',type:'knowledge',version:'v1',source:'project',project_id:'p',instance_id:null,project_kind:null}};
const installedAgent={id:'installed-agent',name:'Installed agent',type:'agent',sourceLabel:'Hub',ref:{id:'installed-agent',type:'agent',version:'v2',source:'hub',project_id:'producer',instance_id:null,project_kind:null}};
const project={project_id:'p',project_version:'v1'};
const catalog=(entries:LiveCatalog['entries']=[]):LiveCatalog=>({entries,errors:[],projects:[project],bindings:[{binding:'kiro-v3',available:true,model_selection:'explicit'}]});
async function configureRun(user:ReturnType<typeof userEvent.setup>) {
 await user.click(screen.getByRole('combobox',{name:'Investigation project'}));
 await user.click(await screen.findByRole('option',{name:'p · v1'}));
 await user.click(screen.getByRole('combobox',{name:'Execution binding'}));
 await user.click(await screen.findByRole('option',{name:'kiro-v3'}));
 await user.click(screen.getByRole('combobox',{name:'Installed agent'}));
 await user.click(await screen.findByRole('option',{name:/^Installed agent · v2/}));
 await user.type(screen.getByRole('textbox',{name:'Model'}),'model-explicit');
}
it('selects source-aware artifacts then clears them on header context changes',async()=>{
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog([source,installedAgent]));
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(await screen.findByRole('checkbox',{name:'Use Project docs'}));
 expect((screen.getByRole('checkbox',{name:'Use Project docs'}) as HTMLInputElement).checked).toBe(true);
 act(()=>useAppStore.setState({activeProjectDir:'/next'}));
 await waitFor(()=>expect(loadLiveCatalog).toHaveBeenCalledTimes(2));
 expect((await screen.findByRole('checkbox',{name:'Use Project docs'}) as HTMLInputElement).checked).toBe(false);
});

it('keeps a source selected while the live stream updates and preserves the follow-up draft', async () => {
  const { listLiveSessions, subscribeLiveEvents } = await import('@/api/live');
  const { getLiveKnowledgePage } = await import('@/api/live');
  vi.mocked(getLiveKnowledgePage).mockResolvedValue({title:'Guide',path:'Guide',source_path:'docs/guide.md',context:source.ref,content:'Source body',generation:'abcdef',refs:{kind:'native',data:{focused_source:null,outgoing:null,incoming:null,coverage:{},warnings:[]}}});
  vi.mocked(subscribeLiveEvents).mockReturnValue({ close: vi.fn() });
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
  vi.mocked(listLiveSessions).mockResolvedValue([liveSession({"id": "saved", "title": "Saved investigation", "state": "ready"})]);
  const user = userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage /></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab', { name: 'Recent sessions' }));
  await user.click(await screen.findByText('Saved investigation'));
  await waitFor(() => expect(subscribeLiveEvents).toHaveBeenCalledTimes(1));
  const handlers = vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(() => {
    handlers.onEvent({"sequence": "1", "turn_id": "turn1", "data": {"type": "prompt", "text": "Explain"}});
    handlers.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "text", "text": "Answer [[alpha:Guide]]"}});
  });
  const draft = screen.getByRole('textbox', { name: /question|follow/i });
  await user.type(draft, 'Keep this draft');
  await user.click(screen.getByRole('button', { name: 'Guide' }));
  await screen.findByText('Source body');
  act(() => handlers.onEvent({"sequence": "3", "turn_id": "turn1", "data": {"type": "text", "text": " More answer."}}));
  expect(screen.getByRole('tab', { name: 'Guide' }).getAttribute('aria-selected')).toBe('true');
  expect(subscribeLiveEvents).toHaveBeenCalledTimes(1);
  expect((draft as HTMLTextAreaElement).value).toBe('Keep this draft');
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  expect(screen.getByText(/More answer/)).toBeTruthy();
});


it('shows a tool call and its streamed result in the same activity item', async () => {
  const {listLiveSessions,subscribeLiveEvents}=await import('@/api/live');
  vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
  vi.mocked(listLiveSessions).mockResolvedValue([liveSession({"id": "tools", "title": "Tool investigation", "state": "ready"})]);
  const user=userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Tool investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
  const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(()=>{
    handlers.onEvent({"sequence": "1", "turn_id": "turn1", "data": {"type": "prompt", "text": "Read docs"}});
    handlers.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "tool_use", "tool_call_id": "a", "name": "read", "title": "read", "input": "input path"}});
    handlers.onEvent({"sequence": "3", "turn_id": "turn1", "data": {"type": "tool_use", "tool_call_id": "b", "name": "read", "title": "read", "input": "another path"}});
    handlers.onEvent({"sequence": "4", "turn_id": "turn1", "data": {"type": "tool_result", "tool_call_id": "b", "output": "other result", "is_error": false}});
    handlers.onEvent({"sequence": "5", "turn_id": "turn1", "data": {"type": "tool_result", "tool_call_id": "a", "output": "matching result", "is_error": false}});
  });
  await user.click(screen.getByRole('tab',{name:'Execution activity'}));
  const items=screen.getAllByRole('article',{name:'Tool · read'});
  expect(items).toHaveLength(2);
  await user.click(within(items[0]).getByText('Input'));
  await user.click(within(items[0]).getByText('Result'));
  expect(within(items[0]).getByText('input path')).toBeTruthy();
  expect(within(items[0]).getByText('matching result')).toBeTruthy();
  expect(within(items[0]).queryByText('other result')).toBeNull();
});

it('deduplicates replayed output and keeps reconnect and reader teardown separate from cancellation', async () => {
  const {listLiveSessions,subscribeLiveEvents,cancelLiveTurn,removeLiveSession,sendLiveMessage}=await import('@/api/live');
  const close=vi.fn();
  vi.mocked(subscribeLiveEvents).mockReturnValue({close});
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
  vi.mocked(listLiveSessions).mockResolvedValue([liveSession({"id": "replay", "title": "Replay investigation", "state": "running"})]);
  const user=userEvent.setup();
  const view=render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Replay investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
  expect(vi.mocked(subscribeLiveEvents).mock.calls[0].slice(0,2)).toEqual(['replay',{generation:'abcdef',sequence:'0'}]);
  const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(()=>{
    handlers.onEvent({"sequence": "1", "turn_id": "turn1", "data": {"type": "prompt", "text": "Explain replay"}});
    handlers.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "text", "text": "Observed answer."}});
    handlers.onError?.();
    handlers.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "text", "text": "Observed answer."}});
  });
  expect(screen.getByText('Observed answer.')).toBeTruthy();
  expect(screen.getByText('Reconnecting')).toBeTruthy();
  expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
  expect(screen.queryByText('Turn complete')).toBeNull();
  act(()=>handlers.onOpen?.(opening('replay')));
  expect(screen.queryByText('Reconnecting')).toBeNull();
  view.unmount();
  expect(close).toHaveBeenCalled();
  expect(cancelLiveTurn).not.toHaveBeenCalled();
  expect(removeLiveSession).not.toHaveBeenCalled();
  expect(sendLiveMessage).not.toHaveBeenCalled();
});

it('preserves a rejected cancel cause and waits for the observed terminal before allowing another turn', async () => {
  const {listLiveSessions,subscribeLiveEvents,cancelLiveTurn,sendLiveMessage}=await import('@/api/live');
  vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
  vi.mocked(listLiveSessions).mockResolvedValue([liveSession({"id": "cancel", "title": "Cancelable investigation", "state": "running"})]);
  vi.mocked(cancelLiveTurn).mockRejectedValueOnce(new Error('Cancellation was denied')).mockResolvedValueOnce({requested:true});
  const user=userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Cancelable investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
  const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(()=>handlers.onEvent({"sequence": "1", "turn_id": "turn1", "data": {"type": "prompt", "text": "Investigate"}}));
  await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Next turn');
  await user.click(screen.getByRole('button',{name:'Stop Run'}));
  expect(await screen.findByText('Cancellation was denied')).toBeTruthy();
  expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
  expect(screen.queryByText('Turn stopped')).toBeNull();
  await user.click(screen.getByRole('button',{name:'Stop Run'}));
  expect(cancelLiveTurn).toHaveBeenNthCalledWith(2,'cancel');
  expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
  expect(screen.queryByText('Turn stopped')).toBeNull();
  await act(async()=>{
    handlers.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "error", "code": "cancelled"}});
    handlers.onEvent({"sequence": "3", "turn_id": "turn1", "data": {"type": "turn_done", "outcome": "cancelled", "joined": true, "clean": true}});
    handlers.onEvent({"sequence": "4", "turn_id": "turn1", "data": {"type": "state", "state": "ready"}});
  });
  expect(screen.getByText('Turn stopped')).toBeTruthy();
  expect(screen.queryByRole('button',{name:'Stop Run'})).toBeNull();
  expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(false);
  expect((screen.getByRole('textbox',{name:'Follow-up question'}) as HTMLTextAreaElement).value).toBe('Next turn');
  expect(sendLiveMessage).not.toHaveBeenCalled();
});

it('does not adopt a late creation response after the selected project changes', async () => {
  const {listLiveSessions,createLiveSession,subscribeLiveEvents}=await import('@/api/live');
  let resolveCreate!:(session:Awaited<ReturnType<typeof createLiveSession>>)=>void;
  vi.mocked(createLiveSession).mockImplementation(()=>new Promise(resolve=>{resolveCreate=resolve;}));
  vi.mocked(listLiveSessions).mockResolvedValue([]);
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog([source,installedAgent]));
  const user=userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await configureRun(user);
  await user.click(await screen.findByRole('checkbox',{name:'Use Project docs'}));
  expect(screen.getByText('Project evidence guide')).toBeTruthy();
  expect(screen.getByText('Project · Payments display · p')).toBeTruthy();
  await user.type(screen.getByRole('textbox',{name:'What should the agent investigate?'}),'Original question');
  await user.click(screen.getByRole('button',{name:'Start Run'}));
  expect(createLiveSession).toHaveBeenCalledWith({request_id:expect.any(String),project,binding:'kiro-v3',agent:installedAgent.ref,artifacts:[source.ref],model:'model-explicit',prompt:'Original question'});
  act(()=>useAppStore.setState({activeProjectDir:'/next'}));
  await waitFor(()=>expect(loadLiveCatalog).toHaveBeenCalledTimes(2));
  await act(async()=>resolveCreate(liveSession({"id": "old-project-session", "title": "Old project response", "state": "ready"})));
  expect(screen.getByRole('tab',{name:'Prepare context'}).getAttribute('aria-selected')).toBe('true');
  expect(screen.queryByText('Old project response')).toBeNull();
  expect(subscribeLiveEvents).not.toHaveBeenCalled();
  expect((screen.getByRole('textbox',{name:'What should the agent investigate?'}) as HTMLTextAreaElement).value).toBe('Original question');
  expect((screen.getByRole('checkbox',{name:'Use Project docs'}) as HTMLInputElement).checked).toBe(false);
});

it('ignores events and connection callbacks from a closed subscription after opening another session', async () => {
  const {listLiveSessions,subscribeLiveEvents,cancelLiveTurn}=await import('@/api/live');
  const oldClose=vi.fn(), currentClose=vi.fn();
  vi.mocked(subscribeLiveEvents).mockReturnValueOnce({close:oldClose}).mockReturnValueOnce({close:currentClose});
  vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
  vi.mocked(listLiveSessions).mockResolvedValue([
    liveSession({"id": "previous", "title": "Previous investigation", "state": "running"}),
    liveSession({"id": "current", "title": "Current investigation", "state": "running"}),
  ]);
  const user=userEvent.setup();
  const view=render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Previous investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
  const previous=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Current investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledTimes(2));
  expect(oldClose).toHaveBeenCalled();
  const current=vi.mocked(subscribeLiveEvents).mock.calls[1][2];
  act(()=>{
    current.onEvent({"sequence": "1", "turn_id": "turn1", "data": {"type": "prompt", "text": "Current question"}});
    current.onEvent({"sequence": "2", "turn_id": "turn1", "data": {"type": "text", "text": "Current answer."}});
    current.onOpen?.(opening('current'));
  });
  await act(async()=>{
    previous.onEvent({"sequence": "901", "turn_id": "turn1", "data": {"type": "prompt", "text": "Foreign question"}});
    previous.onEvent({"sequence": "902", "turn_id": "turn1", "data": {"type": "text", "text": "Foreign answer."}});
    previous.onEvent({"sequence": "903", "turn_id": "turn1", "data": {"type": "state", "state": "failed"}});
    previous.onError?.();
  });
  expect(screen.getByText('Current answer.')).toBeTruthy();
  expect(screen.queryAllByText(/Foreign question|Foreign answer/)).toHaveLength(0);
  expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
  expect(screen.queryByText('Reconnecting')).toBeNull();
  act(()=>current.onError?.());
  expect(screen.getByText('Reconnecting')).toBeTruthy();
  act(()=>previous.onOpen?.(opening('previous')));
  expect(screen.getByText('Reconnecting')).toBeTruthy();
  act(()=>current.onOpen?.(opening('current')));
  expect(screen.queryByText('Reconnecting')).toBeNull();
  view.unmount();
  expect(currentClose).toHaveBeenCalled();
  expect(cancelLiveTurn).not.toHaveBeenCalled();
});

it('loads the coherent replay before tailing and displays an unpersisted terminal without inventing a replay event', async () => {
 const {subscribeLiveEvents,closeLiveSession,cancelLiveTurn,removeLiveSession}=await import('@/api/live');
 const saved=liveSession({id:'native',title:'Native history',state:'ready',cursor:{generation:'abcdef',sequence:'9007199254740993'}});
 vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(getLiveSession).mockResolvedValue({session:saved,events:[
  {sequence:'9007199254740992',turn_id:'native-turn',data:{type:'prompt',text:'Recorded question'}},
  {sequence:'9007199254740993',turn_id:'native-turn',data:{type:'text',text:'Recorded answer.'}},
 ],terminal:null});
 const close=vi.fn();vi.mocked(subscribeLiveEvents).mockReturnValue({close});
 vi.mocked(closeLiveSession).mockResolvedValue({...saved,state:'closed'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Native history'));
 await screen.findByText('Recorded answer.');
 expect(subscribeLiveEvents).toHaveBeenCalledWith('native',saved.cursor,expect.any(Object));
 const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
 act(()=>handlers.onTerminal?.({turn_id:'native-turn',outcome:'failed',error:'storage',joined:true,clean:true,persisted:false}));
 expect(screen.getByText('Observed turn completion')).toBeTruthy();
 expect(screen.getByText('History saved').nextElementSibling?.textContent).toBe('No');
 expect(screen.queryByText('Turn complete')).toBeNull();expect(screen.queryByText('Turn failed')).toBeNull();
 expect(close).not.toHaveBeenCalled();
 await user.click(screen.getByRole('button',{name:'Close session'}));
 await waitFor(()=>expect(screen.queryByRole('button',{name:'Close session'})).toBeNull());
 expect(closeLiveSession).toHaveBeenCalledWith('native');expect(close).toHaveBeenCalled();
 expect(cancelLiveTurn).not.toHaveBeenCalled();expect(removeLiveSession).not.toHaveBeenCalled();
});

it('aborts the previous history read and ignores its late result after selecting another session', async () => {
 const {subscribeLiveEvents}=await import('@/api/live');
 const first=liveSession({id:'first-history',title:'First history'}),second=liveSession({id:'second-history',title:'Second history'});
 vi.mocked(listLiveSessions).mockResolvedValue([first,second]);
 let finish!:(value:Awaited<ReturnType<typeof getLiveSession>>)=>void;
 vi.mocked(getLiveSession).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValueOnce({session:second,events:[],terminal:null});
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('First history'));
 const signal=vi.mocked(getLiveSession).mock.calls[0][1];
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Second history'));
 await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
 expect(signal?.aborted).toBe(true);
 await act(async()=>finish({session:first,events:[{sequence:'1',turn_id:'first',data:{type:'text',text:'Late foreign history'}}],terminal:null}));
 expect(screen.queryByText('Late foreign history')).toBeNull();
 expect(subscribeLiveEvents).toHaveBeenCalledWith(second.id,second.cursor,expect.any(Object));
 expect(screen.getAllByText('Second history').length).toBeGreaterThan(0);
});

it('requires explicit catalogue choices and model, then creates with complete native selectors and no fabricated running state', async () => {
 const {createLiveSession,subscribeLiveEvents}=await import('@/api/live');
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog([installedAgent]));
 vi.mocked(createLiveSession).mockImplementation(async request=>liveSession({id:request.request_id,title:'Created investigation',project:request.project,agent:request.agent,binding:request.binding,model:request.model}));
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 expect(screen.getByRole('combobox',{name:'Investigation project'}).textContent).toContain('Choose a project and version');
 expect(screen.getByRole('combobox',{name:'Execution binding'}).textContent).toContain('Choose a binding');
 expect((screen.getByRole('textbox',{name:'Model'}) as HTMLInputElement).value).toBe('');
 expect(screen.getByRole('button',{name:'Start Run'}).hasAttribute('disabled')).toBe(true);
 await configureRun(user);await user.type(screen.getByRole('textbox',{name:'What should the agent investigate?'}),'Initial question');
 await user.click(screen.getByRole('button',{name:'Start Run'}));
 await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
 expect(createLiveSession).toHaveBeenCalledWith({request_id:expect.any(String),project,binding:'kiro-v3',agent:installedAgent.ref,artifacts:[],model:'model-explicit',prompt:'Initial question'});
 expect(screen.getByText('State').nextElementSibling?.textContent).toBe('ready');
 expect(screen.queryByRole('button',{name:'Stop Run'})).toBeNull();
 expect(screen.queryByText('Initial question')).toBeNull();
  expect(screen.getByText('Waiting for turn updates…')).toBeTruthy();
  await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Following question');
  expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
  const requestId=vi.mocked(createLiveSession).mock.calls[0][0].request_id;
  const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(()=>{
   handlers.onOpen?.({session:liveSession({id:requestId,title:'Created investigation',state:'ready',cursor:{generation:'abcdef',sequence:'4'}}),after:{generation:'abcdef',sequence:'0'},terminal:{turn_id:requestId,outcome:'completed',error:null,joined:true,clean:true,persisted:true}});
   handlers.onEvent({sequence:'1',turn_id:requestId,data:{type:'prompt',text:'Initial question'}});
   handlers.onEvent({sequence:'2',turn_id:requestId,data:{type:'state',state:'running'}});
  });
  expect(screen.getByText('State').nextElementSibling?.textContent).toBe('ready');
  expect(screen.getByText('Observed turn completion')).toBeTruthy();
  expect(screen.getByText('Initial question')).toBeTruthy();
  expect(screen.queryByText('Waiting for turn updates…')).toBeNull();
  expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(false);
});

it('reuses the creation id after an uncertain failure with unchanged intent and preserves the public journal effect', async () => {
 const {createLiveSession,subscribeLiveEvents}=await import('@/api/live');
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog([installedAgent]));
 vi.mocked(createLiveSession).mockRejectedValueOnce(new LiveError(503,'unavailable','Changed')).mockImplementationOnce(async request=>liveSession({id:request.request_id,title:'Retried investigation'}));
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await configureRun(user);await user.type(screen.getByRole('textbox',{name:'What should the agent investigate?'}),'Keep the intent');
 await user.click(screen.getByRole('button',{name:'Start Run'}));
 await screen.findByText(/History effect: Changed/);
 expect((screen.getByRole('textbox',{name:'What should the agent investigate?'}) as HTMLTextAreaElement).value).toBe('Keep the intent');
 await user.click(screen.getByRole('button',{name:'Start Run'}));
 await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
 expect(vi.mocked(createLiveSession).mock.calls[0][0]).toEqual(vi.mocked(createLiveSession).mock.calls[1][0]);
});

it('allows fresh preparation after joined clean cancellation without persisted terminal and reuses the turn id on retry', async () => {
 const {sendLiveMessage,subscribeLiveEvents}=await import('@/api/live');
 const saved=liveSession({id:'reconcile',title:'Reconcile history',state:'running'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(getLiveSession).mockResolvedValue({session:saved,events:[],terminal:{turn_id:'previous',outcome:'cancelled',error:'cancelled',joined:true,clean:true,persisted:false}});
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 vi.mocked(sendLiveMessage).mockRejectedValueOnce(new LiveError(500,'storage','Changed')).mockResolvedValueOnce({...saved,state:'ready'});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Reconcile history'));
 await screen.findByRole('textbox',{name:'Follow-up question'});await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Next intent');
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(false);
 await user.click(screen.getByRole('button',{name:'Ask'}));await screen.findByText(/History effect: Changed/);
 expect((screen.getByRole('textbox',{name:'Follow-up question'}) as HTMLTextAreaElement).value).toBe('Next intent');
 await user.click(screen.getByRole('button',{name:'Ask'}));
 await waitFor(()=>expect(screen.getByText('State').nextElementSibling?.textContent).toBe('ready'));
 expect(sendLiveMessage).toHaveBeenCalledTimes(2);
 expect(vi.mocked(sendLiveMessage).mock.calls[0]).toEqual(['reconcile',{turn_id:expect.any(String),text:'Next intent'}]);
 expect(vi.mocked(sendLiveMessage).mock.calls[0]).toEqual(vi.mocked(sendLiveMessage).mock.calls[1]);
 expect(screen.queryByText('Turn stopped')).toBeNull();expect(screen.queryByText('Turn complete')).toBeNull();
});

it.each(['ready','running'] as const)('keeps the next request blocked after a %s ACK until the matching native completion',async state=>{
 const {sendLiveMessage,subscribeLiveEvents}=await import('@/api/live');
 const saved=liveSession({id:'ack-window',title:'ACK window',state:'running'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(getLiveSession).mockResolvedValue({session:saved,events:[],terminal:{turn_id:'previous',outcome:'cancelled',error:'cancelled',joined:true,clean:true,persisted:false}});
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 vi.mocked(sendLiveMessage).mockResolvedValue({...saved,state});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('ACK window'));
 await user.type(await screen.findByRole('textbox',{name:'Follow-up question'}),'New turn');await user.click(screen.getByRole('button',{name:'Ask'}));
 await waitFor(()=>expect(screen.getByText('State').nextElementSibling?.textContent).toBe(state));
 await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Keep the next draft');
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
 expect(screen.queryByText('Observed turn completion')).toBeNull();
 if(state==='running') expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
 else expect(screen.getByText('Waiting for turn updates…')).toBeTruthy();
 const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
 await act(async()=>handlers.onTerminal?.({turn_id:'previous',outcome:'cancelled',error:'cancelled',joined:true,clean:true,persisted:false}));
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
 expect(screen.queryByText('Observed turn completion')).toBeNull();
 if(state==='running') expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
 await act(async()=>handlers.onOpen?.({session:{...saved,state},after:saved.cursor,terminal:{turn_id:'previous',outcome:'cancelled',error:'cancelled',joined:true,clean:true,persisted:false}}));
 expect(screen.queryByText('Observed turn completion')).toBeNull();
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
 if(state==='running') expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
 await act(async()=>handlers.onTerminal?.({turn_id:vi.mocked(sendLiveMessage).mock.calls[0][1].turn_id,outcome:'completed',error:null,joined:true,clean:true,persisted:false}));
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(false);
 expect((screen.getByRole('textbox',{name:'Follow-up question'}) as HTMLTextAreaElement).value).toBe('Keep the next draft');
 expect(sendLiveMessage).toHaveBeenCalledOnce();
});

it('preserves a matching terminal received before the send ACK',async()=>{
 const {sendLiveMessage,subscribeLiveEvents}=await import('@/api/live');
 const saved=liveSession({id:'fast-turn',title:'Fast turn'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 let finish!:(session:LiveSession)=>void;vi.mocked(sendLiveMessage).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Fast turn'));
 await user.type(await screen.findByRole('textbox',{name:'Follow-up question'}),'Fast request');await user.click(screen.getByRole('button',{name:'Ask'}));
 const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
 const turnId=vi.mocked(sendLiveMessage).mock.calls[0][1].turn_id;
 act(()=>handlers.onTerminal?.({turn_id:turnId,outcome:'completed',error:null,joined:true,clean:true,persisted:false}));
 await act(async()=>finish({...saved,state:'running'}));
 expect(screen.getByText('Observed turn completion')).toBeTruthy();
 expect(screen.getByText('Turn').nextElementSibling?.textContent).toBe(turnId);
 await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Another request');
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(false);
 expect(screen.queryByRole('button',{name:'Stop Run'})).toBeNull();
});

it('preserves state observed on the native stream before an older send ACK',async()=>{
 const {sendLiveMessage,subscribeLiveEvents}=await import('@/api/live');
 const saved=liveSession({id:'stream-before-ack',title:'Stream before ACK'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 let finish!:(session:LiveSession)=>void;vi.mocked(sendLiveMessage).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Stream before ACK'));
 await user.type(await screen.findByRole('textbox',{name:'Follow-up question'}),'Request');await user.click(screen.getByRole('button',{name:'Ask'}));
 const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
 act(()=>handlers.onEvent({sequence:'1',turn_id:vi.mocked(sendLiveMessage).mock.calls[0][1].turn_id,data:{type:'state',state:'running'}}));
 await act(async()=>finish(saved));
 expect(screen.getByText('State').nextElementSibling?.textContent).toBe('running');
 expect(screen.getByRole('button',{name:'Stop Run'})).toBeTruthy();
 await user.type(screen.getByRole('textbox',{name:'Follow-up question'}),'Next draft');
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
});

it('keeps a closed session closed when a send ACK arrives late',async()=>{
 const {sendLiveMessage,subscribeLiveEvents,closeLiveSession}=await import('@/api/live');
 const saved=liveSession({id:'late-ack',title:'Late ACK'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});vi.mocked(closeLiveSession).mockResolvedValue({...saved,state:'closed'});
 let finish!:(session:LiveSession)=>void;vi.mocked(sendLiveMessage).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Late ACK'));
 await user.type(await screen.findByRole('textbox',{name:'Follow-up question'}),'Request');await user.click(screen.getByRole('button',{name:'Ask'}));
 await user.click(screen.getByRole('button',{name:'Close session'}));
 await waitFor(()=>expect(screen.getByText('State').nextElementSibling?.textContent).toBe('closed'));
 await act(async()=>finish({...saved,state:'running'}));
 expect(screen.getByText('State').nextElementSibling?.textContent).toBe('closed');
 expect(screen.queryByRole('button',{name:'Stop Run'})).toBeNull();
 expect(screen.queryByRole('button',{name:'Close session'})).toBeNull();
});

it.each([{joined:false,clean:true},{joined:true,clean:false}])('keeps another turn disabled when known completion is incomplete %o',async flags=>{
 const {sendLiveMessage,subscribeLiveEvents}=await import('@/api/live');
 const saved=liveSession({id:'incomplete',title:'Incomplete history',state:'running'});
 vi.mocked(loadLiveCatalog).mockResolvedValue(catalog());vi.mocked(listLiveSessions).mockResolvedValue([saved]);
 vi.mocked(getLiveSession).mockResolvedValue({session:saved,events:[],terminal:{turn_id:'previous',outcome:'failed',error:'cleanup_incomplete',persisted:false,...flags}});
 vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Recent sessions'}));await user.click(await screen.findByText('Incomplete history'));
 await user.type(await screen.findByRole('textbox',{name:'Follow-up question'}),'Preserved draft');
 expect(screen.getByRole('button',{name:'Ask'}).hasAttribute('disabled')).toBe(true);
 expect(sendLiveMessage).not.toHaveBeenCalled();
});

it('keeps scoped catalogue failures and unavailable bindings and artifacts visible without selecting a substitute', async()=>{
 const denied={...source,unavailable:'project_knowledge_mount_unavailable'};
 vi.mocked(loadLiveCatalog).mockResolvedValue({...catalog([denied,installedAgent]),errors:['Hub: permission_denied'],bindings:[{binding:'kiro-v3',available:false,model_selection:'explicit'}]});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 expect(await screen.findByText('Hub: permission_denied')).toBeTruthy();
 expect(screen.getByRole('checkbox',{name:'Use Project docs'}).hasAttribute('disabled')).toBe(true);
 await user.click(screen.getByRole('combobox',{name:'Execution binding'}));
 expect((await screen.findByRole('option',{name:'kiro-v3 — unavailable'})).getAttribute('aria-disabled')).toBe('true');
 await user.keyboard('{Escape}');
 expect(screen.getByRole('button',{name:'Start Run'}).hasAttribute('disabled')).toBe(true);
});
