import '@/test/contextControls';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '@/api/client';
import { useAppStore } from '@/store/appStore';
import { WorkspaceRefreshProvider, useWorkspaceRefresh } from '../layout/WorkspaceRefresh';
import { WorkspaceNow, type NowSnapshot } from './WorkspaceNow';
vi.mock('@/api/client', () => ({ api: { get: vi.fn() } }));
function snapshot(title='Current work'): NowSnapshot { return { project_id:'p',generated_at:'2026-09-21T12:00:00Z',sections:Object.fromEntries(['tasks','sessions','memories','knowledge'].map(key=>[key,{items:key==='tasks'?[{id:'t',title,href:'/task/explorer/t',owner:'unit:worker',progress:'**Checking evidence**'}]:[],total:key==='tasks'?1:0,has_more:false}])) }; }
function withItems(data: NowSnapshot, key: string, items: NowSnapshot['sections'][string]['items']): NowSnapshot {
 return { ...data, sections: { ...data.sections, [key]: { items, total: items.length, has_more: false } } };
}
beforeEach(()=>{vi.useFakeTimers();vi.resetAllMocks();vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })));useAppStore.setState({activeProjectDir:'/project',activeAgent:'codex'});vi.mocked(api.get).mockResolvedValue(snapshot());});
afterEach(()=>{cleanup();vi.useRealTimers();});
it('polls every five seconds and stops after unmount',async()=>{
 const view=render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);
 await act(async()=>{});
 expect(screen.getByRole('link',{name:'Current work'}).getAttribute('href')).toBe('/task/explorer/t');
 expect(screen.getByText('Checking evidence').tagName).toBe('STRONG');
 await act(async()=>{await vi.advanceTimersByTimeAsync(4999)});expect(api.get).toHaveBeenCalledTimes(1);
 await act(async()=>{await vi.advanceTimersByTimeAsync(1)});expect(api.get).toHaveBeenCalledTimes(2);
 expect(vi.mocked(api.get).mock.calls[1][1]).toMatchObject({ cache: 'no-store' });
 view.unmount();await act(async()=>{await vi.advanceTimersByTimeAsync(10000)});expect(api.get).toHaveBeenCalledTimes(2);
});
it('renders sessions and memories created after the initial snapshot on the next poll', async () => {
 const initial = snapshot();
 const current = withItems(withItems(snapshot(), 'sessions', [{ id:'ses-new', title:'New session', href:'/task/sessions/ses-new', owner:'unit:coordinator' }]), 'memories', [{ id:'mem-new', title:'New memory', href:'/memory/explorer/project/mem-new' }]);
 vi.mocked(api.get).mockResolvedValueOnce(initial).mockResolvedValueOnce(current);
 render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);
 await act(async()=>{});
 expect(screen.queryByRole('link',{name:'New session'})).toBeNull();
 await act(async()=>{await vi.advanceTimersByTimeAsync(5000)});
 expect(screen.getByRole('link',{name:'New session'}).getAttribute('href')).toBe('/task/sessions/ses-new');
 expect(screen.getByRole('link',{name:'New memory'}).getAttribute('href')).toBe('/memory/explorer/project/mem-new');
 expect(useAppStore.getState().activeProjectDir).toBe('/project');
});
it('renders session task progress and refreshes changed counts on the next poll', async () => {
 const initial = withItems(snapshot(), 'sessions', [
  { id:'ses-partial', title:'Partial session', href:'/task/sessions/ses-partial', completed_tasks:1, total_tasks:3 },
  { id:'ses-empty', title:'Empty session', href:'/task/sessions/ses-empty', completed_tasks:0, total_tasks:0 },
  { id:'ses-complete', title:'Complete session', href:'/task/sessions/ses-complete', completed_tasks:2, total_tasks:2 },
 ]);
 const current = withItems(snapshot(), 'sessions', [
  { id:'ses-partial', title:'Partial session', href:'/task/sessions/ses-partial', completed_tasks:2, total_tasks:3 },
  { id:'ses-empty', title:'Empty session', href:'/task/sessions/ses-empty', completed_tasks:0, total_tasks:0 },
  { id:'ses-complete', title:'Complete session', href:'/task/sessions/ses-complete', completed_tasks:2, total_tasks:2 },
 ]);
 vi.mocked(api.get).mockResolvedValueOnce(initial).mockResolvedValueOnce(current);
 render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);
 await act(async()=>{});
 expect(screen.getByText('1 of 3 complete')).toBeTruthy();
 expect(screen.getByText('2 of 2 complete')).toBeTruthy();
 expect(screen.getByText('No tasks')).toBeTruthy();
 expect(screen.getByRole('progressbar', { name:'Task progress: 1 of 3 complete' }).getAttribute('aria-valuenow')).toBe('1');
 expect(screen.getAllByRole('progressbar')).toHaveLength(2);
 await act(async()=>{await vi.advanceTimersByTimeAsync(5000)});
 expect(screen.queryByText('1 of 3 complete')).toBeNull();
 expect(screen.getByRole('progressbar', { name:'Task progress: 2 of 3 complete' }).getAttribute('aria-valuenow')).toBe('2');
 expect(screen.getAllByRole('progressbar')).toHaveLength(2);
});
it('does not overlap requests and ignores the previous project response',async()=>{
 let resolve!:(x:NowSnapshot)=>void;
 vi.mocked(api.get).mockImplementationOnce(()=>new Promise(r=>{resolve=r}));
 render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);
 await act(async()=>{await vi.advanceTimersByTimeAsync(15000)});expect(api.get).toHaveBeenCalledTimes(1);
 const firstSignal=vi.mocked(api.get).mock.calls[0][1]?.signal;
 await act(async()=>{useAppStore.setState({activeProjectDir:'/other'})});
 expect(firstSignal?.aborted).toBe(true);expect(api.get).toHaveBeenCalledTimes(2);
 await act(async()=>{resolve(snapshot('Stale work'))});
 expect(screen.queryByText('Stale work')).toBeNull();expect(screen.getByText('Current work')).toBeTruthy();
});
it('preserves the last snapshot and reports a failed refresh',async()=>{
 render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);await act(async()=>{});
 vi.mocked(api.get).mockRejectedValueOnce(new Error('offline'));
 await act(async()=>{await vi.advanceTimersByTimeAsync(5000)});
 expect(screen.getByText('Activity may be out of date')).toBeTruthy();expect(screen.getByText('Current work')).toBeTruthy();
 await act(async()=>{await vi.advanceTimersByTimeAsync(5000)});expect(screen.queryByText('Activity may be out of date')).toBeNull();
});

