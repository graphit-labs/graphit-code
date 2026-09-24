import type { GraphEdge, GraphNode } from "@/api/ast";

export type Grouping = "directory" | "file" | "language" | "cluster";
export const languageOf = (n: GraphNode) => String(n.properties?.lang || "Unspecified language");
export const clusterOf = (n: GraphNode) => n.properties?.cluster == null || n.properties.cluster === "" ? "Unassigned cluster" : String(n.properties.cluster);
export function groupOf(n: GraphNode, grouping: Grouping): string {
  if (grouping === "language") return languageOf(n);
  if (grouping === "cluster") return clusterOf(n);
  if (!n.file) return "No source path";
  const file = n.file.replace(/\\/g, "/");
  if (grouping === "file") return file;
  return file.includes("/") ? file.slice(0, file.lastIndexOf("/")) : "Project root";
}
export const byName = (a: GraphNode, b: GraphNode) => (Number(a.properties?.search_rank || 0) - Number(b.properties?.search_rank || 0)) || a.name.localeCompare(b.name) || (a.file || "").localeCompare(b.file || "") || a.id.localeCompare(b.id);

/** Keep parallel indexed relations distinct by stable UID. */
export function observedGraph(nodes: GraphNode[], links: GraphEdge[]) {
  const byId = new Map(nodes.map(n => [n.id, n]));
  const unique = new Map<string, GraphEdge>();
  for (const e of links) {
    if (byId.has(e.source) && byId.has(e.target)) unique.set(e.uid || JSON.stringify([e.source, e.type, e.target]), e);
  }
  return { nodes: [...byId.values()].sort(byName), links: [...unique.values()].sort((a,b) => a.source.localeCompare(b.source) || a.type.localeCompare(b.type) || a.target.localeCompare(b.target) || (a.uid || "").localeCompare(b.uid || "")), byId };
}
export function boundaries(ids: Set<string>, links: GraphEdge[]) {
  return {
    incoming: links.filter(e => !ids.has(e.source) && ids.has(e.target)),
    outgoing: links.filter(e => ids.has(e.source) && !ids.has(e.target)),
    internal: links.filter(e => ids.has(e.source) && ids.has(e.target)),
  };
}
export function groupGraph(nodes: GraphNode[], links: GraphEdge[], grouping: Grouping) {
  const groups = new Map<string, GraphNode[]>();
  for (const n of nodes) {
    const key = groupOf(n, grouping);
    const group = groups.get(key) || [];
    group.push(n);
    groups.set(key, group);
  }
  return [...groups].map(([name, members]) => ({ name, members: members.sort(byName), ...boundaries(new Set(members.map(n => n.id)), links) })).sort((a,b) => a.name.localeCompare(b.name));
}
