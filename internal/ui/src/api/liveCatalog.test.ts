import { afterEach, expect, it, vi } from 'vitest';
import { api } from './client';
import { hubApi } from './hub';
import { artifactKey, filterLiveCatalog, loadLiveCatalog } from './liveCatalog';
vi.mock('./client',()=>({api:{get:vi.fn()}}));
vi.mock('./hub',()=>({hubApi:{getRegistry:vi.fn()}}));
afterEach(()=>vi.resetAllMocks());
it('searches tags and project names across all Hub pages including empty pages',async()=>{
 vi.mocked(api.get).mockResolvedValue({entries:[{id:'same',type:'skill',name:'Local',source:'project',project_id:'p',instance_id:'i',project_kind:'file',project_name:'Payments',version:'local'}]});
 vi.mocked(hubApi.getRegistry).mockResolvedValueOnce({entries:Array.from({length:100},(_,i)=>({id:String(i),name:String(i),type:'skill'})),next_cursor:'next'} as any).mockResolvedValueOnce({entries:[],next_cursor:'last'} as any).mockResolvedValueOnce({entries:[{id:'same',name:'Published',type:'skill',tags:['engineering'],latest:'1'}]} as any);
 const {entries,errors}=await loadLiveCatalog('/project','codex');
 expect(errors).toEqual([]);expect(entries).toHaveLength(102);
 expect(hubApi.getRegistry).toHaveBeenLastCalledWith('/project','codex','last');
 expect(filterLiveCatalog(entries,'engineering','','')).toHaveLength(1);
 expect(filterLiveCatalog(entries,'skill','','project')).toHaveLength(1);
 expect(filterLiveCatalog(entries,'Payments','','project')).toHaveLength(1);
 const same=entries.filter(e=>e.id==='same');expect(artifactKey(same[0].ref)).not.toBe(artifactKey(same[1].ref));
});
it('keeps project sources usable when Hub fails',async()=>{
 vi.mocked(api.get).mockResolvedValue({entries:[{id:'docs',type:'knowledge',name:'Docs',source:'project',project_id:'p',instance_id:'i',project_kind:'own_context',project_name:'P'}]});
 vi.mocked(hubApi.getRegistry).mockRejectedValue(new Error('Hub unavailable'));
 const result=await loadLiveCatalog('/project','codex');expect(result.entries).toHaveLength(1);expect(result.errors[0]).toContain('Hub unavailable');
});
it('distinguishes an unconfigured Hub from an empty search result',async()=>{
 vi.mocked(api.get).mockResolvedValue({entries:[]});
 vi.mocked(hubApi.getRegistry).mockResolvedValue({entries:[],available:false} as any);
 const result=await loadLiveCatalog('/project','codex');expect(result.errors[0]).toContain('not configured');
});
