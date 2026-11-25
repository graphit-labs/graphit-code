import { expect, it } from 'vitest'
import { nativeSchemaView, nativeSampleView, nativeQueryView, nativeNeighborhoodView, sourceHighlightLine } from './nativeGraphView'
import { observedGraph } from './relationshipModel'
import { nativeSchema, nativeSample, nativeQuery, nativeQueryWithRelation, neighborhoodPage } from './nativeAst.test-fixtures'
it('keeps physical identities and observed keys distinct despite equal UIDs and parallel relations', () => {
 const view = nativeSampleView(nativeSample, nativeSchema)
 expect(new Set(view.nodes.map(n => n.id)).size).toBe(2)
 expect(view.nodes.map(n => n.nativeKey?.value)).toEqual(['observed-key-A','observed-key-B'])
 expect(view.nodes[0].nativeIdentity?.row).toBe('9007199254740993')
 expect(view.nodes[0].line).toBe(9007199254740993n); expect(sourceHighlightLine(view.nodes[0].line)).toBeNull()
 expect(observedGraph(view.nodes,view.links).links).toHaveLength(2)
 const missing = { ...nativeSample, nodes: nativeSample.nodes.map(({ primary_key: _key, ...node }) => node) }
 expect(nativeSampleView(missing,nativeSchema).nodes.every(n => !n.nativeKey)).toBe(true)
})
it('preserves raw typed query rows, exact counts and genuine native neighborhood keys', () => {
 const schema = nativeSchemaView(nativeSchema)
 expect(schema.nodes[0].count).toBe('18446744073709551615'); expect(schema.langs?.[0].count).toBe(18446744073709551615n)
 const query = nativeQueryView(nativeQuery,nativeSchema)
 const pending = nativeQueryView(nativeQueryWithRelation)
 expect(pending.tabular).toBe(nativeQueryWithRelation); expect(pending.links).toHaveLength(1); expect(pending.nodes.every(n => !n.nativeKey)).toBe(true)
 expect(query.tabular).toBe(nativeQuery); expect(query.nodes[0].nativeKey).toEqual({ type: 'string', value: 'observed-key-A' })
 expect(nativeNeighborhoodView(neighborhoodPage()).anchor?.nativeKey?.value).toBe('observed-key-A')
})
it('prefers stable public entity keys while physical rows move between independent responses', () => {
 const moved = structuredClone(nativeSample)
 moved.nodes = moved.nodes.map(n => ({...n,id:{table:n.id.table+10,row:n.id.row+'0'}}))
 moved.edges = moved.edges.map(e => ({...e,from:moved.nodes[0].id,to:moved.nodes[1].id}))
 const first = nativeSampleView(nativeSample,nativeSchema), next = nativeSampleView(moved,nativeSchema)
 expect(next.nodes.map(n => n.id)).toEqual(first.nodes.map(n => n.id))
 expect(next.nodes[0].nativeIdentity).not.toEqual(first.nodes[0].nativeIdentity)
 expect(next.links.map(e => [e.source,e.target])).toEqual(first.links.map(e => [e.source,e.target]))
})
it('renders native DTOs without graph generation metadata and preserves response-local physical joins', () => {
 const sample = nativeSampleView(nativeSample,nativeSchema), query = nativeQueryView(nativeQueryWithRelation,nativeSchema)
 expect(sample.nodes[0].nativeIdentity).toEqual(nativeSample.nodes[0].id)
 expect(query.nodes[0].nativeIdentity).toEqual(nativeSample.nodes[0].id)
 expect(query.links[0].source).toBe(query.nodes[0].id)
 expect(nativeNeighborhoodView(neighborhoodPage()).anchor?.id).toBe(sample.nodes[0].id)
 expect(sample.nodes[0].nativeIdentity).not.toHaveProperty('generation')
})

it('uses the projected typed primary key without guessing from properties or schema', () => {
 const page=neighborhoodPage();page.nodes[0].properties=page.nodes[0].properties.filter(p=>p.name!=='entity_key');
 const view=nativeNeighborhoodView(page);expect(view.anchor?.nativeKey).toEqual(page.nodes[0].primary_key.value);
 expect(view.links.every(e=>view.nodes.some(n=>n.id===e.source)&&view.nodes.some(n=>n.id===e.target))).toBe(true);
})
it.each(['-1','01','1.5','18446744073709551616',9007199254740993n,null])('rejects a non-u64 decimal occurrence: %s', occurrence => {
 const page=neighborhoodPage();Object.assign(page.edges[0].position,{occurrence});expect(()=>nativeNeighborhoodView(page)).toThrow('position is invalid');
})
it.each(['-9223372036854775809','9223372036854775808','+1','00'])('rejects a non-int64 public key: %s', value => {
 const page=neighborhoodPage();page.nodes[0].primary_key.value={type:'int64',value};expect(()=>nativeNeighborhoodView(page)).toThrow('primary key');
})
it('preserves the key type as part of the complete occurrence group', () => {
 const strings=neighborhoodPage();strings.nodes[0].primary_key.value={type:'string',value:'1'};strings.edges.forEach(e=>{e.position.from_key=strings.nodes[0].primary_key.value});
 const integers=structuredClone(strings);integers.nodes[0].primary_key.value={type:'int64',value:'1'};integers.edges.forEach(e=>{e.position.from_key=integers.nodes[0].primary_key.value});
 expect(nativeNeighborhoodView(strings).links[0].id).not.toBe(nativeNeighborhoodView(integers).links[0].id);
})
it.each(['uid','relationship','from_label','to_label'] as const)('rejects a position that changes its actual %s', field => {
 const page=neighborhoodPage();page.edges[0].position[field]='unobserved';expect(()=>nativeNeighborhoodView(page)).toThrow('position does not match');
})
