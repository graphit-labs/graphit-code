import { expect, it } from 'vitest';
import { executionActivity, type ExecutionEvent } from './executionActivityModel';
const use = (id?: string, tool = 'read'): ExecutionEvent => ({kind:'tool_use',tool,tool_call_id:id,detail:`input-${id}`});
const result = (id?: string, tool?: string, detail = ''): ExecutionEvent => ({kind:'tool_result',tool,tool_call_id:id,detail});

it('pairs interleaved calls by native identity, including nameless and empty results', () => {
 const items = executionActivity([use('a'),use('b'),result('b',undefined,'B'),result('a')]);
 expect(items).toHaveLength(2);
 expect(items[0]).toMatchObject({type:'tool',call:{tool_call_id:'a'},result:{tool_call_id:'a',detail:''}});
 expect(items[1]).toMatchObject({call:{tool_call_id:'b'},result:{detail:'B'}});
});
it('merges completed snapshots and keeps identities separate across turns', () => {
 const items = executionActivity([use('a'),use('a'),result('a'),{kind:'turn_done'},{kind:'prompt'},use('a'),result('a')]);
 expect(items.filter(i=>i.type==='tool')).toHaveLength(2);
});
it('uses legacy pairing only for one pending unidentified call of the same name', () => {
 expect(executionActivity([use(),result(undefined,'read')],true)).toHaveLength(1);
 expect(executionActivity([use(),result(undefined,'read')])).toHaveLength(2);
 const ambiguous=executionActivity([use(),use(),result(undefined,'read'),result(undefined,'read')],true);
 expect(ambiguous).toHaveLength(4);
 expect(ambiguous[2]).toMatchObject({result:{tool:'read'}});
 expect(ambiguous[2]).not.toHaveProperty('call');
});
it('does not infer identity for a nameless or mismatched result', () => {
 expect(executionActivity([use(),result()],true)).toHaveLength(2);
 expect(executionActivity([use('a'),result('b','read')],true)).toHaveLength(2);
 expect(executionActivity([use('a'),result(undefined,'read')],true)).toHaveLength(2);
});
it('resolves an identified result received before its call without mixing turns', () => {
 const items=executionActivity([result('a'),use('a'),{kind:'prompt'},result('a')]);
 expect(items).toHaveLength(2);
 expect(items[0]).toHaveProperty('call'); expect(items[1]).not.toHaveProperty('call');
});
it('uses all available pending calls before deciding legacy ambiguity', () => {
 const events=[use(),...Array.from({length:350},()=>({kind:'stdout',text:'log'})),use(),result(undefined,'read')];
 expect(executionActivity(events,true).at(-1)).not.toHaveProperty('call');
});
