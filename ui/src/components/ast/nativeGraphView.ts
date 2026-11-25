import type { GraphNode, GraphEdge, SchemaResponse } from '@/api/ast'
import type { AstCell, AstNodeId, AstEdgeId, AstSample, AstSchema, AstQuery, AstNeighborhood, AstKey, AstNeighborhoodPosition } from '@/api/nativeAst'
import { stringifyNativeJson } from '@/api/nativeJson'

export const nativeNodeId = (id: AstNodeId) => stringifyNativeJson(['node', id.table, id.row])
export const publicNodeId = (label: string, key: AstKey) => stringifyNativeJson(['entity', label, key.type, key.value])
const nativeEdgeId = (id: AstEdgeId) => stringifyNativeJson(['edge', id.member, id.row])
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

function nodeView(id: AstNodeId, label: string, properties: Record<string, unknown>, key?: AstKey): GraphNode {
  return { id: key ? publicNodeId(label, key) : nativeNodeId(id), label, type: label, name: text(properties.name) ?? label, file: text(properties.path), line: lineValue(properties.line_number), properties, nativeIdentity: id, nativeKey: key }
}
export interface NativeGraphView { nodes: GraphNode[]; links: GraphEdge[]; tabular?: AstQuery }
// Physical addresses join endpoints only inside the response that returned them.
function responseIdentities(nodes: GraphNode[]) {
  return new Map(nodes.filter(n => n.nativeIdentity).map(n => [nativeNodeId(n.nativeIdentity!), n.id]))
}
export function nativeSampleView(sample: AstSample, schema: AstSchema): NativeGraphView {
  const nodes = sample.nodes.map(n => {
      const primary = schema.nodes.find(type => type.label === n.label)?.primary_key;
      const observed = n.primary_key && n.primary_key.name === primary ? n.primary_key : undefined;
      return nodeView(n.id, n.label, { name: n.name, path: n.path, uid: n.uid, cluster: n.cluster, lang: n.lang, line_number: n.line, ...(observed ? { [observed.name]: observed.value.value } : {}) }, observed?.value);
    })
  const identities = responseIdentities(nodes)
  return { nodes, links: sample.edges.map(e => ({ id: nativeEdgeId(e.id), uid: e.uid ?? undefined, source: identities.get(nativeNodeId(e.from)) ?? nativeNodeId(e.from), target: identities.get(nativeNodeId(e.to)) ?? nativeNodeId(e.to), type: e.relationship })) }
}
function typedProperties(cell: unknown): Record<string, AstCell> {
  const value = cell as AstCell | undefined
  return value?.type === 'PROPERTIES' && value.value && typeof value.value === 'object' ? value.value as Record<string, AstCell> : {}
}
export function nativeQueryView(query: AstQuery, schema?: AstSchema): NativeGraphView {
  const nodes = new Map<string, GraphNode>(), edges = new Map<string, GraphEdge>()
  const walk = (cell: unknown): void => {
    if (!cell || typeof cell !== 'object') return
    if (Array.isArray(cell)) { cell.forEach(walk); return }
    const typed = cell as AstCell, record = typed.value as Record<string, AstCell> | undefined
    if (typed.type === 'NODE' && record?.id?.type === 'INTERNAL_ID' && record.label?.type === 'STRING') {
      const identity = record.id.value as AstNodeId, label = String(record.label.value)
      const properties = typedProperties(record.properties), primary = schema?.nodes.find(n => n.label === label)?.primary_key
      const node = nodeView(identity, label, Object.fromEntries(Object.entries(properties).map(([name, value]) => [name, value.value])), primary ? keyValue(properties[primary]) : undefined)
      nodes.set(nativeNodeId(identity), node)
    } else if (typed.type === 'REL' && record?.id?.type === 'INTERNAL_ID' && record.from?.type === 'INTERNAL_ID' && record.to?.type === 'INTERNAL_ID') {
      const identity = record.id.value as AstEdgeId, from = record.from.value as AstNodeId, to = record.to.value as AstNodeId
      const properties = typedProperties(record.properties), id = nativeEdgeId(identity)
      edges.set(id, { id, uid: text(properties.uid?.value), source: nativeNodeId(from), target: nativeNodeId(to), type: String(record.label?.value ?? ''), properties: Object.fromEntries(Object.entries(properties).map(([name, value]) => [name, value.value])) })
    } else if (typed.value && typeof typed.value === 'object') {
      if (Array.isArray(typed.value)) typed.value.forEach(walk)
      else Object.values(typed.value).forEach(walk)
    }
  }
  query.rows.forEach(row => row.forEach(walk))
  const identities = responseIdentities([...nodes.values()])
  return { nodes: [...nodes.values()], links: [...edges.values()].map(edge => ({ ...edge, source: identities.get(edge.source) ?? edge.source, target: identities.get(edge.target) ?? edge.target })), tabular: query }
}
function decimal(value: unknown, minimum: bigint, maximum: bigint): value is string {
  if (typeof value !== 'string' || value.length > 20 || !/^(?:0|-?[1-9][0-9]*)$/.test(value)) return false
  const exact = BigInt(value)
  return exact >= minimum && exact <= maximum
}
function publicKey(value: unknown): value is AstKey {
  if (!value || typeof value !== 'object') return false
  const key = value as AstKey
  return key.type === 'string' && typeof key.value === 'string' || key.type === 'int64' && decimal(key.value, -9223372036854775808n, 9223372036854775807n)
}
function positionId(position: AstNeighborhoodPosition): string {
  if (!position || typeof position !== 'object' || typeof position.uid !== 'string' || typeof position.relationship !== 'string' || typeof position.from_label !== 'string' || typeof position.to_label !== 'string' || !publicKey(position.from_key) || !publicKey(position.to_key) || !decimal(position.occurrence, 0n, 18446744073709551615n)) throw new Error('The relationship position is invalid.')
  return stringifyNativeJson(['relationship', position.uid, position.relationship, position.from_label, position.from_key.type, position.from_key.value, position.to_label, position.to_key.type, position.to_key.value, position.occurrence])
}
export function nativeNeighborhoodView(page: AstNeighborhood) {
  const nodes = page.nodes.map(n => {
    if (!n.primary_key || typeof n.primary_key.name !== 'string' || !publicKey(n.primary_key.value)) throw new Error('A neighborhood node has an invalid public primary key.')
    const properties = Object.fromEntries(n.properties.map(p => [p.name, p.value.type === 'null' ? null : p.value.value]))
    return nodeView(n.id, n.label, properties, n.primary_key.value)
  })
  const identities = responseIdentities(nodes)
  return { anchor: nodes.find(n => n.id === identities.get(nativeNodeId(page.anchor))), nodes, links: page.edges.map(e => {
    const id = positionId(e.position), source = identities.get(nativeNodeId(e.from)), target = identities.get(nativeNodeId(e.to))
    if (e.position.uid !== e.uid || e.position.relationship !== e.relationship || source !== publicNodeId(e.position.from_label, e.position.from_key) || target !== publicNodeId(e.position.to_label, e.position.to_key)) throw new Error('The relationship position does not match its page endpoints.')
    return { id, uid: e.uid, source, target, type: e.relationship, properties: Object.fromEntries(e.properties.map(p => [p.name, p.value.type === 'null' ? null : p.value.value])) }
  }) }
}
