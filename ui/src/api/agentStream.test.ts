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

it('preserves native progress and completion integers and coded HTTP/SSE failures', async () => {
 const progress = vi.fn();
 const frames = 'event: progress\ndata: {"type":"turn.completed","usage":{"input_tokens":18446744073709551615}}\n\nevent: final\ndata: {"status":"answer","receipt":{"completion":{"answer":"literal","ordinal":9007199254740993}}}\n\n';
 const result = await readAgentStream(new Response(frames, {headers:{'Content-Type':'text/event-stream'}}), progress);
 expect(progress).toHaveBeenCalledWith({type:'turn.completed',usage:{input_tokens:18446744073709551615n}});
 expect(result).toEqual({status:'answer',receipt:{completion:{answer:'literal',ordinal:9007199254740993n}}});
 await expect(readAgentStream(new Response('{"error":{"code":"denied"}}',{status:403}))).rejects.toMatchObject({status:403,code:'denied'});
 await expect(readAgentStream(new Response('event: error\ndata: {"error":{"code":"response_limit"}}\n\n',{headers:{'Content-Type':'text/event-stream'}}))).rejects.toThrow('response_limit');
});

it('cancels and releases the stream after a malformed frame', async () => {
 const cancel = vi.fn(), stream = new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new TextEncoder().encode('event: progress\ndata: invalid\n\n'));},cancel});
 await expect(readAgentStream(new Response(stream,{headers:{'Content-Type':'text/event-stream'}}))).rejects.toThrow();
 expect(cancel).toHaveBeenCalledOnce(); expect(stream.locked).toBe(false);
});
it('sends the selected project and complete question to the AST agent endpoint', async () => {
 const fetchMock = vi.fn().mockImplementation(async () => new Response('event: final\ndata: {"answer":"ok","cypher":"RETURN 1"}\n\n',{headers:{'Content-Type':'text/event-stream'}}));
 vi.stubGlobal('fetch',fetchMock);
 const {astApi} = await import('./ast');
 const controller = new AbortController();
 await astApi.generateCypher('find functions', 'shared/code', '/project/selected', {signal:controller.signal});
 expect(fetchMock).toHaveBeenNthCalledWith(1,'/api/generate-cypher',expect.objectContaining({method:'POST',signal:controller.signal,body:JSON.stringify({query:'find functions',context:'shared/code',project_dir:'/project/selected'})}));
});
