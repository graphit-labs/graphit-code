import { beforeEach, expect, it, vi } from "vitest";
import { astApi, type GraphNode, type GraphNeighborhoodResponse, type SchemaResponse } from "@/api/ast";
import { loadNeighborhood } from "./neighborhood";

vi.mock("@/api/ast", () => ({ astApi: { getGraphNeighborhood: vi.fn() } }));
const node: GraphNode = { id: "native:4", label: "Function", type: "Function", name: "Checkout", properties: { uid: "fn:A" } };
const schema: SchemaResponse = {
  nodes: [], edges: [], node_labels: [], edge_types: [], backend: "index",
  node_types: [{ label: "Function", identity_property: "uid", properties: ["uid", "name"] }],
  relationship_endpoints: [{ type: "CALLS", from: "Function", to: "Function" }],
};
const result = (sourceId: string, targetId: string, uids: string[], next_cursor = "", index_generation = "v1"): GraphNeighborhoodResponse => ({
  nodes: [
    { id: sourceId, label: "Function", type: "Function", name: "Checkout", properties: { uid: "fn:A" } },
    { id: targetId, label: "Function", type: "Function", name: "Ship", properties: { uid: "fn:B" } },
  ],
  links: uids.map(uid => ({ source: sourceId, target: targetId, type: "CALLS", id: `native:${uid}`, uid })),
  next_cursor,
  index_generation,
});

beforeEach(() => { vi.resetAllMocks(); vi.mocked(astApi.getGraphNeighborhood).mockResolvedValue({ nodes: [], links: [], next_cursor: "" }); });

it("preserves two parallel relations by UID and resolves endpoints by node UID", async () => {
  vi.mocked(astApi.getGraphNeighborhood).mockImplementation(async args => args.direction === "outgoing" ? result("old:1", "old:2", ["rel:1", "rel:2"]) : { nodes: [], links: [], next_cursor: "" });
  const graph = await loadNeighborhood(node, schema, "installed", "/project");
  expect(graph.links).toHaveLength(2);
  expect(graph.links.map(e => e.uid)).toEqual(["rel:1", "rel:2"]);
  expect(graph.links[0]).toMatchObject({ source: JSON.stringify(["Function", "fn:A"]), target: JSON.stringify(["Function", "fn:B"]) });
  expect(graph.anchor.id).toBe(JSON.stringify(["Function", "fn:A"]));
  expect(node.id).toBe("native:4");
  expect(astApi.getGraphNeighborhood).toHaveBeenCalledWith(expect.objectContaining({ anchor_identity: "fn:A", context: "installed", project_dir: "/project" }));
});

it("refreshes the UID anchored neighborhood when reindex changes its generation", async () => {
  vi.mocked(astApi.getGraphNeighborhood).mockImplementation(async args => args.direction === "outgoing" ? result("old:1", "old:2", ["rel:1"], "rel:1") : { nodes: [], links: [], next_cursor: "" });
  const first = await loadNeighborhood(node, schema);
  vi.mocked(astApi.getGraphNeighborhood).mockClear().mockImplementation(async args => args.direction === "outgoing" ? result("new:9", "new:10", ["rel:2"], "", "v2") : { nodes: [], links: [], next_cursor: "", index_generation: "v2" });
  const second = await loadNeighborhood(node, schema, undefined, undefined, first);
  expect(astApi.getGraphNeighborhood).toHaveBeenCalledTimes(3);
  expect(astApi.getGraphNeighborhood).toHaveBeenCalledWith(expect.objectContaining({ cursor: "rel:1", anchor_identity: "fn:A" }));
  expect(second.nodes).toHaveLength(2);
  expect(second.links.map(e => e.uid)).toEqual(["rel:2"]);
  expect(second.links.every(e => e.source === first.anchor.id)).toBe(true);
  expect(second.indexGeneration).toBe("v2");
});

it("offers retry when the index changes again during a refresh", async () => {
  vi.mocked(astApi.getGraphNeighborhood).mockImplementation(async args => args.direction === "outgoing" ? result("old:1", "old:2", ["rel:1"], "rel:1") : { nodes: [], links: [], next_cursor: "", index_generation: "v1" });
  const first = await loadNeighborhood(node, schema);
  vi.mocked(astApi.getGraphNeighborhood).mockImplementation(async args => {
    if (args.cursor) return result("new:1", "new:2", ["rel:2"], "", "v2");
    return args.direction === "incoming"
      ? { nodes: [], links: [], next_cursor: "", index_generation: "v3" }
      : result("new:1", "new:2", ["rel:2"], "", "v2");
  });
  await expect(loadNeighborhood(node, schema, undefined, undefined, first)).rejects.toThrow("Index changed during exploration. Retry");
});

it("rejects missing node identity and records a recoverable missing relation UID error", async () => {
  await expect(loadNeighborhood({ ...node, properties: {} }, schema)).rejects.toThrow("no indexed unique identifier");
  vi.mocked(astApi.getGraphNeighborhood).mockResolvedValue(result("1", "2", [""]));
  const graph = await loadNeighborhood(node, schema);
  expect(graph.links).toHaveLength(0);
  expect(graph.partitions.find(p => p.direction === "outgoing")?.error).toContain("stable UID");
});

it("uses a pathless SCIP symbol UID and retries only a failed partition", async () => {
  const external = { ...node, name: "Run", file: undefined, properties: { uid: "scip:os/exec.Cmd#Run()." } };
  vi.mocked(astApi.getGraphNeighborhood).mockImplementation(async args => {
    if (args.direction === "outgoing") throw new Error("Temporary index failure");
    return { nodes: [], links: [], next_cursor: "" };
  });
  const first = await loadNeighborhood(external, schema);
  expect(first.partitions.filter(p => p.error)).toHaveLength(1);
  expect(astApi.getGraphNeighborhood).toHaveBeenCalledWith(expect.objectContaining({ anchor_identity: "scip:os/exec.Cmd#Run()." }));
  vi.mocked(astApi.getGraphNeighborhood).mockClear().mockResolvedValue({ nodes: [], links: [], next_cursor: "" });
  const retried = await loadNeighborhood(external, schema, undefined, undefined, first);
  expect(astApi.getGraphNeighborhood).toHaveBeenCalledTimes(1);
  expect(retried.partitions.every(p => !p.error)).toBe(true);
});

it("stops before issuing a request when selection is cancelled", async () => {
  const controller = new AbortController();
  controller.abort();
  await expect(loadNeighborhood(node, schema, undefined, undefined, undefined, controller.signal)).rejects.toThrow();
  expect(astApi.getGraphNeighborhood).not.toHaveBeenCalled();
});
