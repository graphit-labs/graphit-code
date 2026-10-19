import {beforeEach,expect,it,vi} from 'vitest';
import {nativeAstApi,astSelection} from '@/api/nativeAst';
import {nativeSampleView} from './nativeGraphView';
import {nativeSchema,nativeSample,neighborhoodPage,selection,generation,installed,searchReply,resolvedQuery} from './nativeAst.test-fixtures';
import {loadNativeNeighborhood} from './neighborhood';
vi.mock('@/api/nativeAst',async original=>({...await original<typeof import('@/api/nativeAst')>(),nativeAstApi:{neighborhood:vi.fn(),query:vi.fn(),importedNeighborhood:vi.fn(),importedQuery:vi.fn()}}));
beforeEach(()=>{vi.resetAllMocks();vi.mocked(nativeAstApi.neighborhood).mockResolvedValue(neighborhoodPage());vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(neighborhoodPage());vi.mocked(nativeAstApi.importedQuery).mockResolvedValue(resolvedQuery);});

it('preserves parallel native edges even when published relationship UIDs collide',async()=>{
 const graph=await loadNativeNeighborhood(nativeSampleView(nativeSample,nativeSchema).nodes[0],nativeSchema,installed);
 expect(graph.links).toHaveLength(2);expect(new Set(graph.links.map(edge=>edge.id)).size).toBe(2);
 expect(graph.links.map(edge=>edge.uid)).toEqual(['same-edge-uid','same-edge-uid']);
 expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledTimes(2);expect(nativeAstApi.neighborhood).not.toHaveBeenCalled();
});
it('rejects a cursor captured from another literal installed version',async()=>{
 const node=nativeSampleView(nativeSample,nativeSchema).nodes[0];const first=await loadNativeNeighborhood(node,nativeSchema,installed);
 vi.mocked(nativeAstApi.importedNeighborhood).mockClear();
 await expect(loadNativeNeighborhood(node,nativeSchema,{...installed,version:'different'},first)).rejects.toThrow('working context changed');
 expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('does not invalidate a captured page merely because schema metadata was refreshed',async()=>{
 const node=nativeSampleView(nativeSample,nativeSchema).nodes[0];const first=await loadNativeNeighborhood(node,nativeSchema,installed);
 vi.mocked(nativeAstApi.importedNeighborhood).mockClear();
 const next=await loadNativeNeighborhood(node,{...nativeSchema,generation:[...generation].reverse()},installed,first);
 expect(next.nodes).toEqual(first.nodes);
 expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('uses an observed pathless SCIP primary key and retries only the failed partition',async()=>{
 const base=nativeSampleView(nativeSample,nativeSchema).nodes[0];const external={...base,file:undefined,properties:{uid:'scip:os/exec.Cmd#Run().'},nativeKey:{type:'string' as const,value:'scip:observed-key'}};
 const page=neighborhoodPage();page.nodes[0].properties=page.nodes[0].properties.filter(prop=>prop.name!=='path').map(prop=>prop.name==='entity_key'?{...prop,value:external.nativeKey}:prop.name==='uid'?{...prop,value:{type:'string' as const,value:external.properties.uid}}:prop);
 vi.mocked(nativeAstApi.importedNeighborhood).mockImplementation(async(_source,input)=>{if(input.direction==='outgoing')throw new Error('Temporary index failure');return page;});
 const first=await loadNativeNeighborhood(external,nativeSchema,installed);expect(first.partitions.filter(p=>p.error)).toHaveLength(1);
 for(const [,input] of vi.mocked(nativeAstApi.importedNeighborhood).mock.calls)expect(input.anchor_key).toEqual(external.nativeKey);
 vi.mocked(nativeAstApi.importedNeighborhood).mockClear().mockResolvedValue(page);
 const second=await loadNativeNeighborhood(external,nativeSchema,installed,first);expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledTimes(1);expect(second.partitions.every(p=>!p.error)).toBe(true);
});
it('stops before issuing any request when cancelled before selection',async()=>{
 const controller=new AbortController();controller.abort();
 await expect(loadNativeNeighborhood(nativeSampleView(nativeSample,nativeSchema).nodes[0],nativeSchema,installed,undefined,controller.signal)).rejects.toThrow();
 expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();
});
it('resolves search observations to actual SDK internal identity before native pagination',async()=>{
 const hit=searchReply.results[0];const node={id:'renderer-only',label:hit.kind,type:hit.kind,name:hit.name,properties:{...hit}};
 const graph=await loadNativeNeighborhood(node,nativeSchema,installed);
 expect(nativeAstApi.importedQuery).toHaveBeenCalledWith(installed,'MATCH (n:Function) WHERE n.entity_key = $observed_key RETURN n LIMIT 2',{observed_key:hit.entity_key},undefined);
 expect(graph.anchor.nativeIdentity).toBeTruthy();expect(graph.anchor.id).not.toBe(node.id);
});
it('keeps native direction cursors literal and rejects reuse from another anchor', async () => {
 const cursor = { generation, fingerprint: [...generation].reverse(), last_uid: ' literal/?uid ' };
 vi.mocked(nativeAstApi.neighborhood).mockImplementation(async (_selection, input) => ({ ...neighborhoodPage(), next_cursor: input.direction === 'outgoing' && !input.cursor ? cursor : null }));
 const nodes = nativeSampleView(nativeSample,nativeSchema).nodes;
 const first = await loadNativeNeighborhood(nodes[0],nativeSchema,astSelection(selection));
 expect(first.links).toHaveLength(2);
 await expect(loadNativeNeighborhood(nodes[1],nativeSchema,astSelection(selection),first)).rejects.toThrow('another entity');
 vi.mocked(nativeAstApi.neighborhood).mockClear();
 const next = await loadNativeNeighborhood(nodes[0],nativeSchema,astSelection(selection),first);
 expect(nativeAstApi.neighborhood).toHaveBeenCalledTimes(1);
 expect(nativeAstApi.neighborhood).toHaveBeenCalledWith(astSelection(selection),expect.objectContaining({ direction: 'outgoing', anchor_key: nodes[0].nativeKey, cursor }),undefined);
 expect(next.partitions.every(p => !p.more)).toBe(true);
});
it('adopts the real first-page anchor after a new publication without retaining an older snapshot', async () => {
 const node = nativeSampleView(nativeSample,nativeSchema).nodes[0];
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValue({ ...neighborhoodPage(), generation: [...generation].reverse() });
 const graph=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection));
 expect(graph.anchor.id).not.toBe(node.id);
 expect(graph.nodes).toHaveLength(2);
 expect(graph.nodes.every(n=>n.nativeIdentity?.generation.every((byte,i)=>byte===[...generation].reverse()[i]))).toBe(true);
 expect(nativeAstApi.neighborhood).toHaveBeenCalledTimes(2);
 await expect(loadNativeNeighborhood({ ...node, nativeKey: undefined, properties: {} },nativeSchema,astSelection(selection))).rejects.toThrow('primary key is unavailable');
});
it('joins independent relationship queries by public identity as the graph advances', async () => {
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValueOnce(neighborhoodPage()).mockResolvedValueOnce({...neighborhoodPage(),generation:[...generation].reverse()});
 const graph=await loadNativeNeighborhood(nativeSampleView(nativeSample,nativeSchema).nodes[0],nativeSchema,astSelection(selection));
 expect(graph.nodes).toHaveLength(2);
 expect(graph.links).toHaveLength(4);
 expect(graph.partitions.every(partition=>!partition.error)).toBe(true);
 expect(graph.links.every(edge=>graph.nodes.some(node=>node.id===edge.source)&&graph.nodes.some(node=>node.id===edge.target))).toBe(true);
});
it('keeps a real cursor bound to its captured generation when a subsequent page advances', async () => {
 const node=nativeSampleView(nativeSample,nativeSchema).nodes[0];
 const cursor={generation,fingerprint:[...generation].reverse(),last_uid:' literal/?uid '};
 vi.mocked(nativeAstApi.neighborhood).mockImplementation(async(_selection,input)=>({...neighborhoodPage(),next_cursor:input.direction==='outgoing'?cursor:null}));
 const first=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection));
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValue({...neighborhoodPage(),generation:[...generation].reverse()});
 await expect(loadNativeNeighborhood(first.anchor,nativeSchema,astSelection(selection),first)).rejects.toThrow('graph changed');
});
it.each(['entity_key','uid'] as const)('rejects a first page that changes the observed %s', async name => {
 const page=neighborhoodPage();page.nodes[0].properties=page.nodes[0].properties.map(prop=>prop.name===name?{...prop,value:{type:'string' as const,value:'another-entity'}}:prop);
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValue(page);
 await expect(loadNativeNeighborhood(nativeSampleView(nativeSample,nativeSchema).nodes[0],nativeSchema,astSelection(selection))).rejects.toThrow('anchor changed');
});
it('settles every accepted native request when an abort arrives and preserves its signal', async () => {
 const controller = new AbortController(); const signals: AbortSignal[] = [];
 vi.mocked(nativeAstApi.neighborhood).mockImplementation((_selection,_input,signal) => new Promise((_resolve,reject) => { signals.push(signal!); signal!.addEventListener('abort', () => reject(new DOMException('cancelled','AbortError')), { once: true }); }));
 const running = loadNativeNeighborhood(nativeSampleView(nativeSample,nativeSchema).nodes[0],nativeSchema,astSelection(selection),undefined,controller.signal);
 controller.abort(); await expect(running).rejects.toMatchObject({ name: 'AbortError' });
 expect(signals).toHaveLength(2); expect(signals.every(signal => signal === controller.signal && signal.aborted)).toBe(true);
});
