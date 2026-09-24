import "@/test/contextControls";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { GraphNode } from "@/api/ast";
import type { Neighborhood, NeighborhoodLoader } from "./neighborhood";
import { RelationshipExplorer } from "./RelationshipExplorer";
import { boundaries, groupGraph, observedGraph } from "./relationshipModel";

const nodes: GraphNode[] = [
  { id: "a", name: "CreateOrder", label: "Function", type: "Function", file: "orders/service.go", properties: { uid: "orders/service.go::CreateOrder", lang: "go", cluster: 0 } },
  { id: "b", name: "ReserveStock", label: "Function", type: "Function", file: "inventory/stock.go", properties: { uid: "inventory/stock.go::ReserveStock", lang: "go", cluster: "Fulfillment" } },
  { id: "c", name: "Checkout", label: "Function", type: "Function", file: "web/checkout.ts", properties: { uid: "web/checkout.ts::Checkout", lang: "typescript" } },
  { id: "d", name: "External", label: "Module", type: "Module", properties: { uid: "external::Module" } },
];
const links = [
  { source: "c", target: "a", type: "CALLS" },
  { source: "a", target: "b", type: "CALLS" },
  { source: "a", target: "b", type: "CALLS" },
  { source: "a", target: "b", type: "REFERENCES" },
  { source: "a", target: "a", type: "CALLS" },
  { source: "b", target: "missing", type: "CALLS" },
];
afterEach(cleanup);
it("orders and deduplicates observed triples without inventing missing endpoints or reversing direction", () => {
  const g = observedGraph(nodes, links);
  expect(g).toEqual(observedGraph([...nodes].reverse(), [...links].reverse()));
  expect(g.links).toHaveLength(4);
  const b = boundaries(new Set(["a"]), g.links);
  expect(b.incoming.map(e => e.source)).toEqual(["c"]);
  expect(b.outgoing.map(e => e.type)).toEqual(["CALLS", "REFERENCES"]);
  expect(b.internal).toEqual([{ source: "a", target: "a", type: "CALLS" }]);
  expect(groupGraph(g.nodes, g.links, "cluster").map(x=>x.name)).toEqual(["0", "Fulfillment", "Unassigned cluster"]);
  expect(groupGraph(g.nodes, g.links, "directory").some(x=>x.name === "No source path")).toBe(true);
});
it("keeps parallel relation UIDs as separate rendered links and count", async () => {
  const parallel = [
    { source: "a", target: "b", type: "CALLS", id: "native:1", uid: "rel:one" },
    { source: "a", target: "b", type: "CALLS", id: "native:2", uid: "rel:two" },
  ];
  expect(observedGraph(nodes, parallel).links).toHaveLength(2);
  const user = userEvent.setup();
  render(<RelationshipExplorer nodes={nodes} links={parallel} onInspect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: /^orders 1 entities/ }));
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  expect(screen.getByRole("heading", { name: "Outgoing 2" })).toBeTruthy();
  expect(document.querySelectorAll('[data-relationship-uid="rel:one"]')).toHaveLength(1);
  expect(document.querySelectorAll('[data-relationship-uid="rel:two"]')).toHaveLength(1);
  expect(document.querySelector('[data-relationship-uid="rel:one"]')?.getAttribute("data-native-relation-id")).toBe("native:1");
  expect(document.querySelector('[data-relationship-uid="rel:two"]')?.getAttribute("data-native-relation-id")).toBe("native:2");
});
it("follows directed neighbors locally, retains focus on refresh, and opens source only on explicit action", async () => {
  const onInspect = vi.fn(); const user = userEvent.setup();
  const { rerender } = render(<RelationshipExplorer nodes={nodes} links={links} onInspect={onInspect} />);
  await user.click(screen.getByRole("button", { name: /^orders 1 entities/ }));
  await user.click(within(screen.getByRole("region", { name: "Relationship evidence" })).getByRole("button", { name: /^CreateOrder orders/ }));
  expect(screen.getByRole("heading", { name: "CreateOrder" })).toBeTruthy();
  expect(onInspect).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(screen.getByRole("region", { name: "Relationship evidence" }));
  expect(screen.getByText("Self relationships: CALLS")).toBeTruthy();
  rerender(<RelationshipExplorer nodes={[...nodes].reverse()} links={[...links].reverse()} onInspect={onInspect} />);
  expect(screen.getByRole("heading", { name: "CreateOrder" })).toBeTruthy();
  await user.click(screen.getAllByRole("button", { name: /^ReserveStock inventory/ })[0]);
  expect(screen.getByRole("heading", { name: "ReserveStock" })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Back" }));
  await user.click(screen.getByRole("button", { name: "Inspect source & impact" }));
  expect(onInspect).toHaveBeenCalledWith(nodes[0]);
});
it("keeps unknown language and source paths honest, and offers recovery from filtered empty", async () => {
  const user = userEvent.setup();
  render(<RelationshipExplorer nodes={nodes} links={links} onInspect={vi.fn()} />);
  await user.click(screen.getByLabelText("Entity type"));
  await user.click(screen.getByRole("option", { name: /^Module$/ }));
  expect(screen.getAllByText("No source path").length).toBeGreaterThan(0);
  expect(screen.getByText("No indexed source path")).toBeTruthy();
  await user.type(screen.getByLabelText("Find a boundary or entity"), "not-found");
  expect(screen.getByText("No matching entities")).toBeTruthy();
  await user.click(screen.getAllByRole("button", { name: "Clear filters" })[0]);
  expect(screen.getByRole("button", { name: /^orders 1 entities/ })).toBeTruthy();
});

it("does not transfer selection when a refreshed result reuses a renderer ID for another identity", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<RelationshipExplorer nodes={nodes} links={links} onInspect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: /^orders 1 entities/ }));
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  const replacement = nodes.map(n => n.id === "a" ? { ...n, name: "DeleteOrder", properties: { ...n.properties, uid: "different" } } : n);
  rerender(<RelationshipExplorer nodes={replacement} links={links} onInspect={vi.fn()} />);
  expect(screen.queryByRole("heading", { name: "DeleteOrder" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Inspect source & impact" })).toBeNull();
});

it("refuses local selection without indexed identity instead of using the renderer ID", async () => {
  const user = userEvent.setup();
  const loader = vi.fn<NeighborhoodLoader>();
  const missing = { ...nodes[0], properties: { lang: "go" } };
  render(<RelationshipExplorer nodes={[missing]} links={[]} onInspect={vi.fn()} loadNeighborhood={loader} />);
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  expect(screen.getByRole("alert").textContent).toMatch(/Indexed identity unavailable/);
  expect(screen.getByRole("heading", { name: "CreateOrder" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Inspect source & impact" })).toBeTruthy();
  expect(loader).not.toHaveBeenCalled();
});

const neighborhoodFor = (anchor: GraphNode, neighbor = nodes[1]): Neighborhood => ({ anchor, nodes: [anchor, neighbor], links: [{ source: anchor.id, target: neighbor.id, type: "CALLS" }], partitions: [] });
it("loads neighbors outside the result, follows them and returns without changing the catalogue", async () => {
  const user = userEvent.setup();
  const loader = vi.fn<NeighborhoodLoader>().mockImplementation(async node => neighborhoodFor(node, node.id === "b" ? nodes[2] : nodes[1]));
  render(<RelationshipExplorer nodes={[nodes[0]]} links={[]} onInspect={vi.fn()} loadNeighborhood={loader} />);
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  await user.click(await screen.findByRole("button", { name: /^ReserveStock inventory/ }));
  expect(await screen.findByRole("button", { name: /^Checkout web/ })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "ReserveStock" })).toBeTruthy();
  const catalogue = screen.getByRole("complementary", { name: "Result boundaries" });
  expect(within(catalogue).getByRole("button", { name: /^orders 1 entities/ })).toBeTruthy();
  expect(within(catalogue).queryByText("inventory")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Back" }));
  await waitFor(() => expect(loader).toHaveBeenCalledTimes(3));
  expect(screen.getByRole("heading", { name: "CreateOrder" })).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "All boundaries" }));
  expect(screen.getByRole("button", { name: /^CreateOrder orders/ })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^ReserveStock inventory/ })).toBeNull();
});
it("does not replace a newer selection with late neighborhood results", async () => {
  const user = userEvent.setup();
  let finish!: (n: Neighborhood) => void;
  const loader = vi.fn<NeighborhoodLoader>().mockImplementationOnce(() => new Promise(r => { finish = r; })).mockImplementation(async node => neighborhoodFor(node, nodes[2]));
  render(<RelationshipExplorer nodes={[nodes[0], nodes[1]]} links={[]} onInspect={vi.fn()} loadNeighborhood={loader} />);
  await user.click(screen.getByRole("button", { name: /^orders 1 entities/ }));
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  expect(screen.getByText("Loading indexed neighborhood…")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: /^inventory 1 entities/ }));
  await user.click(screen.getByRole("button", { name: /^ReserveStock inventory/ }));
  await screen.findByRole("button", { name: /^Checkout web/ });
  await act(async () => finish(neighborhoodFor(nodes[0])));
  expect(screen.getByRole("heading", { name: "ReserveStock" })).toBeTruthy();
  expect(loader.mock.calls[0][2]?.aborted).toBe(true);
});
it("reports failed neighborhood lookup and retries without an empty-success message", async () => {
  const user = userEvent.setup();
  const loader = vi.fn<NeighborhoodLoader>().mockRejectedValueOnce(new Error("Index unavailable")).mockResolvedValue(neighborhoodFor(nodes[0]));
  render(<RelationshipExplorer nodes={[nodes[0]]} links={[]} onInspect={vi.fn()} loadNeighborhood={loader} />);
  await user.click(screen.getByRole("button", { name: /^CreateOrder orders/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Index unavailable");
  expect(screen.queryByText(/No incoming relationships found/)).toBeNull();
  await user.click(screen.getByRole("button", { name: "Retry neighborhood" }));
  expect(await screen.findByRole("button", { name: /^ReserveStock inventory/ })).toBeTruthy();
});
