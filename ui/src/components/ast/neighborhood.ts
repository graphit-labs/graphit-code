import { nativeAstApi, type AstSchema, type AstSelection, type AstCursor, type AstImportedSource } from '@/api/nativeAst';
import { nativeNeighborhoodView, nativeQueryView, sameGeneration, isGeneration } from './nativeGraphView';
import { stringifyNativeJson } from '@/api/nativeJson';
import { invalidateTaskWorkspace } from '@/components/task/taskWorkspace';
import { type GraphNode, type GraphEdge, type SchemaResponse } from "@/api/ast";

const PAGE_SIZE = 100;
const safe = (s: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
type NodeType = NonNullable<SchemaResponse["node_types"]>[number];
interface Partition { direction: "incoming" | "outgoing"; type: string; target: NodeType; nativeCursor?: AstCursor; more: boolean; error?: string }
export interface Neighborhood { anchor: GraphNode; nodes: GraphNode[]; links: GraphEdge[]; partitions: Partition[]; selectionKey?: string }
export type NeighborhoodLoader = (node: GraphNode, previous?: Neighborhood, signal?: AbortSignal) => Promise<Neighborhood>;
class IndexChanged extends Error {}

/** Resolve a search observation through the published graph before using its anchor. */
export async function resolveNativeNode(node: GraphNode, schema: AstSchema, source: AstSelection | AstImportedSource, signal?: AbortSignal): Promise<GraphNode> {
  signal?.throwIfAborted();
  if (node.nativeIdentity && node.nativeKey) {
    return node;
  }
  const type = schema.nodes.find(type => type.label === node.label);
  const key = type && node.properties?.[type.primary_key];
  if (!type || !safe(type.label) || !safe(type.primary_key) || typeof key !== 'string' || !key) throw new Error('The primary key is unavailable in this result. Reload the sample or query before following relationships.');
  const query = 'MATCH (n:' + type.label + ') WHERE n.' + type.primary_key + ' = $observed_key RETURN n LIMIT 2';
  const raw = 'project_dir' in source ? await nativeAstApi.query(source, query, { observed_key: key }, signal) : await nativeAstApi.importedQuery(source, query, { observed_key: key }, signal);
  signal?.throwIfAborted();
  const nodes = nativeQueryView(raw, schema).nodes;
  if (nodes.length !== 1 || nodes[0].label !== node.label || nodes[0].nativeKey?.value !== key || typeof node.properties?.uid === 'string' && nodes[0].properties?.uid !== node.properties.uid) throw new Error('The search entity is unavailable or ambiguous in the published graph. Search again.');
  return nodes[0];
}

/** Native readers use observed primary keys and opaque SDK cursors. */
export async function loadNativeNeighborhood(node: GraphNode, schema: AstSchema, selection: AstSelection | AstImportedSource, previous?: Neighborhood, signal?: AbortSignal): Promise<Neighborhood> {
  const check = () => signal?.throwIfAborted();
  check();
  if (previous && (previous.anchor.label !== node.label || stringifyNativeJson(previous.anchor.nativeKey) !== stringifyNativeJson(node.nativeKey))) throw new Error('The neighborhood cursor belongs to another entity. Reload the neighborhood.');
  let anchor = previous?.anchor ?? (node.nativeIdentity && node.nativeKey ? node : await resolveNativeNode(node, schema, selection, signal));
  const type = schema.nodes.find(t => t.label === anchor.label);
  if (!type || !anchor.nativeIdentity || !anchor.nativeKey) throw new Error('The primary key is unavailable in this result. Reload the sample or query before following relationships.');
  if (!isGeneration(anchor.nativeIdentity.generation)) throw new Error('The anchor has an invalid graph generation.');
  const selectionKey = stringifyNativeJson(selection);
  if (previous && previous.selectionKey !== selectionKey) throw new Error('The working context changed. Reload the neighborhood.');
  const partitions: Partition[] = previous ? previous.partitions.map(p => ({ ...p })) : [];
  if (!previous) {
    const seen = new Set<string>();
    for (const endpoint of schema.relationships) for (const direction of ['incoming', 'outgoing'] as const) {
      if ((direction === 'outgoing' ? endpoint.from_label : endpoint.to_label) !== type.label) continue;
      const label = direction === 'outgoing' ? endpoint.to_label : endpoint.from_label;
      const key = stringifyNativeJson([direction, endpoint.kind, label]);
      if (seen.has(key)) continue; seen.add(key);
      const target = schema.nodes.find(t => t.label === label);
      if (!target) throw new Error('The relationship endpoint is absent from the current schema. Refresh the schema.');
      partitions.push({ direction, type: endpoint.kind, target: { label, identity_property: target.primary_key, properties: target.columns.map(([name]) => name) }, more: true });
    }
  }
  const nodes = new Map((previous?.nodes ?? []).map(n => [n.id, n]));
  const links = new Map((previous?.links ?? []).map(e => [e.id!, e]));
  const pending = partitions.filter(p => p.more || p.error); let index = 0;
  const outcomes = await Promise.allSettled(Array.from({ length: Math.min(4, pending.length) }, async () => {
    while (index < pending.length) {
      check(); const partition = pending[index++];
      try {
        const input = { anchor_label: type.label, anchor_key: anchor.nativeKey!, relationship: partition.type, other_label: partition.target.label, direction: partition.direction, limit: PAGE_SIZE, ...(partition.nativeCursor ? { cursor: partition.nativeCursor } : {}) };
        const data = 'project_dir' in selection ? await nativeAstApi.neighborhood(selection, input, signal) : await nativeAstApi.importedNeighborhood(selection, input, signal);
        check();
        if (!isGeneration(data.generation)) throw new IndexChanged('The neighborhood has an invalid graph generation.');
        if (partition.nativeCursor && !sameGeneration(data.generation, partition.nativeCursor.generation)) throw new IndexChanged('The graph changed within this relationship cursor. Reload the neighborhood.');
        if (data.next_cursor && !sameGeneration(data.next_cursor.generation, data.generation)) throw new IndexChanged('The graph changed within the neighborhood cursor. Reload the neighborhood.');
        const page = nativeNeighborhoodView(data, schema);
        if (!page.anchor || page.anchor.label !== anchor.label || stringifyNativeJson(page.anchor.nativeKey) !== stringifyNativeJson(anchor.nativeKey) || typeof anchor.properties?.uid === 'string' && page.anchor.properties?.uid !== anchor.properties.uid) throw new IndexChanged('The graph anchor changed. Refresh the result before retrying the neighborhood.');
        // Each partition is a separate query and may observe a newer graph. Only
        // public keys join their renderer nodes; physical IDs stay scoped to the page.
        const identities = new Map<string, string>();
        for (const n of page.nodes) {
          if (!n.nativeKey) throw new Error('A relationship endpoint has no public primary key.');
          const id = stringifyNativeJson(['entity', n.label, n.nativeKey.type, n.nativeKey.value]);
          identities.set(n.id, id);
          nodes.set(id, { ...n, id });
        }
        anchor = nodes.get(identities.get(page.anchor.id)!)!;
        for (const edge of page.links) {
          const source = identities.get(edge.source), target = identities.get(edge.target);
          if (!source || !target) throw new Error('A relationship endpoint is unavailable in this page. Retry the neighborhood.');
          links.set(edge.id, { ...edge, source, target });
        }
        partition.nativeCursor = data.next_cursor ?? undefined; partition.more = data.next_cursor !== null; partition.error = undefined;
      } catch (cause) {
        check(); if ('project_dir' in selection) invalidateTaskWorkspace(cause, 'ast');
        if (cause instanceof IndexChanged) throw cause;
        partition.error = cause instanceof Error ? cause.message : 'The relationship request failed.';
      }
    }
  }));
  for (const outcome of outcomes) if (outcome.status === 'rejected') throw outcome.reason;
  check();
  return { anchor: nodes.get(anchor.id) ?? anchor, nodes: nodes.size ? [...nodes.values()] : [anchor], links: [...links.values()], partitions, selectionKey };
}
