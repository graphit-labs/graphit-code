import '@/test/contextControls';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { readNow, NowReadError, type NowUpdate, type NowSnapshot } from '@/api/now';
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState';
import { projectRecordHref } from '@/api/nativeRecordNavigation';
import { useWorkspaceRefresh } from '../layout/WorkspaceRefresh';
import { WorkspaceRefreshProvider } from '../layout/WorkspaceRefreshProvider';
import { WorkspaceNow } from './WorkspaceNow';
vi.mock('@/api/now', async importOriginal => ({ ...await importOriginal<typeof import('@/api/now')>(), readNow: vi.fn() }));
const selection = { project_id:'project-one',project_version:'feature/ação',project_dir:"/pÿ" };
let connections: { signal: AbortSignal; emit: (value: NowUpdate) => void; reject: (cause: Error) => void }[];
function snapshot(title='Current work'): NowSnapshot {
 return { project_id:selection.project_id,generated_at:'1792152000000000000',sections:Object.fromEntries(['tasks','sessions','memories','knowledge'].map(key=>[key,{availability:'available',items:key==='tasks'?[{id:'tsk-current',title,owner:'unit:worker',updated_at:'1792152000000000000'}]:[],total:key==='tasks'?1:0,has_more:false,error:null}])) };
}
function update(data=snapshot()): NowUpdate { return { selection,snapshot:{type:'snapshot',project_id:selection.project_id,instances:[{instance_id:'runtime-local',version:selection.project_version,now:data}]} }; }
function withSessions(items: NowSnapshot['sections'][string]['items']) { const data=snapshot(); data.sections.sessions={availability:'available',items,total:items.length,has_more:false,error:null};return data; }
function mount() { return render(<MemoryRouter initialEntries={['/workspace']}><WorkspaceNow/></MemoryRouter>); }
beforeEach(()=>{
 vi.useFakeTimers();vi.clearAllMocks();connections=[];
 useTaskWorkspace.setState({ module:'now',selection,contexts:[selection],phase:'ready',error:'' });
 vi.mocked(readNow).mockImplementation((_selection,signal,emit)=>new Promise<void>((resolve,reject)=>{connections.push({signal,emit,reject});signal.addEventListener('abort',()=>resolve(),{once:true});}));
});
afterEach(()=>{cleanup();vi.useRealTimers();});
it('receives pushed activity from local and remote instances without polling', async()=>{
 const view=mount();await act(async()=>{});
 expect(readNow).toHaveBeenCalledWith(selection,expect.any(AbortSignal),expect.any(Function));
 const value=update();value.snapshot.instances.push({instance_id:'runtime-remote',version:selection.project_version,now:snapshot('Remote work')});
 await act(async()=>{connections[0].emit(value)});
 expect(screen.getByRole('link',{name:'Current work'}).getAttribute('href')).toBe(projectRecordHref('task','tsk-current',selection));
 expect(screen.getByRole('region',{name:'Instance runtime-remote'})).toBeTruthy();
 expect(screen.getByRole('link',{name:'Remote work'})).toBeTruthy();expect(screen.getByText('Live updates · Version feature/ação')).toBeTruthy();
 await act(async()=>{await vi.advanceTimersByTimeAsync(15000)});expect(readNow).toHaveBeenCalledTimes(1);
 view.unmount();expect(connections[0].signal.aborted).toBe(true);
 await act(async()=>{await vi.advanceTimersByTimeAsync(10000)});expect(readNow).toHaveBeenCalledTimes(1);
});
it('updates sessions and memories on the same connection', async()=>{
 mount();await act(async()=>{connections[0].emit(update())});
 const value=withSessions([{id:'ses-new',title:'New session',owner:'unit:coordinator',completed_tasks:1,total_tasks:2}]);
 value.sections.memories.items=[{id:'mem-new',title:'New memory',updated_by:'verified-actor'}];value.sections.memories.total=1;
 await act(async()=>{connections[0].emit(update(value))});
 expect(screen.getByRole('link',{name:'New session'}).getAttribute('href')).toBe(projectRecordHref('session','ses-new',selection));
 expect(screen.getByRole('link',{name:'New memory'}).getAttribute('href')).toBe(projectRecordHref('memory','mem-new',selection));
 expect(readNow).toHaveBeenCalledTimes(1);
});
it('preserves completed counts when cancellation shrinks the total and keeps empty sessions neutral', async()=>{
 mount();const initial=withSessions([
 {id:'ses-partial',title:'Partial session',completed_tasks:1,total_tasks:2},
 {id:'ses-empty',title:'Empty session',completed_tasks:0,total_tasks:0},
 {id:'ses-cancelled',title:'Only cancelled tasks',completed_tasks:0,total_tasks:0},
 {id:'ses-complete',title:'Complete session',completed_tasks:2,total_tasks:2},
 ]);
 await act(async()=>{connections[0].emit(update(initial))});
 expect(screen.getByRole('progressbar',{name:'Task progress: 1 of 2 complete'}).getAttribute('aria-valuenow')).toBe('50');
 expect(screen.getAllByText('No applicable tasks')).toHaveLength(2);expect(screen.getAllByRole('progressbar')).toHaveLength(2);
 initial.sections.sessions.items[0].total_tasks=1;
 await act(async()=>{connections[0].emit(update(initial))});
 expect(screen.queryByText('1 of 2 complete')).toBeNull();expect(screen.getByRole('progressbar',{name:'Task progress: 1 of 1 complete'}).getAttribute('aria-valuenow')).toBe('100');
 expect(screen.getAllByRole('progressbar')).toHaveLength(2);
});
it('aborts the previous exact native scope and ignores its late updates', async()=>{
 mount();await act(async()=>{connections[0].emit(update())});
 const changed={...selection,project_version:'other'};
 await act(async()=>{useTaskWorkspace.setState({selection:changed,contexts:[selection,changed]})});
 expect(connections[0].signal.aborted).toBe(true);expect(readNow).toHaveBeenLastCalledWith(changed,expect.any(AbortSignal),expect.any(Function));
 await act(async()=>{connections[0].emit(update(snapshot('Stale work')))});expect(screen.queryByText('Stale work')).toBeNull();expect(screen.queryByText('Current work')).toBeNull();
});
it('withdraws disconnected snapshots and reconnects once without overlapping streams', async()=>{
 mount();await act(async()=>{connections[0].emit(update())});
 await act(async()=>{connections[0].reject(new Error('offline'))});
 expect(screen.queryByText('Current work')).toBeNull();expect(screen.getByText('Activity unavailable')).toBeTruthy();
 await act(async()=>{await vi.advanceTimersByTimeAsync(1999)});expect(readNow).toHaveBeenCalledTimes(1);
 await act(async()=>{await vi.advanceTimersByTimeAsync(1)});expect(readNow).toHaveBeenCalledTimes(2);expect(connections[0].signal.aborted).toBe(true);
 await act(async()=>{connections[1].emit(update(snapshot('Recovered work')))});
 expect(screen.getByText('Recovered work')).toBeTruthy();expect(screen.queryByText('Activity unavailable')).toBeNull();
});
it.each([401,403])('stops reconnecting when authentication or PROJECT permission is denied (%s)', async status=>{
 mount();await act(async()=>{connections[0].reject(new NowReadError(status,'Access denied.'))});
 await act(async()=>{await vi.advanceTimersByTimeAsync(15000)});expect(readNow).toHaveBeenCalledTimes(1);expect(screen.getByText('Disconnected')).toBeTruthy();
});
it('preserves partial forbidden sections and distinguishes no online instances', async()=>{
 mount();const value=snapshot();value.sections.memories={availability:'failed',items:[],total:null,has_more:null,error:'forbidden'};
 await act(async()=>{connections[0].emit(update(value))});
 expect(screen.getByText('Current work')).toBeTruthy();expect(screen.getByText('Access to this source is not permitted.')).toBeTruthy();
 const empty=update();empty.snapshot.instances=[];await act(async()=>{connections[0].emit(empty)});expect(screen.getByText('No online instances')).toBeTruthy();expect(screen.queryByText('Current work')).toBeNull();
});
it('restarts the owned stream from the shared header refresh and waits for its first update', async()=>{
 const refresh=vi.fn(async()=>{});useTaskWorkspace.setState({refresh});
 function Header(){const state=useWorkspaceRefresh();return <button onClick={()=>void state?.refresh()}>Refresh workspace</button>;}
 render(<MemoryRouter initialEntries={['/workspace']}><WorkspaceRefreshProvider><Header/><WorkspaceNow/></WorkspaceRefreshProvider></MemoryRouter>);
 await act(async()=>{connections[0].emit(update())});
 await act(async()=>{fireEvent.click(screen.getByRole('button',{name:'Refresh workspace'}))});
 expect(refresh).toHaveBeenCalledTimes(1);expect(readNow).toHaveBeenCalledTimes(2);expect(connections[0].signal.aborted).toBe(true);
 await act(async()=>{connections[1].emit(update(snapshot('Fresh work')))});expect(screen.getByRole('link',{name:'Fresh work'})).toBeTruthy();
});

