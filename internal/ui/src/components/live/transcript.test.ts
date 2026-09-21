import { expect, it } from 'vitest';
import { transcriptFromEvents } from './LiveSearchPage';
it('retains public CLI diagnostics and tools beside the answer',()=>{
  const kinds=['prompt','thinking','stdout','stderr','tool_use','tool_result','text','turn_done'] as const;
  const [turn]=transcriptFromEvents(kinds.map((kind,i)=>({seq:i+1,kind,text:kind,at:'now'})));
  expect(turn.activity.map(e=>e.kind)).toEqual(['thinking','stdout','stderr','tool_use','tool_result']);
  expect(turn.answer).toBe('text');expect(turn.done).toBe(true);
});

it('preserves preparation labels and tool identities without rewriting kinds', () => {
 const events = [
  {seq:1,kind:'prep' as const,text:'Creating ephemeral project...',at:''},
  {seq:2,kind:'prep' as const,text:'Installing skills...',at:''},
  {seq:3,kind:'tool_use' as const,tool:'read',tool_call_id:'call-1',detail:'path',at:''},
 ];
 expect(transcriptFromEvents(events)[0].activity).toEqual(events);
});

it('does not regress the current answer status when empty progress or stderr arrives', () => {
 const events=[
  {seq:1,kind:'tool_use' as const,tool:'read',at:''},
  {seq:2,kind:'text' as const,text:'Answer',at:''},
  {seq:3,kind:'stderr' as const,text:'diagnostic',at:''},
  {seq:4,kind:'thinking' as const,text:'',at:''},
 ];
 const [turn]=transcriptFromEvents(events);
 expect(turn.currentEvent).toEqual(events[1]);
 expect(turn.activity).toContain(events[2]);
});
