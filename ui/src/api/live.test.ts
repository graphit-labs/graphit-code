import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cancelLiveTurn, closeLiveSession, createLiveSession, getLiveSession, getLiveKnowledgePage, listLiveSessions, removeLiveSession, sendLiveMessage, subscribeLiveEvents, type LiveCursor, type LiveSession, type LiveTerminal, type LiveKnowledgePage, type LiveArtifactSelection } from './live'
import { stringifyNativeJson } from './nativeJson'
import { referenceSource, knowledgeEdge } from './nativeRecordReferences.test-fixtures'

const saved = (state: LiveSession['state'] = 'running', sequence = '9007199254740992'): LiveSession => ({
  id: 'saved/session', project: { project_id: 'p', project_version: 'v1' }, binding: 'codex',
  agent: { source: 'hub', project_id: null, instance_id: null, project_kind: null, type: 'agent', id: 'agent', version: 'v2' },
  artifacts: [], title: 'Saved', model: 'model', state, cursor: { generation: 'abcdef', sequence },
  created_at: '2026-10-15T12:00:00Z', updated_at: '2026-10-15T12:00:01Z',
})
const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status })
const terminal: LiveTerminal = { turn_id: 'turn', outcome: 'failed', error: 'storage', joined: true, clean: true, persisted: false }
const knowledgeContext: LiveArtifactSelection = {source:'hub',project_id:'producer',instance_id:'install',project_kind:'local',type:'knowledge',id:'guide',version:' literal/v2 '}
const knowledgePage = (): LiveKnowledgePage => {
 const focused={...referenceSource,slug:'Guide',source_path:'docs/guide.md'}
 return {title:'Guide',path:'Guide',source_path:focused.source_path,content:'Only one body',context:knowledgeContext,generation:'abcdef',refs:{kind:'imported',data:{focused_source:focused,outgoing:{results:[{...knowledgeEdge(),source:focused}],total:9007199254740993n,complete:false,next_cursor:{opaque:[9007199254740995n,'cursor'] }},incoming:null,coverage:{knowledge_source_complete:false},warnings:[{partial:'limited'}]}}}
}

