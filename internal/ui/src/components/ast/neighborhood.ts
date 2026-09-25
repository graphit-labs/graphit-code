import { astApi, type GraphNode, type GraphEdge, type SchemaResponse } from "@/api/ast";

const PAGE_SIZE = 100;
const safe = (s: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
type NodeType = NonNullable<SchemaResponse["node_types"]>[number];
interface Partition { direction: "incoming" | "outgoing"; type: string; target: NodeType; after?: string; more: boolean; error?: string }
export interface Neighborhood { anchor: GraphNode; nodes: GraphNode[]; links: GraphEdge[]; partitions: Partition[]; indexGeneration?: string }
export type NeighborhoodLoader = (node: GraphNode, previous?: Neighborhood, signal?: AbortSignal) => Promise<Neighborhood>;
const stableId = (label: string, identity: unknown) => JSON.stringify([label, String(identity)]);
class IndexChanged extends Error {}

function stableNode(node: GraphNode, schema: SchemaResponse): GraphNode {
  const type = schema.node_types?.find(t => t.label === node.label);
  if (!type?.identity_property) throw new Error(`The ${node.label} entity type has no declared identity.`);
  const identity = node.properties?.[type.identity_property] ?? (type.identity_property === "path" ? node.file : undefined);
  if (identity == null || identity === "") throw new Error("A related entity has no stable identity. Reindex before exploring.");
  return { ...node, id: stableId(type.label, identity), properties: { ...node.properties, [type.identity_property]: identity } };
}

/** Direct, typed evidence keyed by indexed UIDs, even when the graph is remounted. */
export async function loadNeighborhood(node: GraphNode, schema: SchemaResponse, context?: string, projectDir?: string, previous?: Neighborhood, signal?: AbortSignal, projectId?: string, refreshAttempts = 0): Promise<Neighborhood> {
  const check = () => signal?.throwIfAborted();
  check();
  const type = schema.node_types?.find(t => t.label === node.label);
  if (!type?.identity_property || !schema.relationship_endpoints) throw new Error("This index does not expose entity identity and relationship metadata. Refresh the index schema and try again.");
  if (![type.label, type.identity_property].every(safe)) throw new Error("This entity requires an explicit query in Query lab.");
  const source = previous?.anchor || node;
  const identity = source.properties?.[type.identity_property] ?? (type.identity_property === "path" ? source.file : undefined);
  if (identity == null || identity === "") throw new Error("This entity has no indexed unique identifier. Refresh the result or reindex before exploring.");
  const anchor = stableNode(source, schema);
  const partitions: Partition[] = previous ? previous.partitions.map(p => ({ ...p })) : [];
  if (!previous) {
    const seen = new Set<string>();
    for (const endpoint of schema.relationship_endpoints) {
      for (const direction of ["incoming", "outgoing"] as const) {
        if ((direction === "outgoing" ? endpoint.from : endpoint.to) !== type.label) continue;
        const label = direction === "outgoing" ? endpoint.to : endpoint.from;
        const key = JSON.stringify([direction, endpoint.type, label]);
        if (seen.has(key)) continue;
        seen.add(key);
        const target = schema.node_types?.find(t => t.label === label);
        if (!target?.identity_property) throw new Error(`The ${label} entity type has no declared identity.`);
        partitions.push({ direction, type: endpoint.type, target, more: true });
      }
    }
  }
  const nodes = new Map((previous?.nodes || [anchor]).map(n => [n.id, n]));
  const links = new Map((previous?.links || []).filter(e => e.uid).map(e => [e.uid!, e]));
  const pending = partitions.filter(p => p.more || p.error);
  let indexGeneration = previous?.indexGeneration;
  let index = 0;
  try { await Promise.all(Array.from({ length: Math.min(4, pending.length) }, async () => {
    while (index < pending.length) {
      check();
      const p = pending[index++];
      try {
        if (![p.type, p.target.label, p.target.identity_property].every(safe)) throw new Error("Unsupported schema identifier");
        const data = await astApi.getGraphNeighborhood({
          anchor_label: type.label, anchor_identity: String(identity), relationship_type: p.type,
          target_label: p.target.label, direction: p.direction, cursor: p.after,
          limit: PAGE_SIZE, context, project_dir: projectDir, project_id: projectId, signal,
        });
        check();
        if (data.index_generation && indexGeneration && data.index_generation !== indexGeneration) throw new IndexChanged("Index changed during exploration");
        if (data.index_generation) indexGeneration = data.index_generation;
        const nativeToStable = new Map<string, string>();
        for (const n of data.nodes) {
          const stable = stableNode(n, schema);
          nativeToStable.set(n.id, stable.id);
          nodes.set(stable.id, stable);
        }
        for (const edge of data.links) {
          const uid = edge.uid || String(edge.properties?.uid || "");
          if (!uid) throw new Error("An indexed relationship has no stable UID. Reindex before exploring.");
          const sourceId = nativeToStable.get(edge.source);
          const targetId = nativeToStable.get(edge.target);
          if (!sourceId || !targetId) throw new Error("An indexed relationship has an unresolved endpoint. Refresh the result.");
          links.set(uid, { ...edge, uid, source: sourceId, target: targetId });
        }
        p.after = data.next_cursor || undefined;
        p.more = Boolean(data.next_cursor);
        p.error = undefined;
      } catch (e) { check(); if (e instanceof IndexChanged) throw e; p.error = (e as Error).message; }
    }
  })); } catch (e) {
    if (e instanceof IndexChanged && refreshAttempts === 0) return loadNeighborhood(node, schema, context, projectDir, undefined, signal, projectId, 1);
    if (e instanceof IndexChanged) throw Object.assign(new Error("Index changed during exploration. Retry the neighborhood."), { cause: e });
    throw e;
  }
  check();
  return { anchor, nodes: [...nodes.values()], links: [...links.values()], partitions, indexGeneration };
}
