import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { getLiveKnowledgePage, LiveError, type LiveArtifactSelection, type LiveKnowledgeCandidate, type LiveKnowledgePage, type LiveKnowledgeResult } from '@/api/live';
import { knowledgeEdge, referenceSource } from '@/api/nativeRecordReferences.test-fixtures';
import { LiveAnswer } from './LiveAnswer';
import { LiveEvidence } from './LiveEvidence';
vi.mock('@/api/live', async original => ({ ...await original<typeof import('@/api/live')>(), getLiveKnowledgePage: vi.fn() }));
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

function View({ sessionId = 'A', generation = 'abcdef', content = '**Answer** [[alpha:rules]] [[beta:rules]] [[alias:rules]]' }: { sessionId?: string; generation?: string; content?: string }) {
  const [value, setValue] = useState('output');
  return <LiveEvidence sessionId={sessionId} generation={generation} value={value} onChange={setValue}
    output={onLink => <><input aria-label="Saved draft" defaultValue="continue here" /><LiveAnswer content={content} onLink={onLink} /></>}
    activity={<p>Existing activity</p>} />;
}
const selection = (id: string): LiveArtifactSelection => ({source:'hub',project_id:'producer',instance_id:null,project_kind:null,type:'knowledge',id:'artifact-'+id,version:'1.2'});
const origin = (id: string) => 'hub · producer · knowledge · artifact-'+id+' · 1.2';
const page = (context = 'alpha', path = 'rules', title = 'Rules', content = '**Source** [[next]]'): LiveKnowledgePage => {
  const focused = {...referenceSource,slug:path,source_path:'docs/'+path+'.md'};
  return {context:selection(context),path,source_path:focused.source_path,title,content,generation:'abcdef',refs:{kind:'imported',data:{focused_source:focused,outgoing:{results:[{...knowledgeEdge(),source:focused}],total:1,complete:true,next_cursor:null},incoming:null,coverage:{knowledge_source_complete:true},warnings:[]}}};
};
const candidate=(value:LiveKnowledgePage):LiveKnowledgeCandidate=>({title:value.title,path:value.path,source_path:value.source_path,context:value.context});
const output = () => screen.getByRole('tabpanel', { name: 'Agent output' });

it('opens nested sources in reusable tabs and preserves output, draft and activity', async () => {
  const user = userEvent.setup();
  vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('alpha', 'next', 'Next', 'Resolved'));
  render(<View />);
  const originalOutput = output(); originalOutput.scrollTop = 83;
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  expect(await screen.findByText('Source')).toBeTruthy();
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByRole('tab', { name: 'Rules' }).getAttribute('aria-selected')).toBe('true');
  expect(screen.getByText('artifact-alpha')).toBeTruthy();
  expect(screen.getByText('1.2')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'next' }));
  await screen.findByText('Resolved');
  expect(getLiveKnowledgePage).toHaveBeenLastCalledWith('A', 'abcdef', 'next', {context:selection('alpha')}, expect.any(AbortSignal));
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  expect(output()).toBe(originalOutput); expect(output().scrollTop).toBe(83);
  expect((screen.getByLabelText('Saved draft') as HTMLInputElement).value).toBe('continue here');
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  expect(getLiveKnowledgePage).toHaveBeenCalledTimes(2);
  await user.click(screen.getByRole('tab', { name: 'Execution activity' }));
  expect(screen.getByText('Existing activity')).toBeTruthy();
  await user.click(screen.getByRole('tab', { name: 'Next' }));
  await user.click(screen.getByRole('button', { name: 'Close tab Next' }));
  expect(screen.queryByRole('tab', { name: 'Next' })).toBeNull();
  expect(screen.getByRole('tab', { name: 'Agent output' }).getAttribute('aria-selected')).toBe('true');
});

