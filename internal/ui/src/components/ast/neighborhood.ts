import { astApi, type GraphNode, type GraphEdge, type SchemaResponse } from "@/api/ast";

const PAGE_SIZE = 100;
const quote = (s: string) => "'" + s.replace(/\\/g, "\\\\").replace(/'/g, "\\'") + "'";
const safe = (s: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
type NodeType = NonNullable<SchemaResponse["node_types"]>[number];
interface Partition { direction: "incoming" | "outgoing"; type: string; target: NodeType; after?: string; more: boolean; error?: string }
export interface Neighborhood { anchor: GraphNode; nodes: GraphNode[]; links: GraphEdge[]; partitions: Partition[] }
export type NeighborhoodLoader = (node: GraphNode, previous?: Neighborhood, signal?: AbortSignal) => Promise<Neighborhood>;
const stableId = (label: string, identity: unknown) => JSON.stringify([label, String(identity)]);

/** Direct, schema-typed evidence. This never writes to the original search result. */
export async function loadNeighborhood(node: GraphNode, schema: SchemaResponse, context?: string, projectDir?: string, previous?: Neighborhood, signal?: AbortSignal, projectId?: string): Promise<Neighborhood> {
  const check = () => signal?.throwIfAborted();
  check();
  const type = schema.node_types?.find(t => t.label === node.label);
  if (!type?.identity_property || !schema.relationship_endpoints) throw new Error("This index does not expose entity identity and relationship metadata. Refresh the index schema and try again.");
  if (![type.label, type.identity_property].every(safe)) throw new Error("This entity requires an explicit query in Query lab.");
  let anchor = previous?.anchor || node;
  let identity: unknown = anchor.properties?.[type.identity_property] ?? (type.identity_property === "path" ? anchor.file : undefined);
  if (identity == null || identity === "") {
    const filters: string[] = [];
    if (type.properties.includes("name")) filters.push("n.name = " + quote(node.name));
    if (type.properties.includes("path") && node.file) filters.push("n.path = " + quote(node.file));
    if (type.properties.includes("line_number") && Number.isFinite(node.line)) filters.push("n.line_number = " + node.line);
    if (!filters.length) throw new Error("This entity has no indexed identity or location to resolve.");
    const resolved = await astApi.getGraph({ context, project_dir: projectDir, ...(projectId ? { project_id: projectId } : {}), signal, cypher_query: `MATCH (n:${type.label}) WHERE ${filters.join(" AND ")} RETURN n LIMIT 2` });
    check();
    if (resolved.nodes.length !== 1) throw new Error(resolved.nodes.length ? "Multiple indexed entities match this location. Refine the search before exploring." : "This entity is no longer in the index. Refresh the search.");
    anchor = resolved.nodes[0];
    identity = anchor.properties?.[type.identity_property];
  }
  if (identity == null || identity === "") throw new Error("The indexed entity has no stable identity. Renderer IDs cannot identify indexed entities.");
  anchor = { ...anchor, id: stableId(type.label, identity), properties: { ...anchor.properties, [type.identity_property]: identity } };
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
  const links = new Map((previous?.links || []).map(e => [JSON.stringify([e.source, e.type, e.target]), e]));
  const pending = partitions.filter(p => p.more || p.error);
  let index = 0;
  // Bound API pressure; obsolete selections stop scheduling further partitions.
  await Promise.all(Array.from({ length: Math.min(4, pending.length) }, async () => {
    while (index < pending.length) {
      check();
      const p = pending[index++];
      try {
        const target = p.target;
        if (![p.type, target.label, target.identity_property].every(safe)) throw new Error("Unsupported schema identifier");
        const properties = [...new Set([target.identity_property, ...["name", "path", "line_number", "lang", "cluster", "docstring"].filter(k => target.properties.includes(k))])];
        // Include another declared property so canonical traversal resolves the reached label.
        if (properties.length === 1) { const extra = target.properties.find(k => k !== target.identity_property && safe(k)); if (extra) properties.push(extra); }
        if (properties.length === 1) throw new Error("No descriptive property is declared for this entity type");
        const fields = properties.map(k => `n.${k} AS ${k === target.identity_property ? "identity" : k}`);
        const pattern = p.direction === "outgoing" ? `(anchor:${type.label})-[:${p.type}]->(n:${target.label})` : `(n:${target.label})-[:${p.type}]->(anchor:${type.label})`;
        const cursor = p.after === undefined ? "" : ` AND n.${target.identity_property} > ${quote(p.after)}`;
        const data = await astApi.getGraph({ context, project_dir: projectDir, ...(projectId ? { project_id: projectId } : {}), signal, cypher_query: `MATCH ${pattern} WHERE anchor.${type.identity_property} = ${quote(String(identity))}${cursor} RETURN DISTINCT ${fields.join(", ")} ORDER BY identity LIMIT ${PAGE_SIZE + 1}` });
        check();
        if (!data.tabular) throw new Error("The index did not return relationship rows");
        const rows = data.tabular.rows;
        let after = p.after;
        const reached: GraphNode[] = [];
        for (const row of rows.slice(0, PAGE_SIZE)) {
          const item = Object.fromEntries(data.tabular.columns.map((key, i) => [key, row[i]]));
          if (item.identity == null || item.identity === "") throw new Error("A related entity has no stable identity");
          after = String(item.identity);
          reached.push({ id: stableId(target.label, item.identity), label: target.label, type: target.label, name: String(item.name || item.path || item.identity), file: target.identity_property === "path" ? String(item.identity) : item.path ? String(item.path) : undefined, line: Number(item.line_number) || undefined, properties: { ...item, [target.identity_property]: item.identity } });
        }
        for (const n of reached) {
          if (n.id !== anchor.id) nodes.set(n.id, n);
          const e = { source: p.direction === "outgoing" ? anchor.id : n.id, target: p.direction === "outgoing" ? n.id : anchor.id, type: p.type };
          links.set(JSON.stringify([e.source, e.type, e.target]), e);
        }
        p.after = after; p.more = rows.length > PAGE_SIZE; p.error = undefined;
      } catch (e) { check(); p.error = (e as Error).message; }
    }
  }));
  check();
  return { anchor, nodes: [...nodes.values()], links: [...links.values()], partitions };
}
