import { afterEach, expect, it, vi } from 'vitest';
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


afterEach(() => vi.unstubAllGlobals());
it('keeps server validation errors returned before the stream starts', async () => {
 await expect(readAgentStream(new Response('{"error":"Agent request body exceeds 1 MiB"}', { status: 413 }))).rejects.toThrow('Agent request body exceeds 1 MiB');
 await expect(readAgentStream(new Response('<html>bad gateway</html>', { status: 502 }))).rejects.toThrow('HTTP 502');
});
it('sends the selected project and complete question to both agent endpoints', async () => {
 const fetchMock = vi.fn().mockImplementation(async () => new Response('event: final\ndata: {"answer":"ok","cypher":"RETURN 1"}\n\n',{headers:{'Content-Type':'text/event-stream'}}));
 vi.stubGlobal('fetch',fetchMock);
 const {aiSearchWiki} = await import('./wiki');
 const {astApi} = await import('./ast');
 const controller = new AbortController();
 await aiSearchWiki('/wiki/selected', 'como funciona?', '/project/selected', {signal:controller.signal});
 await astApi.generateCypher('find functions', 'shared/code', '/project/selected', {signal:controller.signal});
 expect(fetchMock).toHaveBeenNthCalledWith(1,'/api/wiki/ai-search',expect.objectContaining({method:'POST',signal:controller.signal,body:JSON.stringify({dir:'/wiki/selected',query:'como funciona?',project_dir:'/project/selected'})}));
 expect(fetchMock).toHaveBeenNthCalledWith(2,'/api/generate-cypher',expect.objectContaining({method:'POST',signal:controller.signal,body:JSON.stringify({query:'find functions',context:'shared/code',project_dir:'/project/selected'})}));
});