it('keeps different contexts separate and merges resolved aliases', async () => {
  const user = userEvent.setup();
  vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('beta')).mockResolvedValueOnce(page('alpha','rules','Rules','Freshly observed source'));
  render(<View />);
  for (const index of [0, 1, 2]) {
    await user.click(within(output()).getAllByRole('button', { name: 'rules' })[index]);
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
    await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  }
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  expect(screen.getByRole('tab', { name: 'Rules · '+origin('alpha') })).toBeTruthy();
  expect(screen.getByRole('tab', { name: 'Rules · '+origin('beta') })).toBeTruthy();
  expect(screen.getByText('Freshly observed source')).toBeTruthy();
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[2]);
  expect(getLiveKnowledgePage).toHaveBeenCalledTimes(3);
  expect(screen.getByRole('tab', { name: 'Rules · '+origin('alpha') }).getAttribute('aria-selected')).toBe('true');
});

it('offers context choice and retries source errors within the tab', async () => {
  const user = userEvent.setup();
  vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce({ generation:'abcdef', candidates: [candidate(page()), candidate(page('beta'))] }).mockRejectedValueOnce(new Error('Source removed')).mockResolvedValueOnce(page('beta'));
  render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  await user.click(await screen.findByRole('button', { name: 'Rules · '+origin('beta') }));
  expect(await screen.findByText('Source removed')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  await screen.findByText('Source');
  expect(getLiveKnowledgePage).toHaveBeenLastCalledWith('A', 'abcdef', 'rules', {context:selection('beta')}, expect.any(AbortSignal));
  expect(screen.getAllByRole('tab')).toHaveLength(3);
});

it('aborts closed readers and ignores their late result', async () => {
  let resolve!: (value: LiveKnowledgeResult) => void;
  vi.mocked(getLiveKnowledgePage).mockImplementation(() => new Promise(r => { resolve = r; }));
  const user = userEvent.setup(); render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  const signal = vi.mocked(getLiveKnowledgePage).mock.calls[0][4];
  await user.click(screen.getByRole('button', { name: 'Close tab rules' }));
  expect(signal?.aborted).toBe(true);
  await act(async () => resolve(page()));
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(screen.queryByText('Source')).toBeNull();
});

it('clears readers when the investigation changes', async () => {
  let resolve!: (value: LiveKnowledgeResult) => void;
  vi.mocked(getLiveKnowledgePage).mockImplementation(() => new Promise(r => { resolve = r; }));
  const user = userEvent.setup(), view = render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  const signal = vi.mocked(getLiveKnowledgePage).mock.calls[0][4];
  view.rerender(<View sessionId="B" content="Different answer" />);
  expect(signal?.aborted).toBe(true);
  await act(async () => resolve(page()));
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(screen.getByText('Different answer')).toBeTruthy();
  expect(screen.queryByText('Source')).toBeNull();
});

it('aborts and clears readers when the same investigation changes generation',async()=>{
 let finish!:(value:LiveKnowledgeResult)=>void;
 vi.mocked(getLiveKnowledgePage).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const user=userEvent.setup(),view=render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));
 const signal=vi.mocked(getLiveKnowledgePage).mock.calls[0][4];
 view.rerender(<View generation="abcdee" content="New generation answer [[fresh]]"/>);
 expect(signal?.aborted).toBe(true);
 await act(async()=>finish(page()));
 expect(screen.queryByText('Source')).toBeNull();expect(screen.getAllByRole('tab')).toHaveLength(2);
 await user.click(screen.getByRole('button',{name:'fresh'}));
 expect(getLiveKnowledgePage).toHaveBeenLastCalledWith('A','abcdee','fresh',{},expect.any(AbortSignal));
});

it('keeps the selected tuple and generation on context_changed retry without adopting another source',async()=>{
 const selected={...selection('alpha'),instance_id:'instance',project_kind:'local'};
 vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce({generation:'abcdef',candidates:[candidate({...page(),context:selected})]}).mockRejectedValueOnce(new LiveError(409,'context_changed')).mockResolvedValueOnce({...page(),context:selected});
 const user=userEvent.setup();render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));
 await user.click(await screen.findByRole('button',{name:/Rules · hub · producer · instance · local/}));
 expect(await screen.findByText(/HTTP 409, context_changed/)).toBeTruthy();
 expect(screen.queryByText('Source')).toBeNull();
 await user.click(screen.getByRole('button',{name:'Try again'}));await screen.findByText('Source');
 expect(vi.mocked(getLiveKnowledgePage).mock.calls[1].slice(0,4)).toEqual(['A','abcdef','rules',{context:selected}]);
 expect(vi.mocked(getLiveKnowledgePage).mock.calls[2].slice(0,4)).toEqual(['A','abcdef','rules',{context:selected}]);
});

