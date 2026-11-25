import {beforeEach,expect,it,vi} from 'vitest';
import {nativeAstApi,astSelection} from '@/api/nativeAst';
import {nativeSampleView} from './nativeGraphView';
import {nativeSchema,nativeSample,neighborhoodPage,selection,installed,searchReply,resolvedQuery} from './nativeAst.test-fixtures';
import {loadNativeNeighborhood,resolveNativeNode} from './neighborhood';
vi.mock('@/api/nativeAst',async original=>({...await original<typeof import('@/api/nativeAst')>(),nativeAstApi:{neighborhood:vi.fn(),query:vi.fn(),importedNeighborhood:vi.fn(),importedQuery:vi.fn()}}));
beforeEach(()=>{vi.resetAllMocks();vi.mocked(nativeAstApi.neighborhood).mockResolvedValue(neighborhoodPage());vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(neighborhoodPage());vi.mocked(nativeAstApi.importedQuery).mockResolvedValue(resolvedQuery);});
const anchor=()=>nativeSampleView(nativeSample,nativeSchema).nodes[0];
it('loads one bounded page across both directions and types without schema fan-out or a preliminary query',async()=>{
 const schema={...nativeSchema,relationships:Array.from({length:100},(_,i)=>({...nativeSchema.relationships[0],kind:'REL_'+i}))};
 const node=anchor(),graph=await loadNativeNeighborhood(node,schema,installed);
 expect(graph.links.map(e=>e.uid)).toEqual(['observed-edge-0','observed-edge-1']);
 expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledTimes(1);expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledWith(installed,{anchor_label:node.label,anchor_key:node.nativeKey,limit:100},undefined);
 expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();expect(nativeAstApi.neighborhood).not.toHaveBeenCalled();
});
it('rejects continuation reuse in another exact installed context or entity before dispatch',async()=>{
 vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue({...neighborhoodPage(),next_cursor:{literal:'opaque'}});
 const node=anchor(),first=await loadNativeNeighborhood(node,nativeSchema,installed);vi.mocked(nativeAstApi.importedNeighborhood).mockClear();
 await expect(loadNativeNeighborhood(node,nativeSchema,{...installed,version:'different'},first)).rejects.toThrow('working context changed');
 await expect(loadNativeNeighborhood({...node,nativeKey:{type:'string',value:'another'}},nativeSchema,installed,first)).rejects.toThrow('another entity');
 expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('does not re-read a completed neighborhood when schema metadata is refreshed',async()=>{
 const node=anchor(),first=await loadNativeNeighborhood(node,nativeSchema,installed);vi.mocked(nativeAstApi.importedNeighborhood).mockClear();
 expect(await loadNativeNeighborhood(node,{...nativeSchema,relationships:[]},installed,first)).toBe(first);
 expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('binds continuation to exact logical selection instead of incidental catalogue display fields',async()=>{
 const cursor={fingerprint:[3],position:neighborhoodPage().edges[0].position};
 vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValueOnce({...neighborhoodPage(),next_cursor:cursor});
 const node=anchor(),first=await loadNativeNeighborhood(node,nativeSchema,installed);
 await loadNativeNeighborhood(node,nativeSchema,{id:installed.id,version:installed.version},first);
 expect(nativeAstApi.importedNeighborhood).toHaveBeenLastCalledWith({id:installed.id,version:installed.version},expect.objectContaining({cursor}),undefined);
});
it('passes opaque continuation literally and retains successful evidence through a failed page and retry',async()=>{
 const cursor={fingerprint:[1,7],position:neighborhoodPage().edges[0].position,extra:{exact:9007199254740993n}};
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValueOnce({...neighborhoodPage(),next_cursor:cursor}).mockRejectedValueOnce(new Error('Next page unavailable')).mockResolvedValueOnce(neighborhoodPage());
 const node=anchor(),first=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection)),before=structuredClone(first);
 await expect(loadNativeNeighborhood(node,nativeSchema,astSelection(selection),first)).rejects.toThrow('Next page unavailable');expect(first).toEqual(before);
 const next=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection),first);expect(next.links).toHaveLength(2);expect(next.nextCursor).toBeNull();
 for(const [,input] of vi.mocked(nativeAstApi.neighborhood).mock.calls.slice(1))expect(input.cursor).toBe(cursor);
});
it('uses an observed pathless SCIP primary key directly',async()=>{
 const node={...anchor(),file:undefined,properties:{uid:'scip:os/exec.Cmd#Run().'},nativeKey:{type:'string' as const,value:'scip:observed-key'}};
 const page=neighborhoodPage();page.nodes[0].primary_key.value=node.nativeKey;page.edges.forEach(e=>{e.position.from_key=node.nativeKey});page.nodes[0].properties=page.nodes[0].properties.filter(p=>p.name!=='path').map(p=>p.name==='entity_key'?{...p,value:node.nativeKey}:p.name==='uid'?{...p,value:{type:'string' as const,value:node.properties.uid}}:p);
 vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(page);const graph=await loadNativeNeighborhood(node,nativeSchema,installed);
 expect(graph.anchor.file).toBeUndefined();expect(graph.anchor.nativeKey).toEqual(node.nativeKey);expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();
});
it('resolves search observations in the neighborhood request without another Cypher lookup',async()=>{
 const hit=searchReply.results[0],node={id:'renderer-only',label:hit.kind,type:hit.kind,name:hit.name,properties:{...hit}};
 const graph=await loadNativeNeighborhood(node,nativeSchema,installed);expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();
 expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledWith(installed,{anchor_label:'Function',anchor_key:{type:'string',value:hit.entity_key},limit:100},undefined);
 expect(graph.anchor.nativeIdentity).toBeTruthy();expect(graph.anchor.id).not.toBe(node.id);
});
it('deduplicates public relationships as physical addresses change and are reused between pages',async()=>{
 const firstPage=neighborhoodPage(),second=neighborhoodPage();firstPage.next_cursor={opaque:'second page'};
 second.nodes=second.nodes.map((n,i)=>({...n,id:{table:8,row:String(i)}}));second.anchor=second.nodes[0].id;
 const extra={...structuredClone(second.nodes[1]),id:nativeSample.nodes[0].id,primary_key:{name:'entity_key',value:{type:'string' as const,value:'observed-key-C'}},properties:second.nodes[1].properties.map(p=>p.name==='entity_key'?{...p,value:{type:'string' as const,value:'observed-key-C'}}:p.name==='name'?{...p,value:{type:'string' as const,value:'Third'}}:p)};second.nodes.push(extra);
 second.edges=second.edges.map((e,i)=>({...e,id:{member:99-i,row:'1'},from:second.anchor,to:second.nodes[1].id}));second.edges.push({...second.edges[0],id:nativeSample.edges[0].id,to:extra.id,uid:'third-edge',position:{...second.edges[0].position,uid:'third-edge',to_key:extra.primary_key.value}});
 vi.mocked(nativeAstApi.neighborhood).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(second);
 const node=anchor(),first=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection)),next=await loadNativeNeighborhood(node,nativeSchema,astSelection(selection),first);
 expect(next.nodes.map(n=>n.nativeKey?.value)).toEqual(['observed-key-A','observed-key-B','observed-key-C']);expect(next.links.map(e=>e.uid)).toEqual(['observed-edge-0','observed-edge-1','third-edge']);
 expect(next.nodes[0].id).toBe(first.nodes[0].id);expect(next.nodes[0].nativeIdentity).not.toEqual(first.nodes[0].nativeIdentity);
 expect(next.links.every(e=>next.nodes.some(n=>n.id===e.source)&&next.nodes.some(n=>n.id===e.target))).toBe(true);
 expect(first.nodes).toHaveLength(2);expect(first.links).toHaveLength(2);
});
it.each(['entity_key','uid'] as const)('rejects a page changing the observed anchor %s',async name=>{
 const page=neighborhoodPage();if(name==='entity_key'){page.nodes[0].primary_key={name,value:{type:'string',value:'another-entity'}};page.edges=page.edges.map(e=>({...e,position:{...e.position,from_key:page.nodes[0].primary_key.value}}));}page.nodes[0].properties=page.nodes[0].properties.map(p=>p.name===name?{...p,value:{type:'string' as const,value:'another-entity'}}:p);vi.mocked(nativeAstApi.neighborhood).mockResolvedValue(page);
 await expect(loadNativeNeighborhood(anchor(),nativeSchema,astSelection(selection))).rejects.toThrow('anchor changed');
});
it('refuses missing public identities instead of deriving identity from renderer IDs or offsets',async()=>{
 await expect(loadNativeNeighborhood({...anchor(),nativeKey:undefined,properties:{}},nativeSchema,installed)).rejects.toThrow('primary key is unavailable');expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
 const page=neighborhoodPage();page.edges[0].uid='';vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(page);
 await expect(loadNativeNeighborhood(anchor(),nativeSchema,installed)).rejects.toThrow('position does not match');
});
it('preserves a lossless integer primary key',async()=>{
 const schema={...nativeSchema,nodes:nativeSchema.nodes.map(n=>({...n,columns:n.columns.map(([name,type]):[string,string]=>[name,name===n.primary_key?'INT64':type])}))};
 const node={...anchor(),nativeKey:undefined,properties:{entity_key:9007199254740993n}},page=neighborhoodPage();page.nodes[0].primary_key.value={type:'int64',value:'9007199254740993'};page.edges.forEach(e=>{e.position.from_key=page.nodes[0].primary_key.value});page.nodes[0].properties=page.nodes[0].properties.map(p=>p.name==='entity_key'?{...p,value:{type:'int64' as const,value:'9007199254740993'}}:p);vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(page);
 await loadNativeNeighborhood(node,schema,installed);expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledWith(installed,expect.objectContaining({anchor_key:{type:'int64',value:'9007199254740993'}}),undefined);
});
it('stops before dispatch when cancelled and fences a late page after in-flight cancellation',async()=>{
 const before=new AbortController();before.abort();await expect(loadNativeNeighborhood(anchor(),nativeSchema,installed,undefined,before.signal)).rejects.toMatchObject({name:'AbortError'});expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
 let finish!:(value:ReturnType<typeof neighborhoodPage>)=>void;vi.mocked(nativeAstApi.importedNeighborhood).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));
 const during=new AbortController(),running=loadNativeNeighborhood(anchor(),nativeSchema,installed,undefined,during.signal);expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledWith(installed,expect.anything(),during.signal);
 during.abort();finish(neighborhoodPage());await expect(running).rejects.toMatchObject({name:'AbortError'});
});
it('still resolves a standalone impact anchor with a bounded query and rejects ambiguity',async()=>{
 const hit=searchReply.results[0],node={id:'renderer-only',label:hit.kind,type:hit.kind,name:hit.name,properties:{...hit}},resolved=await resolveNativeNode(node,nativeSchema,installed);
 expect(resolved.nativeKey?.value).toBe(hit.entity_key);expect(nativeAstApi.importedQuery).toHaveBeenCalledWith(installed,'MATCH (n:Function) WHERE n.entity_key = $observed_key RETURN n LIMIT 2',{observed_key:hit.entity_key},undefined);
 const duplicate=structuredClone(resolvedQuery);const cell=duplicate.rows[0][0].value as {id:{value:{row:string}}};cell.id.value.row='different';vi.mocked(nativeAstApi.importedQuery).mockResolvedValue({...resolvedQuery,rows:[...resolvedQuery.rows,...duplicate.rows]});
 await expect(resolveNativeNode(node,nativeSchema,installed)).rejects.toThrow('unavailable or ambiguous');
});

