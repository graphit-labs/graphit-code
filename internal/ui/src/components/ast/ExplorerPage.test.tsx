import "@/test/contextControls";
import { WorkspaceRefreshProvider } from "@/components/layout/WorkspaceRefresh";
import { WorkspaceSelectors } from "@/components/layout/WorkspaceSelectors";
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, cleanup, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { astApi } from "@/api/ast";
import { useAppStore } from "@/store/appStore";
import ExplorerPage from "./ExplorerPage";
vi.mock("@/api/ast", () => ({
  astApi: {
    getSchema: vi.fn(),
    getContexts: vi.fn(),
    getGraph: vi.fn(),
    getFile: vi.fn(),
    search: vi.fn(),
    generateCypher: vi.fn(),
  },
}));
vi.mock("./GraphCanvas", () => ({ GraphCanvas: () => null }));
vi.mock("./CodePanel", () => ({
  CodePanel: ({ content }: { content: string }) => <pre>{content}</pre>,
}));
const empty = { nodes: [], links: [], files: [], fileContents: {} };
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({ activeProjectDir: "/project", projectsError: "", loadProjects: vi.fn(async () => {}) });
  vi.mocked(astApi.getSchema).mockResolvedValue({
    nodes: [{ label: "Function", count: 1000 }],
    edges: [{ type: "CALLS", count: 500 }],
    node_labels: ["Function"],
    edge_types: ["CALLS"],
    backend: "index",
    node_types: [
      {
        label: "Function",
        identity_property: "uid",
        properties: ["uid", "name", "path", "line_number"],
      },
    ],
    relationship_endpoints: [
      { type: "CALLS", from: "Function", to: "Function" },
    ],
  });
  vi.mocked(astApi.getContexts).mockResolvedValue({
    contexts: [],
    project_root: "/project",
    project_name: "Demo",
  });
  vi.mocked(astApi.getGraph).mockResolvedValue(empty);
  vi.mocked(astApi.search).mockResolvedValue([
    { Name: "validate", Type: "Function", Path: "src/check.ts", Line: 12 },
  ]);
  vi.mocked(astApi.getFile).mockResolvedValue({
    content: "export function validate() {}",
    source: "indexed",
  });
});
afterEach(cleanup);
function setup() {
  render(
    <MemoryRouter initialEntries={["/ast/explorer/library"]}>
      <WorkspaceRefreshProvider><WorkspaceSelectors /><Routes>
        <Route path="/ast/explorer/:contextId?" element={<ExplorerPage />} />
      </Routes></WorkspaceRefreshProvider>
    </MemoryRouter>,
  );
  return userEvent.setup();
}
it("searches the full scoped index without loading a sample, then reads indexed source", async () => {
  const user = setup();
  await user.type(screen.getByLabelText("Search indexed code"), "validate");
  await user.click(screen.getByRole("button", { name: "Search index" }));
  await user.click(
    await screen.findByRole("button", { name: /validate Function/ }),
  );
  expect(astApi.search).toHaveBeenCalledWith("validate", "library", "/project");
  expect(astApi.getGraph).not.toHaveBeenCalled();
  expect(await screen.findByText("export function validate() {}")).toBeTruthy();
  expect(astApi.getFile).toHaveBeenCalledWith(
    "src/check.ts",
    "library",
    "/project",
  );
  vi.mocked(astApi.getGraph)
    .mockResolvedValueOnce({
      ...empty,
      nodes: [
        {
          id: "render:1",
          properties: { uid: "indexed-uid" },
          name: "validate",
          label: "Function",
          type: "Function",
          file: "src/check.ts",
          line: 12,
        },
      ],
    })
    .mockResolvedValue({
      ...empty,
      tabular: {
        columns: ["identity", "name", "path", "line"],
        rows: [["u", "caller", "src/caller.ts", 8]],
      },
    });
  await user.click(screen.getByRole("tab", { name: "Incoming" }));
  expect(await screen.findByRole("button", { name: /caller/ })).toBeTruthy();
  expect(astApi.getGraph).toHaveBeenLastCalledWith({
    context: "library",
    project_dir: "/project",
    cypher_query: expect.stringContaining(
      "WHERE anchor.uid = 'indexed-uid' RETURN DISTINCT n.uid AS identity",
    ),
  });
});
it("discards search results returned after switching project", async () => {
  const user = setup();
  let finish: (r: any) => void = () => {};
  vi.mocked(astApi.search).mockImplementationOnce(
    () =>
      new Promise((r) => {
        finish = r;
      }),
  );
  await user.type(screen.getByLabelText("Search indexed code"), "old");
  await user.click(screen.getByRole("button", { name: "Search index" }));
  act(() => useAppStore.setState({ activeProjectDir: "/new" }));
  await act(async () =>
    finish([{ Name: "stale", Type: "Function", Path: "old.ts", Line: 1 }]),
  );
  expect(screen.queryByRole("button", { name: /stale/ })).toBeNull();
  await waitFor(() =>
    expect(astApi.getSchema).toHaveBeenCalledWith("library", "/new"),
  );
});

