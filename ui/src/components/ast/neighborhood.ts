import { nativeAstApi, type AstSchema, type AstSelection, type AstCursor, type AstImportedSource, type AstKey } from '@/api/nativeAst';
import { nativeNeighborhoodView, nativeQueryView, publicNodeId } from './nativeGraphView';
import { stringifyNativeJson } from '@/api/nativeJson';
import { invalidateTaskWorkspace } from '@/components/task/taskWorkspaceState';
import { type GraphNode, type GraphEdge } from "@/api/ast";

const PAGE_SIZE = 100;
const safe = (s: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
export interface Neighborhood { anchor: GraphNode; nodes: GraphNode[]; links: GraphEdge[]; nextCursor: AstCursor | null; selectionKey?: string }
export type NeighborhoodLoader = (node: GraphNode, previous?: Neighborhood, signal?: AbortSignal) => Promise<Neighborhood>;

function observedKey(node: GraphNode, schema: AstSchema): AstKey {
  const type = schema.nodes.find(type => type.label === node.label);
  if (type && node.nativeKey) return node.nativeKey;
  const value = type && node.properties?.[type.primary_key];
  const column = type?.columns.find(([name]) => name === type.primary_key)?.[1];
  if (column === 'STRING' && typeof value === 'string' && value) return { type: 'string', value };
  if (column === 'INT64' && (typeof value === 'bigint' || typeof value === 'number' && Number.isSafeInteger(value) || typeof value === 'string' && /^-?\d+$/.test(value))) return { type: 'int64', value: String(value) };
  throw new Error('The primary key is unavailable in this result. Reload the sample or query before following relationships.');
}

/** Resolve a search observation through the published graph before using its anchor. */
export async function resolveNativeNode(node: GraphNode, schema: AstSchema, source: AstSelection | AstImportedSource, signal?: AbortSignal): Promise<GraphNode> {
  signal?.throwIfAborted();
  if (node.nativeKey) return node;
  const type = schema.nodes.find(type => type.label === node.label);
  const key = observedKey(node, schema);
  if (!type || !safe(type.label) || !safe(type.primary_key)) throw new Error('The primary key is unavailable in this result. Reload the sample or query before following relationships.');
  const query = 'MATCH (n:' + type.label + ') WHERE n.' + type.primary_key + ' = $observed_key RETURN n LIMIT 2';
  const raw = 'project_dir' in source ? await nativeAstApi.query(source, query, { observed_key: key.type === 'int64' ? BigInt(key.value) : key.value }, signal) : await nativeAstApi.importedQuery(source, query, { observed_key: key.type === 'int64' ? BigInt(key.value) : key.value }, signal);
  signal?.throwIfAborted();
  const nodes = nativeQueryView(raw, schema).nodes;
  if (nodes.length !== 1 || nodes[0].label !== node.label || stringifyNativeJson(nodes[0].nativeKey) !== stringifyNativeJson(key) || typeof node.properties?.uid === 'string' && nodes[0].properties?.uid !== node.properties.uid) throw new Error('The search entity is unavailable or ambiguous in the published graph. Search again.');
  return nodes[0];
}

/** One bounded SDK page per request; the continuation remains opaque. */
export async function loadNativeNeighborhood(node: GraphNode, schema: AstSchema, selection: AstSelection | AstImportedSource, previous?: Neighborhood, signal?: AbortSignal): Promise<Neighborhood> {
  signal?.throwIfAborted();
  const selectionKey = stringifyNativeJson('project_dir' in selection
    ? ['project', selection.project_dir, selection.project_version]
    : ['imported', selection.id, selection.version]);
  if (previous && previous.selectionKey !== selectionKey) throw new Error('The working context changed. Reload the neighborhood.');
  const key = observedKey(node, schema);
  if (previous && (previous.anchor.label !== node.label || stringifyNativeJson(previous.anchor.nativeKey) !== stringifyNativeJson(key))) throw new Error('The neighborhood cursor belongs to another entity. Reload the neighborhood.');
  if (previous?.nextCursor === null) return previous;
  try {
    const input = { anchor_label: node.label, anchor_key: key, limit: PAGE_SIZE, ...(previous ? { cursor: previous.nextCursor } : {}) };
    const data = 'project_dir' in selection ? await nativeAstApi.neighborhood(selection, input, signal) : await nativeAstApi.importedNeighborhood(selection, input, signal);
    signal?.throwIfAborted();
    const page = nativeNeighborhoodView(data);
    if (!page.anchor || page.anchor.label !== node.label || stringifyNativeJson(page.anchor.nativeKey) !== stringifyNativeJson(key) || typeof node.properties?.uid === 'string' && page.anchor.properties?.uid !== node.properties.uid) throw new Error('The graph anchor changed. Refresh the result before retrying the neighborhood.');
    const nodes = new Map((previous?.nodes ?? []).map(n => [n.id, n]));
    const links = new Map((previous?.links ?? []).map(edge => [edge.id!, edge]));
    const identities = new Map<string, string>();
    for (const endpoint of page.nodes) {
      if (!endpoint.nativeKey) throw new Error('A relationship endpoint has no public primary key.');
      const id = publicNodeId(endpoint.label, endpoint.nativeKey);
      identities.set(endpoint.id, id);
      nodes.set(id, { ...endpoint, id });
    }
    const anchor = nodes.get(identities.get(page.anchor.id)!)!;
    for (const edge of page.links) {
      const source = identities.get(edge.source), target = identities.get(edge.target);
      if (!source || !target) throw new Error('A relationship endpoint is unavailable in this page. Retry the neighborhood.');
      links.set(edge.id, { ...edge, source, target });
    }
    return { anchor, nodes: [...nodes.values()], links: [...links.values()], nextCursor: data.next_cursor, selectionKey };
  } catch (cause) {
    signal?.throwIfAborted();
    if ('project_dir' in selection) invalidateTaskWorkspace(cause, 'ast');
    throw cause;
  }
}
