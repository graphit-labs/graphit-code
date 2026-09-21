import { api } from './client';
import { hubApi, type RegistryEntry } from './hub';
import type { LiveArtifact } from './live';
export interface LiveCatalogEntry extends RegistryEntry {
  ref: LiveArtifact;
  sourceLabel: string;
  unavailable?: string;
}
export const artifactKey = (a: LiveArtifact) => [a.source || 'hub', a.project_id || '', a.instance_id || '', a.project_kind || '', a.type || '', a.id, a.version || ''].join('|');
export async function loadLiveCatalog(projectDir: string, agent: string) {
 const hubEntries: LiveCatalogEntry[]=[];
 const hub = async()=>{
  let cursor: string|undefined;const seen=new Set<string>();
  do {
   const page=await hubApi.getRegistry(projectDir||undefined,agent,cursor);
   if(page.error) throw new Error(page.error);
   if(page.available===false) throw new Error('Hub registry is not configured or available. Project artifacts remain usable.');
   for(const entry of page.entries||[]) hubEntries.push({...entry,sourceLabel:'Hub',ref:{id:entry.id,type:entry.type,version:entry.latest,source:'hub'}});
   cursor=page.next_cursor;
   if(cursor && seen.has(cursor)) throw new Error('Hub returned a repeated page cursor');
   if(cursor) seen.add(cursor);
  } while(cursor);
  return hubEntries;
 };
 const local=async()=>{
  const result=await api.get<{entries: Array<LiveArtifact & {name:string;description?:string;project_name:string;unavailable?:string}>}>(`/api/live/artifacts?${new URLSearchParams({agent,project_dir:projectDir})}`);
  return result.entries.map(item=>({id:item.id,type:item.type||'',name:item.name,description:item.description,latest:item.version,project_id:item.project_id,sourceLabel:`Project · ${item.project_name}`,unavailable:item.unavailable,ref:{id:item.id,type:item.type,version:item.version,source:item.source,project_id:item.project_id,instance_id:item.instance_id,project_kind:item.project_kind}}));
 };
 const results=await Promise.allSettled([local(),hub()]);
 const entries:LiveCatalogEntry[]=[],errors:string[]=[];
 results.forEach((result,index)=>{if(result.status==='fulfilled') entries.push(...result.value);else {errors.push(`${index===0?'Project':'Hub'} catalogue: ${result.reason instanceof Error?result.reason.message:String(result.reason)}`);if(index===1) entries.push(...hubEntries);}});
 const unique=new Map(entries.map(item=>[artifactKey(item.ref),item]));
 return {entries:[...unique.values()],errors};
}
export function filterLiveCatalog(entries:LiveCatalogEntry[],query:string,type:string,source:string) {
 const q=query.trim().toLocaleLowerCase();
 return entries.filter(e=>(!type||e.type===type)&&(!source||e.ref.source===source)&&(!q||[e.id,e.type,e.name,e.description,e.project_id,e.sourceLabel,...(e.tags||[])].join(' ').toLocaleLowerCase().includes(q)));
}