it.each(['native','imported'] as const)('reads the pinned live page with literal path and complete context, preserving %s SDK references losslessly',async kind=>{
 const reply=knowledgePage(),abort=new AbortController();reply.refs.kind=kind
 const fetchMock=vi.fn().mockResolvedValue(new Response(stringifyNativeJson(reply)));vi.stubGlobal('fetch',fetchMock)
 expect(await getLiveKnowledgePage('saved/session','abcdef','literal: Guide/?#',{context:knowledgeContext},abort.signal)).toEqual(reply)
 expect(fetchMock).toHaveBeenCalledOnce()
 const [url,options]=fetchMock.mock.calls[0],parsed=new URL(url,'http://localhost')
 expect(parsed.pathname).toBe('/api/live/sessions/saved%2Fsession/knowledge/page')
 expect([...parsed.searchParams.keys()]).toEqual(['generation','page','context'])
 expect(parsed.searchParams.get('generation')).toBe('abcdef');expect(parsed.searchParams.get('page')).toBe('literal: Guide/?#')
 expect(JSON.parse(parsed.searchParams.get('context')!)).toEqual(knowledgeContext)
 expect(options).toEqual({credentials:'same-origin',signal:abort.signal})
})
it('omits absent context and preserves the native ambiguity candidates',async()=>{
 const page=knowledgePage(),reply={generation:'abcdef',candidates:[{title:page.title,path:page.path,source_path:page.source_path,context:page.context}]}
 const fetchMock=vi.fn().mockResolvedValue(json(reply));vi.stubGlobal('fetch',fetchMock)
 expect(await getLiveKnowledgePage('saved','abcdef','Guide')).toEqual(reply)
 expect(new URL(fetchMock.mock.calls[0][0],'http://localhost').searchParams.has('context')).toBe(false)
})
it.each([[404,'not_found'],[403,'permission_denied'],[409,'context_changed']] as const)('preserves the actual viewer failure %s %s without another source request',async(status,code)=>{
 const fetchMock=vi.fn().mockResolvedValue(json({error:{code}},status));vi.stubGlobal('fetch',fetchMock)
 await expect(getLiveKnowledgePage('saved','abcdef','Guide',{context:knowledgeContext})).rejects.toMatchObject({status,code})
 expect(fetchMock).toHaveBeenCalledOnce()
})
it('rejects a page from another session generation without adopting it',async()=>{
 vi.stubGlobal('fetch',vi.fn().mockResolvedValue(json({...knowledgePage(),refs:{kind:'native',data:{focused_source:null,outgoing:null,incoming:null,coverage:{},warnings:[]}},generation:'abcdee'})))
 await expect(getLiveKnowledgePage('saved','abcdef','Guide')).rejects.toMatchObject({status:null,code:'invalid_live_response'})
})
it.each([{kind:'outgoing',witness:null},{kind:'incoming',witness:Array(32).fill(7)}])('sends the literal $kind SDK reference focus and preserves an explicit candidate selection',async({witness})=>{
 const reference={namespace:{scope:{project:' literal namespace '},context:' literal/context '},witness}
 const query={reference,...(witness ? {context:knowledgeContext} : {})}
 const fetchMock=vi.fn().mockResolvedValue(new Response(stringifyNativeJson(knowledgePage())));vi.stubGlobal('fetch',fetchMock)
 await getLiveKnowledgePage('saved','abcdef',' literal:Page ',query)
 const url=new URL(fetchMock.mock.calls[0][0],'http://localhost')
 expect(JSON.parse(url.searchParams.get('reference')!)).toEqual(reference)
 expect(url.searchParams.get('generation')).toBe('abcdef');expect(url.searchParams.get('page')).toBe(' literal:Page ')
 expect(url.searchParams.has('context')).toBe(witness!==null)
 if(witness) expect(JSON.parse(url.searchParams.get('context')!)).toEqual(knowledgeContext)
 expect(fetchMock).toHaveBeenCalledOnce()
})
class Source extends EventTarget {
  static CLOSED = 2
  static all: Source[] = []
  readyState = 0
  close = vi.fn()
  onerror: ((event: Event) => unknown) | null = null
  constructor(readonly url: string, readonly options?: EventSourceInit) { super(); Source.all.push(this) }
  send(type: string, value: unknown, lastEventId = '') {
    const frame = new MessageEvent(type, { data: JSON.stringify(value), lastEventId })
    if (type === 'error') this.onerror?.(frame)
    else this.dispatchEvent(frame)
  }
  disconnect() { this.onerror?.(new Event('error')) }
  opened(after: LiveCursor, head = '18446744073709551615') { this.send('opened', { session: saved('running', head), after, terminal: null }) }
}
beforeEach(() => { Source.all = []; vi.useFakeTimers(); vi.stubGlobal('EventSource', Source) })
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals() })