it('preserves indistinguishable parallel relationships by their public occurrence ordinal',async()=>{
 const page=neighborhoodPage();page.edges[1].uid=page.edges[0].uid;page.edges[1].position={...page.edges[0].position,occurrence:'1'};vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValue(page);
 const graph=await loadNativeNeighborhood(anchor(),nativeSchema,installed);
 expect(graph.links).toHaveLength(2);expect(new Set(graph.links.map(e=>e.id)).size).toBe(2);expect(graph.links[0].uid).toBe(graph.links[1].uid);
});
it('joins overlapping pages by full logical position while retaining equal-UID parallel occurrences',async()=>{
 const first=neighborhoodPage();first.edges[1].uid=first.edges[0].uid;first.edges[1].position={...first.edges[0].position,occurrence:'9007199254740993'};first.next_cursor={opaque:{position:first.edges[1].position}};
 const second=structuredClone(first);second.next_cursor=null;second.nodes=second.nodes.map((n,i)=>({...n,id:{table:19,row:String(i)}}));second.anchor=second.nodes[0].id;second.edges=second.edges.map((e,i)=>({...e,id:{member:99,row:String(i)},from:second.nodes[0].id,to:second.nodes[1].id}));second.edges.push({...second.edges[1],id:{member:99,row:'3'},position:{...second.edges[1].position,occurrence:'18446744073709551615'}});
 vi.mocked(nativeAstApi.importedNeighborhood).mockResolvedValueOnce(first).mockResolvedValueOnce(second);
 const node=anchor(),loaded=await loadNativeNeighborhood(node,nativeSchema,installed),continued=await loadNativeNeighborhood(node,nativeSchema,installed,loaded);
 expect(loaded.links).toHaveLength(2);expect(continued.links).toHaveLength(3);expect(continued.links.map(e=>e.uid)).toEqual(Array(3).fill(first.edges[0].uid));expect(continued.links.slice(0,2).map(e=>e.id)).toEqual(loaded.links.map(e=>e.id));expect(continued.links[2].id).toContain('18446744073709551615');
 expect(vi.mocked(nativeAstApi.importedNeighborhood).mock.calls[1][1].cursor).toBe(first.next_cursor);
});
