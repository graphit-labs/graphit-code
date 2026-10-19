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
 expect(nativeNeighborhoodView(neighborhoodPage(),nativeSchema).anchor?.nativeKey?.value).toBe('observed-key-A')
})
it('uses the response identities when publication advances beyond cached schema metadata', () => {
 const schema = { ...nativeSchema, generation: nativeSchema.generation.map(x => 255-x) }
 const sample = nativeSampleView(nativeSample,schema)
 const query = nativeQueryView(nativeQuery,schema)
 expect(sample.nodes[0].nativeIdentity?.generation).toEqual(nativeSample.generation)
 expect(query.nodes[0].nativeIdentity?.generation).toEqual(nativeSample.generation)
 expect(sample.nodes[0].nativeKey?.value).toBe('observed-key-A')
 expect(query.nodes[0].nativeKey?.value).toBe('observed-key-A')
 expect(nativeNeighborhoodView(neighborhoodPage(),schema).anchor?.nativeIdentity?.generation).toEqual(nativeSample.generation)
})
it.each(['node', 'relation', 'from', 'to'] as const)('rejects mixed generations in %s identities before rendering the query', part => {
 const query = structuredClone(nativeQueryWithRelation)
 const node = query.rows[0][0].value as { id: { value: { generation: number[] } } }
 const relation = query.rows[0][2].value as Record<'id' | 'from' | 'to', { value: { generation: number[] } }>
 const identity = part === 'node' ? node.id.value : relation[part === 'relation' ? 'id' : part].value
 identity.generation = nativeSchema.generation.map(x => 255-x)
 expect(() => nativeQueryView(query,nativeSchema)).toThrow('mixes graph generations')
})
it.each([{generation:[]}, {generation:Array(32).fill(256)}])('rejects invalid response generations without inventing renderer identities: $generation', ({generation}) => {
 expect(() => nativeSampleView({...nativeSample,generation},nativeSchema)).toThrow('invalid graph generation')
 expect(() => nativeNeighborhoodView({...neighborhoodPage(),generation},nativeSchema)).toThrow('invalid graph generation')
 const query = structuredClone(nativeQuery)
 const identity = (query.rows[0][0].value as { id: { value: { generation: number[] } } }).id.value
 identity.generation = generation
 expect(() => nativeQueryView(query,nativeSchema)).toThrow('invalid graph generation')
})