it('reads native list/detail envelopes with exact cursors, timestamps and abort signals', async () => {
  const session = saved(), abort = new AbortController()
  const fetchMock = vi.fn().mockResolvedValueOnce(json({ sessions: [session] })).mockResolvedValueOnce(json({ session, events: [], terminal }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await listLiveSessions(abort.signal)).toEqual([session])
  expect(await getLiveSession(session.id, abort.signal)).toEqual({ session, events: [], terminal })
  expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/live/sessions/saved%2Fsession', { credentials: 'same-origin', signal: abort.signal })
})

it('creates with the closed native request and an explicit model without legacy scope query fields', async () => {
  const accepted={...saved('ready'),id:'request-id'}
  const request={request_id:'request-id',project:accepted.project,binding:'kiro-v3',agent:accepted.agent,artifacts:[],model:'explicit-model',title:null,prompt:'Initial question'}
  const fetchMock=vi.fn().mockResolvedValue(json(accepted));vi.stubGlobal('fetch',fetchMock)
  expect(await createLiveSession(request)).toEqual(accepted)
  expect(fetchMock).toHaveBeenCalledTimes(1); expect(fetchMock).toHaveBeenCalledWith('/api/live/sessions',expect.objectContaining({method:'POST',credentials:'same-origin',headers:{'Content-Type':'application/json','X-Graphit-Request':'ui'},body:JSON.stringify(request)}))
})

it('sends the exact native turn id and text and retains the authoritative raw session acknowledgement', async () => {
  const accepted=saved('ready'),request={turn_id:'turn-id',text:'Follow-up question'}
  const fetchMock=vi.fn().mockResolvedValue(json(accepted));vi.stubGlobal('fetch',fetchMock)
  expect(await sendLiveMessage(accepted.id,request)).toEqual(accepted)
  expect(fetchMock).toHaveBeenCalledTimes(1); expect(fetchMock).toHaveBeenCalledWith('/api/live/sessions/saved%2Fsession/messages',expect.objectContaining({method:'POST',body:JSON.stringify(request)}))
})

it('keeps cancel acknowledgement, close and delete envelopes distinct', async () => {
  const fetchMock = vi.fn().mockResolvedValueOnce(json({ requested: false })).mockResolvedValueOnce(json({ session: saved('closed') })).mockResolvedValueOnce(json({ deleted: true }))
  vi.stubGlobal('fetch', fetchMock)
  expect(await cancelLiveTurn('saved/session')).toEqual({ requested: false })
  expect(await closeLiveSession('saved/session')).toEqual(saved('closed'))
  expect(await removeLiveSession('saved/session')).toEqual({ deleted: true })
  expect(fetchMock.mock.calls.map(([url, init]) => [url, init.method])).toEqual([
    ['/api/live/sessions/saved%2Fsession/cancel', 'POST'], ['/api/live/sessions/saved%2Fsession/close', 'POST'], ['/api/live/sessions/saved%2Fsession', 'DELETE'],
  ])
  for (const [, init] of fetchMock.mock.calls) expect(init).toMatchObject({ credentials: 'same-origin', headers: { 'X-Graphit-Request': 'ui' } })
})

it('preserves public HTTP cause and journal effect without displaying raw response bodies', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(json({ error: { code: 'context_changed', journal_effect: 'Changed' } }, 409)).mockResolvedValueOnce(new Response('private upstream body', { status: 503 })))
  await expect(closeLiveSession('saved/session')).rejects.toMatchObject({ status: 409, code: 'context_changed', journalEffect: 'Changed' })
  await expect(listLiveSessions()).rejects.toMatchObject({ status: 503, code: null, message: 'Live request failed (HTTP 503)' })
})

it('replays lossless sequences, deduplicates and reconnects using only the last accepted query cursor', () => {
  const after = { generation: 'abcdef', sequence: '9007199254740992' }
  const onEvent = vi.fn(), onError = vi.fn(), onTerminal = vi.fn(), onClosed = vi.fn()
  subscribeLiveEvents('saved/session', after, { onEvent, onError, onTerminal, onClosed })
  const first = Source.all[0]
  expect(first.options).toEqual({ withCredentials: true })
  first.opened(after)
  first.dispatchEvent(new MessageEvent('event', {data:'{"sequence":"9007199254740993","turn_id":"turn","data":{"type":"tool_use","tool_call_id":"call","name":"read","title":"Read","input":{"pin":18446744073709551615}}}',lastEventId:'abcdef:9007199254740993'}))
  expect(onEvent.mock.calls[0][0].data.input.pin).toBe(18446744073709551615n)
  first.send('event', { sequence: '9007199254740993', turn_id: 'turn', data: { type: 'text', text: 'duplicate' } }, 'abcdef:9007199254740993')
  expect(onEvent).toHaveBeenCalledOnce()
  first.disconnect(); first.disconnect()
  expect(first.close).toHaveBeenCalledOnce(); expect(onError).toHaveBeenCalledOnce()
  vi.advanceTimersByTime(1000)
  expect(Source.all).toHaveLength(2)
  const second = Source.all[1]
  expect(second.url).toBe('/api/live/sessions/saved%2Fsession/events?generation=abcdef&sequence=9007199254740993')
  second.opened({ ...after, sequence: '9007199254740993' })
  second.send('terminal', terminal)
  expect(onTerminal).toHaveBeenCalledWith(terminal)
  expect(second.close).not.toHaveBeenCalled()
  second.send('event', { sequence: '9007199254740994', turn_id: 'next', data: { type: 'text', text: 'Next turn' } }, 'abcdef:9007199254740994')
  expect(onEvent).toHaveBeenCalledTimes(2)
  second.send('closed', saved('closed', '9007199254740994'))
  expect(onClosed).toHaveBeenCalledOnce(); expect(second.close).toHaveBeenCalledOnce()
  second.disconnect(); vi.advanceTimersByTime(10000)
  expect(Source.all).toHaveLength(2)
})

