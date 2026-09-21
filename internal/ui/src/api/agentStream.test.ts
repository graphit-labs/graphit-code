import { expect, it, vi } from 'vitest';
import { readAgentStream } from './agentStream';

it('delivers progress before final across split UTF-8 and CRLF chunks', async () => {
 let controller!: ReadableStreamDefaultController<Uint8Array>;
 const body = new ReadableStream<Uint8Array>({start(c) {controller=c;}}), progress=vi.fn(), encode=new TextEncoder();
 const result=readAgentStream<{answer:string}>(new Response(body,{headers:{'Content-Type':'text/event-stream'}}),progress);
 const bytes=encode.encode('event: progress\r\ndata: {"kind":"stderr","text":"ação"}\r\n\r\n');
 for (const b of bytes) controller.enqueue(new Uint8Array([b]));
 await vi.waitFor(()=>expect(progress).toHaveBeenCalledWith({kind:'stderr',text:'ação'}));
 controller.enqueue(encode.encode('event: final\ndata: {"answer":"done"}\n\n'));controller.close();
 expect(await result).toEqual({answer:'done'});
});
it('rejects explicit errors and truncated streams, accepts legacy JSON',async()=>{
 const stream=(text:string)=>new Response(text,{headers:{'Content-Type':'text/event-stream'}});
 await expect(readAgentStream(stream('event: error\ndata: {"error":"failed"}\n\n'))).rejects.toThrow('failed');
 await expect(readAgentStream(stream('event: done\ndata: {}\n\n'))).rejects.toThrow('without a final');
 expect(await readAgentStream(new Response('{"answer":"json"}'))).toEqual({answer:'json'});
});
