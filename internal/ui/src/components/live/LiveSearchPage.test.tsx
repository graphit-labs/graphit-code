import '@/test/contextControls';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act,cleanup,render,screen,waitFor,within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { WorkspaceRefreshProvider } from '@/components/layout/WorkspaceRefresh';
import { useAppStore } from '@/store/appStore';
import { loadLiveCatalog } from '@/api/liveCatalog';
import LiveSearchPage from './LiveSearchPage';
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }));
vi.mock('@/api/liveCatalog',async original=>({...await original<typeof import('@/api/liveCatalog')>(),loadLiveCatalog:vi.fn()}));
vi.mock('@/api/live',()=>({listLiveSessions:vi.fn(async()=>[]),subscribeLiveEvents:vi.fn(()=>({close:vi.fn()})),createLiveSession:vi.fn(),cancelLiveTurn:vi.fn(),sendLiveMessage:vi.fn(),removeLiveSession:vi.fn()}));
beforeEach(()=>{Element.prototype.scrollIntoView=vi.fn();useAppStore.setState({activeProjectDir:'/project',activeAgent:'codex',projectsLoaded:true});});
afterEach(()=>{cleanup();vi.resetAllMocks();});
const source={id:'docs',name:'Project docs',type:'knowledge',latest:'local',sourceLabel:'Project · Current',ref:{id:'docs',type:'knowledge',version:'local',source:'project' as const,project_id:'p',instance_id:'i',project_kind:'own_context'}};
it('selects source-aware artifacts then clears them on header context changes',async()=>{
 vi.mocked(loadLiveCatalog).mockResolvedValue({entries:[source],errors:[]});
 const user=userEvent.setup();render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
 await user.click(await screen.findByRole('checkbox',{name:'Use Project docs'}));
 expect((screen.getByRole('checkbox',{name:'Use Project docs'}) as HTMLInputElement).checked).toBe(true);
 act(()=>useAppStore.setState({activeProjectDir:'/next'}));
 await waitFor(()=>expect(loadLiveCatalog).toHaveBeenLastCalledWith('/next','codex'));
 expect((await screen.findByRole('checkbox',{name:'Use Project docs'}) as HTMLInputElement).checked).toBe(false);
});

it('keeps a source selected while the live stream updates and preserves the follow-up draft', async () => {
  const { listLiveSessions, subscribeLiveEvents } = await import('@/api/live');
  const { api } = await import('@/api/client');
  vi.spyOn(api, 'get').mockResolvedValue({ title: 'Guide', path: 'Guide', context: 'alpha', content: 'Source body', version: 'local' });
  vi.mocked(subscribeLiveEvents).mockReturnValue({ close: vi.fn() });
  vi.mocked(loadLiveCatalog).mockResolvedValue({ entries: [], errors: [] });
  vi.mocked(listLiveSessions).mockResolvedValue([{ id: 'saved', agent: 'codex', title: 'Saved investigation', state: 'ready', created_at: '', updated_at: '', last_seq: 0 }]);
  const user = userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage /></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab', { name: 'Recent sessions' }));
  await user.click(await screen.findByText('Saved investigation'));
  await waitFor(() => expect(subscribeLiveEvents).toHaveBeenCalledTimes(1));
  const handlers = vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(() => {
    handlers.onEvent({ seq: 1, kind: 'prompt', text: 'Explain', at: '' });
    handlers.onEvent({ seq: 2, kind: 'text', text: 'Answer [[alpha:Guide]]', at: '' });
  });
  const draft = screen.getByRole('textbox', { name: /question|follow/i });
  await user.type(draft, 'Keep this draft');
  await user.click(screen.getByRole('button', { name: 'Guide' }));
  await screen.findByText('Source body');
  act(() => handlers.onEvent({ seq: 3, kind: 'text', text: ' More answer.', at: '' }));
  expect(screen.getByRole('tab', { name: 'Guide' }).getAttribute('aria-selected')).toBe('true');
  expect(subscribeLiveEvents).toHaveBeenCalledTimes(1);
  expect((draft as HTMLTextAreaElement).value).toBe('Keep this draft');
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  expect(screen.getByText(/More answer/)).toBeTruthy();
});


it('shows a tool call and its streamed result in the same activity item', async () => {
  const {listLiveSessions,subscribeLiveEvents}=await import('@/api/live');
  vi.mocked(subscribeLiveEvents).mockReturnValue({close:vi.fn()});
  vi.mocked(loadLiveCatalog).mockResolvedValue({entries:[],errors:[]});
  vi.mocked(listLiveSessions).mockResolvedValue([{id:'tools',agent:'codex',title:'Tool investigation',state:'ready',created_at:'',updated_at:''}]);
  const user=userEvent.setup();
  render(<MemoryRouter><WorkspaceRefreshProvider><LiveSearchPage/></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(screen.getByRole('tab',{name:'Recent sessions'}));
  await user.click(await screen.findByText('Tool investigation'));
  await waitFor(()=>expect(subscribeLiveEvents).toHaveBeenCalledOnce());
  const handlers=vi.mocked(subscribeLiveEvents).mock.calls[0][2];
  act(()=>{
    handlers.onEvent({seq:1,kind:'prompt',text:'Read docs',at:''});
    handlers.onEvent({seq:2,kind:'tool_use',tool:'read',tool_call_id:'a',detail:'input path',at:''});
    handlers.onEvent({seq:3,kind:'tool_use',tool:'read',tool_call_id:'b',detail:'another path',at:''});
    handlers.onEvent({seq:4,kind:'tool_result',tool_call_id:'b',detail:'other result',at:''});
    handlers.onEvent({seq:5,kind:'tool_result',tool_call_id:'a',detail:'matching result',at:''});
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