it("refuses to merge relationships from ambiguous indexed symbols", async () => {
  const user = setup();
  await user.type(screen.getByLabelText("Search indexed code"), "validate");
  await user.click(screen.getByRole("button", { name: "Search index" }));
  await user.click(
    await screen.findByRole("button", { name: /validate Function/ }),
  );
  vi.mocked(astApi.getGraph).mockResolvedValue({
    ...empty,
    nodes: [
      { id: "one", name: "validate", label: "Function", type: "Function" },
      { id: "two", name: "validate", label: "Function", type: "Function" },
    ],
  });
  await user.click(screen.getByRole("tab", { name: "Incoming" }));
  expect(await screen.findByText(/Multiple indexed symbols/)).toBeTruthy();
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
});

it("traces file imports using path identity and Module-only properties", async () => {
  vi.mocked(astApi.getSchema).mockResolvedValue({
    nodes: [],
    edges: [{ type: "IMPORTS", count: 1 }],
    node_labels: ["File", "Module"],
    edge_types: ["IMPORTS"],
    backend: "index",
    node_types: [
      {
        label: "File",
        identity_property: "path",
        properties: ["path", "name"],
      },
      {
        label: "Module",
        identity_property: "uid",
        properties: ["uid", "name"],
      },
    ],
    relationship_endpoints: [{ type: "IMPORTS", from: "File", to: "Module" }],
  });
  vi.mocked(astApi.search).mockResolvedValue([
    { Name: "check.ts", Type: "File", Path: "src/check.ts", Line: 0 },
  ]);
  const user = setup();
  await user.type(screen.getByLabelText("Search indexed code"), "check");
  await user.click(screen.getByRole("button", { name: "Search index" }));
  await user.click(
    await screen.findByRole("button", { name: /check.ts File/ }),
  );
  await user.click(screen.getByRole("tab", { name: "Outgoing" }));
  await user.click(screen.getByLabelText("Relationship type"));
  await user.click(screen.getByRole("option", { name: "IMPORTS" }));
  await waitFor(() =>
    expect(astApi.getGraph).toHaveBeenLastCalledWith({
      context: "library",
      project_dir: "/project",
      cypher_query:
        "MATCH (anchor:File)-[:IMPORTS]->(n:Module) WHERE anchor.path = 'src/check.ts' RETURN DISTINCT n.uid AS identity, n.name AS name LIMIT 100",
    }),
  );
});

it("never uses renderer IDs as persistent symbol identity", async () => {
  const user = setup();
  await user.type(screen.getByLabelText("Search indexed code"), "validate");
  await user.click(screen.getByRole("button", { name: "Search index" }));
  await user.click(
    await screen.findByRole("button", { name: /validate Function/ }),
  );
  vi.mocked(astApi.getGraph).mockResolvedValue({
    ...empty,
    nodes: [
      { id: "render:1", name: "validate", label: "Function", type: "Function" },
    ],
  });
  await user.click(screen.getByRole("tab", { name: "Incoming" }));
  expect(await screen.findByText(/Renderer IDs cannot/)).toBeTruthy();
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
});

it("keeps a newer explicit query when an older graph refresh resolves last", async () => {
  const user = setup();
  await user.click(screen.getByRole("tab", { name: "Query lab" }));
  const editor = screen.getByLabelText("Cypher query");
  await user.type(editor, "MATCH (n) RETURN n LIMIT 1");
  await user.click(screen.getByRole("button", { name: "Run query" }));
  let finish!: (value: any) => void;
  vi.mocked(astApi.getGraph).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  await user.clear(editor); await user.type(editor, "MATCH (n) RETURN n LIMIT 2");
  vi.mocked(astApi.getGraph).mockResolvedValueOnce({ ...empty, tabular: { columns: ["name"], rows: [["New result"]] } });
  await user.click(screen.getByRole("button", { name: "Run query" }));
  expect(await screen.findByText("New result")).toBeTruthy();
  await act(async () => finish({ ...empty, tabular: { columns: ["name"], rows: [["Stale result"]] } }));
  expect(screen.queryByText("Stale result")).toBeNull();
  expect(screen.getByText("New result")).toBeTruthy();
});

it('loads a bounded sample on first map entry with accessible boundary controls', async () => {
  const user = setup();
  await user.click(screen.getByRole('tab', { name: 'Relationship map' }));
  await waitFor(() => expect(astApi.getGraph).toHaveBeenCalledTimes(1));
  expect(astApi.getGraph).toHaveBeenCalledWith({ context: 'library', project_dir: '/project' });
  expect(screen.getByLabelText('Organize by')).toBeTruthy();
  expect(screen.getByText('No graph entities in this result')).toBeTruthy();
  await user.click(screen.getByRole('tab', { name: 'Query lab' }));
  await user.click(screen.getByRole('tab', { name: 'Relationship map' }));
  expect(astApi.getGraph).toHaveBeenCalledTimes(1);
});
it('renders returned scalar and mixed query rows in the result viewer', async () => {
  vi.mocked(astApi.getGraph).mockResolvedValue({ ...empty, nodes: [{ id: '1', name: 'Create', label: 'Function', type: 'Function' }], tabular: { columns: ['name'], rows: [['Create']] } });
  const user = setup();
  await user.click(screen.getByRole('tab', { name: 'Query lab' }));
  await user.type(screen.getByLabelText('Cypher query'), 'MATCH (n:Function) RETURN n, n.name AS name LIMIT 2');
  await user.click(screen.getByRole('button', { name: 'Run query' }));
  await screen.findByRole('cell', { name: 'Create' });
  expect(screen.getByText('1 result rows')).toBeTruthy();
});
