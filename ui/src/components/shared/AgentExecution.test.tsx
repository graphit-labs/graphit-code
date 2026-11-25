import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { AgentExecution, appendProgress } from './AgentExecution';
import { ExecutionActivity } from './ExecutionActivity';
afterEach(cleanup);
it('replaces the live label, ignores empty updates and keeps history collapsed', () => {
 const events=[{kind:'prep',text:'Creating ephemeral project...'}];
 const {rerender,container}=render(<AgentExecution events={events} running />);
 expect(screen.getByRole('status').textContent).toBe('Creating ephemeral project...');
 expect(container.querySelector('details')?.open).toBe(false);
 events.push({kind:'prep',text:'Installing the Framework’s skills...'}, {kind:'thinking',text:''});
 rerender(<AgentExecution events={events} running />);
 expect(screen.getAllByRole('status')).toHaveLength(1);
 expect(screen.getByRole('status').textContent).toBe('Installing the Framework’s skills...');
 expect(screen.queryByText('Working…')).toBeNull();
 expect(container.querySelectorAll('.execution-dots')).toHaveLength(1);
 rerender(<AgentExecution events={events} running={false} outcome="completed" />);
 expect(screen.getByRole('status').textContent).toBe('Execution complete');
 expect(container.querySelector('.execution-dots')).toBeNull();
});
it('keeps diagnostics and cancellation accessible without presenting thinking chunks as status', () => {
 const cancel=vi.fn(); const events=[{kind:'thinking',text:'partial thought'}, {kind:'stderr',text:'diagnostic'}];
 const {rerender}=render(<AgentExecution events={events} running onCancel={cancel} />);
 expect(screen.getByRole('status').textContent).toBe('Processing...');
 fireEvent.click(screen.getByText('Execution details'));
 expect(screen.getByText('diagnostic')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Stop generation'}));expect(cancel).toHaveBeenCalledOnce();
 rerender(<AgentExecution events={events} running={false} outcome="cancelled" />);
 expect(screen.getByRole('status').textContent).toBe('Execution stopped');
 rerender(<AgentExecution events={[...events,{kind:'error',text:'failed request'}]} running={false} outcome="failed" />);
 expect(screen.getByRole('status').textContent).toBe('Execution failed');
 expect(screen.getByText('failed request')).toBeTruthy();
});
it('groups input and result in one activity item, including empty output', () => {
 render(<ExecutionActivity events={[{kind:'tool_use',tool:'read',tool_call_id:'a',detail:'input'},{kind:'tool_result',tool_call_id:'a',text:'result'},{kind:'tool_use',tool:'empty',tool_call_id:'b'},{kind:'tool_result',tool_call_id:'b'}]}/>);
 const item=screen.getByRole('article',{name:'Tool · read'});
 expect(within(item).getByText('input')).toBeTruthy();expect(within(item).getByText('result')).toBeTruthy();
 expect(screen.getAllByRole('article')).toHaveLength(2);
 expect(within(screen.getByRole('article',{name:'Tool · empty'})).getByText('No output returned.')).toBeTruthy();
});
it('limits the compact diagnostic buffer without changing the full activity payload', () => {
 let events=[] as Parameters<typeof appendProgress>[0];
 for(let i=0;i<305;i++)events=appendProgress(events,{kind:'stdout',tool:`event-${i}`,text:'x'.repeat(13000)});
 expect(events).toHaveLength(300);expect(events[0].text).toHaveLength(12000);expect(events[0].tool).toBe('event-5');
 const full='start'+ 'x'.repeat(13000)+'end';
 render(<ExecutionActivity events={[{kind:'tool_result',tool_call_id:'large',detail:full}]}/>);
 expect(screen.getByText(full)).toBeTruthy();
});