it('joins the header refresh and deduplicates it with polling', async () => {
  const loadProjects = vi.fn(async () => {});
  useAppStore.setState({ loadProjects, projectsError: '' });
  function Header() { const refresh = useWorkspaceRefresh(); return <button onClick={() => void refresh?.refresh()}>Refresh workspace</button>; }
  render(<MemoryRouter><WorkspaceRefreshProvider><Header/><WorkspaceNow/></WorkspaceRefreshProvider></MemoryRouter>);
  await act(async () => {});
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Refresh workspace' })); });
  expect(loadProjects).toHaveBeenCalledTimes(1);
  expect(api.get).toHaveBeenCalledTimes(2);
 });
 it('renders a fresh multi-source snapshot from the header refresh', async () => {
  const loadProjects = vi.fn(async () => {});
  const current = withItems(withItems(snapshot(), 'sessions', [{ id:'ses-header', title:'Header session', href:'/task/sessions/ses-header', completed_tasks:2, total_tasks:3 }]), 'knowledge', [{ id:'fresh-doc', title:'Fresh document', href:'/knowledge/explorer?page=fresh-doc' }]);
  vi.mocked(api.get).mockResolvedValueOnce(snapshot()).mockResolvedValueOnce(current);
  useAppStore.setState({ loadProjects, projectsError: '' });
  function Header() { const refresh = useWorkspaceRefresh(); return <button onClick={() => void refresh?.refresh()}>Refresh workspace</button>; }
  render(<MemoryRouter><WorkspaceRefreshProvider><Header/><WorkspaceNow/></WorkspaceRefreshProvider></MemoryRouter>);
  await act(async () => {});
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Refresh workspace' })); });
  expect(screen.getByRole('link',{name:'Header session'})).toBeTruthy();
  expect(screen.getByRole('progressbar',{name:'Task progress: 2 of 3 complete'})).toBeTruthy();
  expect(screen.getByRole('link',{name:'Fresh document'})).toBeTruthy();
  expect(loadProjects).toHaveBeenCalledTimes(1);
  expect(api.get).toHaveBeenCalledTimes(2);
 });
