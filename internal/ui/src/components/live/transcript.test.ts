import { expect, it } from 'vitest';
import { transcriptFromEvents } from './LiveSearchPage';
it('retains public CLI diagnostics and tools beside the answer',()=>{
  const kinds=['prompt','thinking','stdout','stderr','tool_use','tool_result','text','turn_done'] as const;
  const [turn]=transcriptFromEvents(kinds.map((kind,i)=>({seq:i+1,kind,text:kind,at:'now'})));
  expect(turn.activity.map(e=>e.label)).toEqual(['thinking','stdout','stderr','tool_use','tool_result']);
  expect(turn.answer).toBe('text');expect(turn.done).toBe(true);
});
