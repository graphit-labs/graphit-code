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
beforeEach(()=>{vi.useFakeTimers();vi.resetAllMocks();vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })));useAppStore.setState({activeProjectDir:'/project',activeAgent:'codex'});vi.mocked(api.get).mockResolvedValue(snapshot());});
afterEach(()=>{cleanup();vi.useRealTimers();});
it('polls every five seconds and stops after unmount',async()=>{
 const view=render(<MemoryRouter><WorkspaceNow/></MemoryRouter>);
 await act(async()=>{});
 expect(screen.getByRole('link',{name:'Current work'}).getAttribute('href')).toBe('/task/explorer/t');
 expect(screen.getByText('Checking evidence').tagName).toBe('STRONG');
 await act(async()=>{await vi.advanceTimersByTimeAsync(4999)});expect(api.get).toHaveBeenCalledTimes(1);
 await act(async()=>{await vi.advanceTimersByTimeAsync(1)});expect(api.get).toHaveBeenCalledTimes(2);
 view.unmount();await act(async()=>{await vi.advanceTimersByTimeAsync(10000)});expect(api.get).toHaveBeenCalledTimes(2);
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