it('keeps the observation time through retransmission and advances only for a new observation', async()=>{
 const observed=snapshot();const milliseconds=Number(BigInt(observed.generated_at)/1000000n);
 vi.setSystemTime(new Date(milliseconds+60000));
 mount();await act(async()=>{connections[0].emit(update(observed))});
 const label='Observed at '+new Date(milliseconds).toLocaleString();
 expect(screen.getByText(label)).toBeTruthy();
 await act(async()=>{await vi.advanceTimersByTimeAsync(60000)});
 const retransmitted=structuredClone(observed);
 await act(async()=>{connections[0].emit(update(retransmitted))});
 expect(screen.getByText(label)).toBeTruthy();
 expect(screen.queryByText(/^Last received /)).toBeNull();
 expect(screen.getAllByRole('link',{name:'Current work'})).toHaveLength(1);
 expect(screen.getByText('Live updates · Version feature/ação')).toBeTruthy();
 const fresh=snapshot('Newly observed work');fresh.generated_at=String(BigInt(observed.generated_at)+30000000000n);
 await act(async()=>{connections[0].emit(update(fresh))});
 expect(screen.queryByText(label)).toBeNull();
 expect(screen.getByText('Observed at '+new Date(milliseconds+30000).toLocaleString())).toBeTruthy();
 expect(screen.getByRole('link',{name:'Newly observed work'})).toBeTruthy();
 expect(readNow).toHaveBeenCalledTimes(1);
});
