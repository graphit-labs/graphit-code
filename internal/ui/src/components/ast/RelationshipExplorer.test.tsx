import "@/test/contextControls";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { GraphNode } from "@/api/ast";
import { RelationshipExplorer } from "./RelationshipExplorer";
import { boundaries, groupGraph, observedGraph } from "./relationshipModel";

const nodes: GraphNode[] = [
  { id: "a", name: "CreateOrder", label: "Function", type: "Function", file: "orders/service.go", properties: { lang: "go", cluster: 0 } },
  { id: "b", name: "ReserveStock", label: "Function", type: "Function", file: "inventory/stock.go", properties: { lang: "go", cluster: "Fulfillment" } },
  { id: "c", name: "Checkout", label: "Function", type: "Function", file: "web/checkout.ts", properties: { lang: "typescript" } },
  { id: "d", name: "External", label: "Module", type: "Module" },
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
