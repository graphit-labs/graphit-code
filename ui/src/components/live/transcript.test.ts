import { expect, it } from 'vitest';
import { transcriptFromEvents } from './LiveSearchPage';
import type { LiveEvent, LiveEventData } from '@/api/live';
const event = (sequence: number, data: LiveEventData, turn_id = 'turn1'): LiveEvent => ({ sequence: String(sequence), turn_id, data });
it('retains public CLI diagnostics and tools beside the answer',()=>{
  const [turn]=transcriptFromEvents([
    event(1,{type:'prompt',text:'prompt'}),event(2,{type:'thinking',text:'thinking'}),event(3,{type:'stdout',text:'stdout'}),event(4,{type:'stderr',text:'stderr'}),
    event(5,{type:'tool_use',tool_call_id:'call',name:'read',title:'Read',input:{path:'guide'}}),event(6,{type:'tool_result',tool_call_id:'call',output:'body',is_error:false}),
    event(7,{type:'text',text:'text'}),event(8,{type:'turn_done',outcome:'completed',joined:true,clean:true}),
  ]);
  expect(turn.activity.map(e=>e.kind)).toEqual(['thinking','stdout','stderr','tool_use','tool_result']);
  expect(turn.answer).toBe('text');expect(turn.done).toBe(true);
});

it('preserves preparation labels and tool identities without rewriting kinds', () => {
 const events = [
  event(1,{type:'prep',message:'Creating ephemeral project...',artifact:null}),
  event(2,{type:'prep',message:'Installing skills...',artifact:null}),
  event(3,{type:'tool_use',name:'read',title:'Read',tool_call_id:'call-1',input:'path'}),
 ];
 expect(transcriptFromEvents(events)[0].activity).toEqual([
  {kind:'prep',text:'Creating ephemeral project...'}, {kind:'prep',text:'Installing skills...'},
  {kind:'tool_use',tool:'read',tool_call_id:'call-1',detail:'path'},
 ]);
});

it('does not regress the current answer status when empty progress or stderr arrives', () => {
 const events=[
  event(1,{type:'tool_use',name:'read',title:'Read',tool_call_id:'call',input:null}),
  event(2,{type:'text',text:'Answer'}),
  event(3,{type:'stderr',text:'diagnostic'}),
  event(4,{type:'thinking',text:''}),
 ];
 const [turn]=transcriptFromEvents(events);
 expect(turn.currentEvent).toEqual({kind:'text',text:'Answer'});
 expect(turn.activity).toContainEqual({kind:'stderr',text:'diagnostic'});
});

it('keeps explicit turn identities and tool payload integers without cross-turn pairing', () => {
 const turns=transcriptFromEvents([
  event(1,{type:'tool_use',tool_call_id:'same-call',name:null,title:'Read first',input:{pin:18446744073709551615n}},'first'),
  event(2,{type:'tool_use',tool_call_id:'same-call',name:'read',title:'Read second',input:'second'},'second'),
  event(3,{type:'tool_result',tool_call_id:'same-call',output:'first result',is_error:true},'first'),
  event(4,{type:'turn_done',outcome:'failed',joined:true,clean:false},'first'),
 ]);
 expect(turns.map(t=>t.id)).toEqual(['first','second']);
 expect(turns[0].activity[0].detail).toContain('18446744073709551615');
 expect(turns[0].activity[1].detail).toBe('Tool error:\nfirst result');
 expect(turns[0].outcome).toBe('failed');expect(turns[1].done).toBe(false);
 expect(turns[1].activity).toHaveLength(1);
});
