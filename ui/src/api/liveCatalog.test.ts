import { afterEach, expect, it, vi } from 'vitest'
import { artifactKey, filterLiveCatalog, loadLiveCatalog, type LiveCatalog, type LiveCatalogEntry } from './liveCatalog'
const entry=(overrides:Partial<LiveCatalogEntry>={}):LiveCatalogEntry=>({id:'docs',name:'Docs',type:'knowledge',sourceLabel:'Project',ref:{source:'project',project_id:'payments',instance_id:null,project_kind:null,type:'knowledge',id:'docs',version:'v1'},...overrides})
const catalog=(entries:LiveCatalogEntry[]=[],errors:string[]=[]):LiveCatalog=>({entries,errors,projects:[{project_id:'payments',project_version:'v1'}],bindings:[{binding:'kiro-v3',available:true,model_selection:'explicit'}]})
afterEach(()=>vi.unstubAllGlobals())
it('reads only the canonical native catalogue and keeps complete provenance and capability metadata',async()=>{
 const reply=catalog([entry({description:'Delivery guide',project_name:'Payments display'})],['Hub: unavailable']),controller=new AbortController()
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(reply)))
 vi.stubGlobal('fetch',fetchMock)
 expect(await loadLiveCatalog(controller.signal)).toEqual(reply)
 expect(fetchMock).toHaveBeenCalledTimes(1); expect(fetchMock).toHaveBeenCalledWith('/api/live/artifacts',{credentials:'same-origin',signal:controller.signal})
})
it('filters exact native metadata and keeps tuples distinct across versions and sources',()=>{
 const local=entry(),hub=entry({name:'Published',sourceLabel:'Hub',ref:{...local.ref,source:'hub',project_id:'producer',version:'v2'}})
 const entries=[local,hub]
 expect(filterLiveCatalog(entries,'payments','','')).toEqual([local])
 expect(filterLiveCatalog(entries,'v2','','hub')).toEqual([hub])
 expect(filterLiveCatalog(entries,'','knowledge','project')).toEqual([local])
 expect(artifactKey(local.ref)).not.toBe(artifactKey(hub.ref))
 expect(artifactKey({...local.ref,id:'a|b',version:'c'})).not.toBe(artifactKey({...local.ref,id:'a',version:'b|c'}))
})
it.each(['Delivery guide','Payments display','engineering'])('retains the required catalogue metadata search for %s',query=>{
 const item={...entry(),tags:['engineering'],description:'Delivery guide',project_name:'Payments display'}
 expect(filterLiveCatalog([item],query,'','')).toEqual([item])
})
it('preserves scoped errors and unavailable entries without reviving old catalogue endpoints',async()=>{
 const unavailable=entry({unavailable:'project_knowledge_mount_unavailable'})
 const fetchMock=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(catalog([unavailable],['Hub: permission_denied'])))).mockResolvedValueOnce(new Response(JSON.stringify({error:{code:'permission_denied'}}),{status:403}))
 vi.stubGlobal('fetch',fetchMock)
 const result=await loadLiveCatalog()
 expect(result.entries[0].unavailable).toBe('project_knowledge_mount_unavailable')
 expect(result.errors).toEqual(['Hub: permission_denied'])
 await expect(loadLiveCatalog()).rejects.toMatchObject({status:403,code:'permission_denied'})
 expect(fetchMock).toHaveBeenCalledTimes(2)
})
