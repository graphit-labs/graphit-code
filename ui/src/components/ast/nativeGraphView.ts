import type { GraphNode, GraphEdge, SchemaResponse } from '@/api/ast'
import type { AstCell, AstGeneration, AstNodeId, AstEdgeId, AstSample, AstSchema, AstQuery, AstNeighborhood, AstKey } from '@/api/nativeAst'
import { stringifyNativeJson } from '@/api/nativeJson'

export const nativeNodeId = (generation: AstGeneration, id: AstNodeId) => stringifyNativeJson(['node', generation, id.table, id.row])
const nativeEdgeId = (generation: AstGeneration, id: AstEdgeId) => stringifyNativeJson(['edge', generation, id.member, id.row])
const lineValue = (value: unknown) => {
  if (typeof value === 'bigint') return value
  if (typeof value === 'number' && Number.isSafeInteger(value)) return value
  if (typeof value === 'string' && /^-?\d+$/.test(value)) { const exact = BigInt(value); return exact <= BigInt(Number.MAX_SAFE_INTEGER) && exact >= BigInt(Number.MIN_SAFE_INTEGER) ? Number(exact) : exact }
  return undefined
}
export const sourceHighlightLine = (line: GraphNode['line']) => typeof line === 'number' && Number.isSafeInteger(line) && line > 0 ? line : null
const text = (value: unknown) => typeof value === 'string' ? value : undefined
const keyValue = (cell: AstCell | undefined): AstKey | undefined => cell?.type === 'STRING' && typeof cell.value === 'string' ? { type: 'string', value: cell.value } : cell?.type === 'INT64' && typeof cell.value === 'string' ? { type: 'int64', value: cell.value } : undefined

/** Renderer model only. The API retains the full native DTO independently. */
export function nativeSchemaView(schema: AstSchema): SchemaResponse {
  const languages = new Map<string, { count: bigint; labels: { label: string; count: string }[] }>()
  for (const node of schema.nodes) for (const [lang, count] of node.language_counts) {
    const group = languages.get(lang) ?? { count: 0n, labels: [] }
    group.count += BigInt(count); group.labels.push({ label: node.label, count }); languages.set(lang, group)
  }
  return {
    nodes: schema.nodes.map(n => ({ label: n.label, count: n.rows })),
    edges: schema.relationships.map(r => ({ type: r.kind, count: r.rows })),
    node_types: schema.nodes.map(n => ({ label: n.label, identity_property: n.primary_key, properties: n.columns.map(([name]) => name) })),
    relationship_endpoints: schema.relationships.map(r => ({ type: r.kind, from: r.from_label, to: r.to_label })),
    node_labels: schema.nodes.map(n => n.label), edge_types: [...new Set(schema.relationships.map(r => r.kind))],
    langs: [...languages].map(([lang, group]) => ({ lang, ...group })), backend: 'Native AST graph',
  }
}