it.each(['native','imported'] as const)('shows contextual %s reference links and partial coverage without internal SDK metadata',async kind=>{
 const result=page('alpha','rules','Rules','Unique source body');result.refs.kind=kind;
 result.refs.data.outgoing!.total=9007199254740993n;
 result.refs.data.outgoing!.complete=false;result.refs.data.outgoing!.next_cursor={opaque:[9007199254740995n,'opaque-token']};
 result.refs.data.warnings=[{coverage:'partial projection'}];
 vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(result).mockResolvedValueOnce(page('alpha','Target','Target','Referenced page'));
 const user=userEvent.setup();render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));await screen.findByText('Unique source body');
 expect(screen.getAllByText('Unique source body')).toHaveLength(1);
 expect(screen.getByText(/9007199254740993 references/).textContent).toContain('More pages are available.');
 expect(screen.queryByText(/opaque-token|compilation_digest|witness|knowledge_source_complete/)).toBeNull();
 expect(screen.getByText('Partial reference view')).toBeTruthy();
 await user.click(screen.getByRole('button',{name:/^Target/}));await screen.findByText('Referenced page');
 expect(getLiveKnowledgePage).toHaveBeenLastCalledWith('A','abcdef','Target',{reference:{namespace:referenceSource.namespace!,witness:null}},expect.any(AbortSignal));
});

it('resolves another namespace only on click and preserves the reference focus through candidate selection',async()=>{
 const result=page(),edge=result.refs.data.outgoing!.results[0];
 const namespace={scope:{project:'other-project'},context:'other-context'},reference={namespace,witness:null};
 if('authored' in edge.payload) edge.payload.authored.target.namespace=namespace;
 const destination=page('beta','Target','Target from beta','Foreign selected page');destination.refs.data.focused_source!.namespace=namespace;
 vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(result).mockResolvedValueOnce({generation:'abcdef',candidates:[candidate(destination)]}).mockResolvedValueOnce(destination);
 const user=userEvent.setup();render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));await screen.findByText('Source');
 expect(getLiveKnowledgePage).toHaveBeenCalledOnce();
 await user.click(screen.getByRole('button',{name:/^Target/}));
 expect(getLiveKnowledgePage).toHaveBeenNthCalledWith(2,'A','abcdef','Target',{reference},expect.any(AbortSignal));
 await user.click(await screen.findByRole('button',{name:'Target from beta · '+origin('beta')}));
 await screen.findByText('Foreign selected page');
 expect(getLiveKnowledgePage).toHaveBeenNthCalledWith(3,'A','abcdef','Target',{context:selection('beta'),reference},expect.any(AbortSignal));
});

it('sends the observed incoming source namespace and witness when its link is opened',async()=>{
 const result=page(),namespace={scope:{project:'source-project'},context:' source namespace '},witness=Array(32).fill(9);
 const edge={...knowledgeEdge(),source:{...referenceSource,slug:'Parent',title:'Parent page',namespace,witness}};
 result.refs.data.incoming={results:[edge],total:1,complete:true,next_cursor:null};
 vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(result).mockResolvedValueOnce(page('beta','Parent','Parent page','Parent source body'));
 const user=userEvent.setup();render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));await screen.findByText('Source');
 expect(getLiveKnowledgePage).toHaveBeenCalledOnce();
 await user.click(screen.getByRole('button',{name:/^Parent page/}));await screen.findByText('Parent source body');
 expect(getLiveKnowledgePage).toHaveBeenLastCalledWith('A','abcdef','Parent',{reference:{namespace,witness}},expect.any(AbortSignal));
});