it('keeps a future cursor separate from the observed head and cancels pending retry on reader close', () => {
  const after = { generation: 'abcdef', sequence: '18446744073709551615' }, onEvent = vi.fn()
  const reader = subscribeLiveEvents('saved/session', after, { onEvent })
  const source = Source.all[0]; source.opened(after, '1'); source.disconnect()
  reader.close(); vi.advanceTimersByTime(5000)
  expect(Source.all).toHaveLength(1); expect(onEvent).not.toHaveBeenCalled()
})

it('treats typed SSE error as fatal and retains the actual public cause without a retry', () => {
  const onError = vi.fn(), reader = subscribeLiveEvents('saved/session', saved().cursor, { onEvent: vi.fn(), onError })
  const source = Source.all[0]; source.opened(saved().cursor)
  source.send('error', { error: { code: 'permission_denied', journal_effect: 'Unchanged' } })
  source.disconnect(); vi.advanceTimersByTime(5000)
  expect(onError).toHaveBeenCalledOnce()
  expect(onError.mock.calls[0][0]).toMatchObject({ status: null, code: 'permission_denied', journalEffect: 'Unchanged' })
  expect(Source.all).toHaveLength(1); reader.close()
})

it('does not revive an EventSource that the browser marked permanently closed', () => {
  const onError=vi.fn()
  subscribeLiveEvents('saved/session',saved().cursor,{onEvent:vi.fn(),onError})
  const source=Source.all[0];source.readyState=Source.CLOSED;source.disconnect()
  vi.advanceTimersByTime(5000)
  expect(Source.all).toHaveLength(1)
  expect(onError.mock.calls[0][0]).toMatchObject({status:null,code:'stream_closed'})
})

it.each([0, '01', '-1', '18446744073709551616'])('rejects a noncanonical or overflowing sequence %s before subscribing', sequence => {
  expect(() => subscribeLiveEvents('saved/session', { generation: 'abcdef', sequence: sequence as string }, { onEvent: vi.fn() })).toThrow('invalid_live_response')
  expect(Source.all).toHaveLength(0)
})

it('rejects a foreign generation frame and late callbacks after reader teardown', () => {
  const onError = vi.fn(), onEvent = vi.fn(), after = saved().cursor
  const reader = subscribeLiveEvents('saved/session', after, { onEvent, onError })
  const source = Source.all[0]; source.opened(after)
  source.send('event', { sequence: '9007199254740993', turn_id: 'turn', data: { type: 'text', text: 'Foreign' } }, 'bad:9007199254740993')
  expect(onError.mock.calls[0][0]).toMatchObject({ code: 'invalid_live_response' })
  reader.close(); source.send('terminal', terminal); source.disconnect(); vi.advanceTimersByTime(5000)
  expect(onEvent).not.toHaveBeenCalled(); expect(Source.all).toHaveLength(1)
})
