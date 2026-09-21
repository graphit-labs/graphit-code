import { beforeEach, expect, it, vi } from "vitest";
import { astApi, type GraphNode, type SchemaResponse } from "@/api/ast";
import { loadNeighborhood } from "./neighborhood";
vi.mock("@/api/ast", () => ({ astApi: { getGraph: vi.fn() } }));
const empty = { nodes: [], links: [], files: [], fileContents: {} };
const rows = (values: unknown[][]) => ({ ...empty, tabular: { columns: ["identity", "name", "path", "lang"], rows: values } });
const node: GraphNode = { id: "renderer:7", label: "Function", type: "Function", name: "Checkout", file: "src/order.ts", line: 4, properties: { uid: "f1" } };
const schema: SchemaResponse = {
  nodes: [], edges: [], node_labels: [], edge_types: [], backend: "index",
  node_types: [{ label: "Function", identity_property: "uid", properties: ["uid", "name", "path", "line_number", "lang"] }, { label: "File", identity_property: "path", properties: ["path", "name"] }],
  relationship_endpoints: [{ type: "CALLS", from: "Function", to: "Function" }, { type: "DECLARES", from: "File", to: "Function" }],
};
beforeEach(() => { vi.resetAllMocks(); vi.mocked(astApi.getGraph).mockResolvedValue(rows([])); });
it("queries every typed direction independently, preserves scope and constructs only evidenced direct edges", async () => {
  vi.mocked(astApi.getGraph).mockImplementation(async ({ cypher_query: q }) => q!.includes("DECLARES") ? rows([["src/order.ts", "order.ts", "src/order.ts", "typescript"]]) : q!.startsWith("MATCH (anchor:") ? rows([["f2", "Ship", "src/ship.ts", "typescript"], ["f1", "Checkout", "src/order.ts", "typescript"]]) : rows([["f3", "Handle", "src/http.ts", "typescript"]]));
  const result = await loadNeighborhood(node, schema, "installed", "/example");
  expect(astApi.getGraph).toHaveBeenCalledTimes(3);
  for (const [args] of vi.mocked(astApi.getGraph).mock.calls) {
    expect(args).toMatchObject({ context: "installed", project_dir: "/example" });
    expect(args.cypher_query).toContain("anchor.uid = 'f1'");
    expect(args.cypher_query).not.toContain("renderer:7");
    expect(args.cypher_query).toContain("ORDER BY identity LIMIT 101");
  }
  expect(result.links).toContainEqual({ source: JSON.stringify(["Function","f3"]), target: result.anchor.id, type: "CALLS" });
  expect(result.links).toContainEqual({ source: result.anchor.id, target: JSON.stringify(["Function","f2"]), type: "CALLS" });
  expect(result.links.filter(e => e.source === e.target)).toEqual([{ source: result.anchor.id, target: result.anchor.id, type: "CALLS" }]);
  expect(result.nodes).toHaveLength(4);
  expect(node.id).toBe("renderer:7");
});
it("resolves missing search identity by exact typed location and refuses ambiguity", async () => {
  const search = { ...node, properties: {} };
  vi.mocked(astApi.getGraph).mockResolvedValueOnce({ ...empty, nodes: [node] });
  await loadNeighborhood(search, schema);
  expect(astApi.getGraph).toHaveBeenNthCalledWith(1, expect.objectContaining({ cypher_query: "MATCH (n:Function) WHERE n.name = 'Checkout' AND n.path = 'src/order.ts' AND n.line_number = 4 RETURN n LIMIT 2" }));
  vi.mocked(astApi.getGraph).mockClear().mockResolvedValueOnce({ ...empty, nodes: [node, node] });
  await expect(loadNeighborhood(search, schema)).rejects.toThrow("Multiple indexed entities");
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
});
it("pages only unfinished partitions using the last visible identity, not the lookahead", async () => {
  const values = Array.from({ length: 101 }, (_, i) => [String(i).padStart(3,"0"), "Neighbor" + i]);
  vi.mocked(astApi.getGraph).mockImplementation(async ({ cypher_query: q }) => q!.startsWith("MATCH (anchor:") ? rows(values) : rows([]));
  const first = await loadNeighborhood(node, schema);
  expect(first.links).toHaveLength(100);
  expect(first.partitions.filter(p => p.more)).toHaveLength(1);
  vi.mocked(astApi.getGraph).mockClear().mockResolvedValue(rows([["100", "Last"]]));
  const second = await loadNeighborhood(node, schema, undefined, undefined, first);
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
  expect(astApi.getGraph).toHaveBeenCalledWith(expect.objectContaining({ cypher_query: expect.stringContaining("AND n.uid > '099'") }));
  expect(second.links).toHaveLength(101);
  expect(second.partitions.some(p => p.more)).toBe(false);
});
it("retains successful partitions and retries failures without losing evidence", async () => {
  vi.mocked(astApi.getGraph).mockImplementation(async ({ cypher_query: q }) => { if (q!.includes("DECLARES")) throw new Error("Offline"); return rows([["f2", "Ship"]]); });
  const first = await loadNeighborhood(node, schema);
  expect(first.partitions.filter(p => p.error)).toHaveLength(1);
  expect(first.links).toHaveLength(2);
  vi.mocked(astApi.getGraph).mockClear().mockResolvedValue(rows([]));
  const retry = await loadNeighborhood(node, schema, undefined, undefined, first);
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
  expect(retry.links).toEqual(first.links);
  expect(retry.partitions.some(p => p.error)).toBe(false);
});
it("uses File path identity and never queries undeclared fields", async () => {
  await loadNeighborhood({ ...node, label: "File", properties: {}, file: "src/order.ts" }, schema);
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
  expect(astApi.getGraph).toHaveBeenCalledWith(expect.objectContaining({ cypher_query: expect.stringContaining("anchor.path = 'src/order.ts'") }));
});
it("fails closed on missing identity or metadata and stops cancelled work", async () => {
  await expect(loadNeighborhood(node, { ...schema, node_types: [] })).rejects.toThrow("metadata");
  vi.mocked(astApi.getGraph).mockResolvedValue({ ...empty, nodes: [{ ...node, properties: {} }] });
  await expect(loadNeighborhood({ ...node, properties: {} }, schema)).rejects.toThrow("Renderer IDs");
  vi.mocked(astApi.getGraph).mockClear();
  const controller = new AbortController(); controller.abort();
  await expect(loadNeighborhood(node, schema, undefined, undefined, undefined, controller.signal)).rejects.toThrow();
  expect(astApi.getGraph).not.toHaveBeenCalled();
});