it('keeps different reference witnesses separate after resolving the same full tuple and page path',async()=>{
 const initial=page(),namespace=referenceSource.namespace!;
 const witnessA=Array(32).fill(3),witnessB=Array(32).fill(4);
 initial.refs.data.incoming={results:[
  {...knowledgeEdge(),source:{...referenceSource,slug:'Parent',title:'Parent A',namespace,witness:witnessA}},
  {...knowledgeEdge(),source:{...referenceSource,slug:'Parent',title:'Parent B',namespace,witness:witnessB}},
 ],total:2,complete:true,next_cursor:null};
 const observedA=page('beta','Parent','Parent A','Observed body A'),observedB=page('beta','Parent','Parent B','Observed body B');
 observedA.refs.data.focused_source!.witness=witnessA;observedB.refs.data.focused_source!.witness=witnessB;
 vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(initial).mockResolvedValueOnce(observedA).mockResolvedValueOnce(observedB);
 const user=userEvent.setup();render(<View content="[[rules]]"/>);
 await user.click(screen.getByRole('button',{name:'rules'}));await screen.findByText('Source');
 await user.click(screen.getByRole('button',{name:/^Parent A/}));await screen.findByText('Observed body A');
 await user.click(screen.getByRole('tab',{name:'Rules'}));
 await user.click(screen.getByRole('button',{name:/^Parent B/}));await screen.findByText('Observed body B');
 expect(screen.getByRole('tab',{name:'Parent B'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.getByRole('tab',{name:'Parent A'})).toBeTruthy();
 expect(getLiveKnowledgePage).toHaveBeenNthCalledWith(2,'A','abcdef','Parent',{reference:{namespace,witness:witnessA}},expect.any(AbortSignal));
 expect(getLiveKnowledgePage).toHaveBeenNthCalledWith(3,'A','abcdef','Parent',{reference:{namespace,witness:witnessB}},expect.any(AbortSignal));
});

it('keeps the latest selection when different source requests finish out of order', async () => {
  const resolve: Array<(value: LiveKnowledgeResult) => void> = [];
  vi.mocked(getLiveKnowledgePage).mockImplementation(() => new Promise(r => resolve.push(r)));
  const user = userEvent.setup(); render(<View />);
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[1]);
  await act(async () => resolve[1](page('beta')));
  await act(async () => resolve[0](page()));
  expect(screen.getByRole('tab', { name: 'Rules · '+origin('beta') }).getAttribute('aria-selected')).toBe('true');
  screen.getByRole('tab', { name: 'Rules · '+origin('beta') }).focus();
  await user.keyboard('{Home}');
  expect(screen.getByRole('tab', { name: 'Agent output' }).getAttribute('aria-selected')).toBe('true');
  await user.keyboard('{End}');
  expect(screen.getByRole('tab', { name: 'Rules · '+origin('beta') }).getAttribute('aria-selected')).toBe('true');
});


it('closes an inactive source directly from its tab without changing the active source', async () => {
  const user = userEvent.setup();
  vi.mocked(getLiveKnowledgePage).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('beta'));
  const {container} = render(<View />);
  await user.click(within(output()).getAllByRole('button', {name:'rules'})[0]);
  await screen.findByText('Source');
  await user.click(screen.getByRole('tab',{name:'Agent output'}));
  await user.click(within(output()).getAllByRole('button', {name:'rules'})[1]);
  const active = await screen.findByRole('tab',{name:'Rules · '+origin('beta')});
  const row = screen.getByRole('tablist', {name:'Run evidence'});
  expect(within(row).getAllByRole('button', {name:/^Close tab/})).toHaveLength(2);
  expect(container.querySelector('button button')).toBeNull();
  await user.click(within(row).getByRole('button',{name:'Close tab Rules · '+origin('alpha')}));
  expect(screen.getAllByRole('tab')).toHaveLength(3);
  expect(active.getAttribute('aria-selected')).toBe('true');
  expect(document.activeElement).toBe(active);
  expect((screen.getByLabelText('Saved draft') as HTMLInputElement).value).toBe('continue here');
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('button',{name:'Close tab Rules'}));
  await user.keyboard('{Enter}');
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(document.activeElement).toBe(screen.getByRole('tab',{name:'Agent output'}));
});