function nodeView(generation: AstGeneration, id: AstNodeId, label: string, properties: Record<string, unknown>, key?: AstKey): GraphNode {
  return { id: nativeNodeId(generation, id), label, type: label, name: text(properties.name) ?? label, file: text(properties.path), line: lineValue(properties.line_number), properties, nativeIdentity: { generation, ...id }, nativeKey: key }
}
export interface NativeGraphView { nodes: GraphNode[]; links: GraphEdge[]; tabular?: AstQuery }
export const isGeneration = (value: unknown): value is AstGeneration => Array.isArray(value) && value.length === 32 && value.every(byte => Number.isInteger(byte) && byte >= 0 && byte <= 255)
export const sameGeneration = (a: AstGeneration, b: AstGeneration) => a.length === b.length && a.every((byte, i) => byte === b[i])
export function nativeSampleView(sample: AstSample, schema: AstSchema): NativeGraphView {
  if (!isGeneration(sample.generation)) throw new Error('The sample has an invalid graph generation.');
  return {
    nodes: sample.nodes.map(n => {
      const primary = schema.nodes.find(type => type.label === n.label)?.primary_key;
      const observed = n.primary_key && n.primary_key.name === primary ? n.primary_key : undefined;
      return nodeView(sample.generation, n.id, n.label, { name: n.name, path: n.path, uid: n.uid, cluster: n.cluster, lang: n.lang, line_number: n.line, ...(observed ? { [observed.name]: observed.value.value } : {}) }, observed?.value);
    }),
    links: sample.edges.map(e => ({ id: nativeEdgeId(sample.generation, e.id), uid: e.uid ?? undefined, source: nativeNodeId(sample.generation, e.from), target: nativeNodeId(sample.generation, e.to), type: e.relationship })),
  }
}
function typedProperties(cell: unknown): Record<string, AstCell> {
  const value = cell as AstCell | undefined
  return value?.type === 'PROPERTIES' && value.value && typeof value.value === 'object' ? value.value as Record<string, AstCell> : {}
}
export function nativeQueryView(query: AstQuery, schema?: AstSchema): NativeGraphView {
  const nodes = new Map<string, GraphNode>(), edges = new Map<string, GraphEdge>()
  let generation: AstGeneration | undefined
  const observe = (value: unknown): AstGeneration => {
    if (!isGeneration(value)) throw new Error('The query has an invalid graph generation.')
    if (generation && !sameGeneration(generation, value)) throw new Error('The query mixes graph generations. Run it again.')
    generation = value
    return value
  }
  const walk = (cell: unknown): void => {
    if (!cell || typeof cell !== 'object') return
    if (Array.isArray(cell)) { cell.forEach(walk); return }
    const typed = cell as AstCell, record = typed.value as Record<string, AstCell> | undefined
    if (typed.type === 'NODE' && record?.id?.type === 'INTERNAL_ID' && record.label?.type === 'STRING') {
      const identity = record.id.value as AstNodeId & { generation: AstGeneration }, label = String(record.label.value)
      observe(identity.generation)
      const properties = typedProperties(record.properties), primary = schema?.nodes.find(n => n.label === label)?.primary_key
      const node = nodeView(identity.generation, identity, label, Object.fromEntries(Object.entries(properties).map(([name, value]) => [name, value.value])), primary ? keyValue(properties[primary]) : undefined)
      nodes.set(node.id, node)
    } else if (typed.type === 'REL' && record?.id?.type === 'INTERNAL_ID' && record.from?.type === 'INTERNAL_ID' && record.to?.type === 'INTERNAL_ID') {
      const identity = record.id.value as AstEdgeId & { generation: AstGeneration }, from = record.from.value as AstNodeId, to = record.to.value as AstNodeId
      observe(identity.generation)
      observe((from as AstNodeId & { generation: AstGeneration }).generation)
      observe((to as AstNodeId & { generation: AstGeneration }).generation)
      const properties = typedProperties(record.properties), id = nativeEdgeId(identity.generation, identity)
      edges.set(id, { id, uid: text(properties.uid?.value), source: nativeNodeId(identity.generation, from), target: nativeNodeId(identity.generation, to), type: String(record.label?.value ?? ''), properties: Object.fromEntries(Object.entries(properties).map(([name, value]) => [name, value.value])) })
    } else if (typed.type === 'INTERNAL_ID') {
      observe((typed.value as { generation?: unknown })?.generation)
    } else if (typed.value && typeof typed.value === 'object') {
      if (Array.isArray(typed.value)) typed.value.forEach(walk)
      else Object.values(typed.value).forEach(walk)
    }
  }
  query.rows.forEach(row => row.forEach(walk))
  return { nodes: [...nodes.values()], links: [...edges.values()], tabular: query }
}
export function nativeNeighborhoodView(page: AstNeighborhood, schema: AstSchema) {
  if (!isGeneration(page.generation)) throw new Error('The neighborhood has an invalid graph generation.');
  const nodes = page.nodes.map(n => {
    const properties = Object.fromEntries(n.properties.map(p => [p.name, p.value.type === 'null' ? null : p.value.value])), primary = schema.nodes.find(t => t.label === n.label)?.primary_key
    const value = n.properties.find(p => p.name === primary)?.value
    const key: AstKey | undefined = value?.type === 'string' || value?.type === 'int64' ? value : undefined
    return nodeView(page.generation, n.id, n.label, properties, key)
  })
  return { anchor: nodes.find(n => n.id === nativeNodeId(page.generation, page.anchor)), nodes, links: page.edges.map(e => ({ id: nativeEdgeId(page.generation, e.id), uid: e.uid ?? undefined, source: nativeNodeId(page.generation, e.from), target: nativeNodeId(page.generation, e.to), type: e.relationship, properties: Object.fromEntries(e.properties.map(p => [p.name, p.value.type === 'null' ? null : p.value.value])) })) }
}
