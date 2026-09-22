import "@/test/contextControls";
import { WorkspaceRefreshProvider } from "@/components/layout/WorkspaceRefresh";
import { WorkspaceSelectors } from "@/components/layout/WorkspaceSelectors";
import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { render, screen, cleanup, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { useAppStore } from "@/store/appStore";
import * as wiki from "@/api/wiki";
import { hubApi } from "@/api/hub";
import WikiExplorerPage from "./WikiExplorerPage";
vi.mock("@/api/wiki", () => ({
  fetchModules: vi.fn(),
  fetchPages: vi.fn(),
  fetchPage: vi.fn(),
  searchWiki: vi.fn(),
  aiSearchWiki: vi.fn(),
}));
vi.mock("@/api/hub", () => ({ hubApi: { getProjectContexts: vi.fn() } }));
const meta = {
  path: "rules.md",
  title: "Engineering rules",
  type: "specification",
  wordCount: 50,
  links: [],
  tags: ["engineering"],
  source: "docs/rules.md",
  confidence: 1,
};
beforeEach(() => {
  vi.resetAllMocks();
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
  useAppStore.setState({ activeProjectKey: "workspace:demo:/project", activeProjectOrigin: "workspace", activeProjectId: "demo", activeProjectDir: "/project", projectsError: "", loadProjects: vi.fn(async () => {}) });
  vi.mocked(hubApi.getProjectContexts).mockResolvedValue({
    project: { id: "01HUB", name: "Remote", revision: 1, status: "active" },
    live: { task: true, memory: true }, entries: [],
  });
  vi.mocked(wiki.fetchModules).mockResolvedValue([
    {
      id: "knowledge",
      label: "Project knowledge",
      context: "project",
      path: "/wiki",
      pages: 1,
      hasLog: false,
    },
  ]);
  vi.mocked(wiki.fetchPages).mockResolvedValue([meta]);
  vi.mocked(wiki.fetchPage).mockResolvedValue({
    ...meta,
    content: "# Engineering rules\n\nKeep evidence attached.",
  });
  vi.mocked(wiki.searchWiki).mockResolvedValue([
    {
      path: "rules.md",
      title: "Engineering rules",
      snippet: "Keep evidence attached.",
      score: 2,
    },
  ]);
});
afterEach(cleanup);
it("resolves latest and pinned Knowledge versions to exact Hub contexts", async () => {
  const user = userEvent.setup();
  useAppStore.setState({
    activeProjectKey: "hub:01HUB", activeProjectOrigin: "hub", activeProjectId: "01HUB",
    activeProjectDir: "", projectName: "Remote",
  });
  vi.mocked(hubApi.getProjectContexts).mockResolvedValue({
    project: { id: "01HUB", name: "Remote", revision: 1, status: "active" },
    live: { task: true, memory: true },
    entries: [{ id: "knowledge-artifact", name: "System knowledge", type: "knowledge", latest: "2.0.0", versions: ["1.0.0", "2.0.0"], qualified_latest: "knowledge-artifact@2.0.0" }],
  });

  render(<MemoryRouter initialEntries={["/knowledge/explorer"]}><WikiExplorerPage /></MemoryRouter>);
  await waitFor(() => expect(wiki.fetchPages).toHaveBeenCalledWith("", {
    projectId: "01HUB", context: "knowledge-artifact@2.0.0",
  }));
  expect(screen.getByText(/Reading exact context/).textContent).toContain("knowledge-artifact@2.0.0");

  await user.click(screen.getByRole("combobox", { name: "Knowledge version" }));
  await user.click(screen.getByRole("option", { name: "1.0.0" }));
  await waitFor(() => expect(wiki.fetchPages).toHaveBeenCalledWith("", {
    projectId: "01HUB", context: "knowledge-artifact@1.0.0",
  }));
});
it("opens a library document with provenance and preserves raw Markdown", async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Engineering rules" }),
  );
  expect(await screen.findByText("Keep evidence attached.")).toBeTruthy();
  expect(screen.getByText("docs/rules.md")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "View Markdown" }));
  expect(screen.getByText(/# Engineering rules/)).toBeTruthy();
  expect(wiki.fetchPage).toHaveBeenCalledWith("/wiki", "rules.md");
});
it("resolves a keyword result to its authoritative document", async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await screen.findByRole("button", { name: "Engineering rules" });
  await user.type(screen.getByLabelText("Search knowledge"), "evidence");
  await user.click(screen.getByRole("button", { name: "Find" }));
  await user.click(
    await screen.findByRole("button", {
      name: /Engineering rules rules.md/,
    }),
  );
  expect(await screen.findByText("Keep evidence attached.")).toBeTruthy();
  expect(wiki.searchWiki).toHaveBeenCalledWith("/wiki", "evidence");
});
it("does not show a document that arrives after the project changes", async () => {
  const user = userEvent.setup();
  let finish: (v: any) => void = () => {};
  vi.mocked(wiki.fetchPage).mockImplementationOnce(
    () =>
      new Promise((r) => {
        finish = r;
      }),
  );
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Engineering rules" }),
  );
  act(() => useAppStore.setState({ activeProjectKey: "workspace:next:/next", activeProjectOrigin: "workspace", activeProjectId: "next", activeProjectDir: "/next" }));
  await act(async () => finish({ ...meta, content: "Stale document" }));
  expect(screen.queryByText("Stale document")).toBeNull();
});
it("keeps the latest cross-context link when an earlier lookup completes later", async () => {
  const user = userEvent.setup();
  let finish: (v: any) => void = () => {};
  vi.mocked(wiki.fetchModules).mockResolvedValue([
    {
      id: "home",
      label: "Home",
      context: "project",
      path: "/wiki",
      pages: 1,
      hasLog: false,
    },
    {
      id: "first",
      label: "First context",
      context: "first",
      path: "/first",
      pages: 1,
      hasLog: false,
    },
    {
      id: "second",
      label: "Second context",
      context: "second",
      path: "/second",
      pages: 1,
      hasLog: false,
    },
  ]);
  vi.mocked(wiki.fetchPages).mockImplementation((path) =>
    path === "/first"
      ? new Promise((r) => {
          finish = r;
        })
      : Promise.resolve(
          path === "/second"
            ? [{ ...meta, path: "guide.md", title: "Guide" }]
            : [meta],
        ),
  );
  vi.mocked(wiki.fetchPage).mockImplementation((path) =>
    Promise.resolve({
      ...meta,
      content:
        path === "/wiki"
          ? "[[first/Guide]] and [[second/Guide]]"
          : path === "/second"
            ? "Latest context content"
            : "Outdated context content",
    }),
  );
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Engineering rules" }),
  );
  await user.click(await screen.findByRole("button", { name: "first/Guide" }));
  await user.click(screen.getByRole("button", { name: "second/Guide" }));
  expect(await screen.findByText("Latest context content")).toBeTruthy();
  await act(async () =>
    finish([{ ...meta, path: "guide.md", title: "Guide" }]),
  );
  expect(screen.queryByText("Outdated context content")).toBeNull();
  expect(screen.getByText("Latest context content")).toBeTruthy();
});
it("keeps an AI answer distinct from its authoritative source document", async () => {
  const user = userEvent.setup();
  vi.mocked(wiki.aiSearchWiki).mockResolvedValue({
    answer: "A generated interpretation.",
    results: [
      {
        path: "rules.md",
        title: "Engineering rules",
        relevance: "Source evidence",
        score: 95,
      },
    ],
  });
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await screen.findByRole("button", { name: "Engineering rules" });
  await user.click(screen.getByRole("radio", { name: "Answer with sources" }));
  await user.type(
    screen.getByLabelText("Search knowledge"),
    "What needs verification?",
  );
  await user.click(screen.getByRole("button", { name: "Ask" }));
  expect(await screen.findByText("A generated interpretation.")).toBeTruthy();
  expect(screen.getByText("AI-assisted answer")).toBeTruthy();
  expect(wiki.aiSearchWiki).toHaveBeenCalledWith(
    "/wiki",
    "What needs verification?",
    "/project",
    expect.objectContaining({ signal: expect.any(AbortSignal), onProgress: expect.any(Function) }),
  );
  await user.click(
    screen.getByRole("button", { name: /Engineering rules Source evidence/ }),
  );
  expect(await screen.findByText("Keep evidence attached.")).toBeTruthy();
});

it("does not reopen an old document when refresh completes after a project switch", async () => {
  const user = userEvent.setup();
  render(
    <MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors />
      <WikiExplorerPage />
    </WorkspaceRefreshProvider></MemoryRouter>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Engineering rules" }),
  );
  await screen.findByText("Keep evidence attached.");
  let finish: (rows: any) => void = () => {};
  vi.mocked(wiki.fetchPages).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  vi.mocked(wiki.fetchModules).mockResolvedValue([
    {
      id: "next",
      label: "Next context",
      context: "project",
      path: "/next-wiki",
      pages: 1,
      hasLog: false,
    },
  ]);
  vi.mocked(wiki.fetchPages).mockResolvedValue([
    { ...meta, title: "Next rules", path: "next.md" },
  ]);
  act(() => useAppStore.setState({ activeProjectKey: "workspace:next:/next", activeProjectOrigin: "workspace", activeProjectId: "next", activeProjectDir: "/next" }));
  await screen.findByRole("button", { name: "Next rules" });
  await act(async () => finish([meta]));
  expect(screen.queryByText("Keep evidence attached.")).toBeNull();
  expect(screen.getByRole("button", { name: "Next rules" })).toBeTruthy();
  expect(wiki.fetchPage).toHaveBeenCalledTimes(1);
});

it("does not restore a removed context from a pending document response", async () => {
  const user = userEvent.setup();
  let finish!: (value: any) => void;
  vi.mocked(wiki.fetchPage).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  render(<MemoryRouter><WorkspaceRefreshProvider><WorkspaceSelectors /><WikiExplorerPage /></WorkspaceRefreshProvider></MemoryRouter>);
  await user.click(await screen.findByRole("button", { name: "Engineering rules" }));
  vi.mocked(wiki.fetchModules).mockResolvedValue([]);
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  await act(async () => finish({ ...meta, content: "Removed context content" }));
  expect(screen.queryByText("Removed context content")).toBeNull();
  expect(screen.queryByRole("button", { name: "Engineering rules" })).toBeNull();
});

it('uses the header project scope without a redundant knowledge picker', async () => {
  render(<MemoryRouter><WikiExplorerPage autoSelectProject /></MemoryRouter>);
  await screen.findByRole('button', { name: 'Engineering rules' });
  expect(screen.queryByRole('combobox', { name: 'Knowledge context' })).toBeNull();
});

it('opens the exact Knowledge page addressed by a persisted reference URL', async () => {
  render(<MemoryRouter initialEntries={['/knowledge/explorer?page=rules.md']}><WikiExplorerPage autoSelectProject /></MemoryRouter>);
  expect(await screen.findByText('Keep evidence attached.')).toBeTruthy();
  expect(wiki.fetchPage).toHaveBeenCalledWith('/wiki', 'rules.md');
});

it('keeps the context picker on an explicitly imported Knowledge route', async () => {
 vi.mocked(wiki.fetchModules).mockResolvedValue([
  {id:'own',label:'Project knowledge',context:'project',path:'/wiki',pages:1,hasLog:false},
  {id:'shared',label:'Shared knowledge',context:'shared',path:'/shared',pages:1,hasLog:false}
 ]);
 render(<MemoryRouter initialEntries={['/knowledge/explorer/shared']}><Routes><Route path="/knowledge/explorer/:moduleId" element={<WikiExplorerPage autoSelectProject />} /></Routes></MemoryRouter>);
 const picker=await screen.findByRole('combobox',{name:'Knowledge context'});
 expect(picker.textContent).toContain('Shared knowledge');
});

it("renders keyword preview Markdown with usable links outside the record button", async () => {
  vi.mocked(wiki.searchWiki).mockResolvedValue([{ path: "rules.md", title: "Engineering rules", snippet: "**Preview evidence** with `check`\n\n- [External evidence](https://example.com/evidence)", score: 2 }]);
  const user = userEvent.setup();
  render(<MemoryRouter><WikiExplorerPage /></MemoryRouter>);
  await screen.findByRole("button", { name: "Engineering rules" });
  await user.type(screen.getByLabelText("Search knowledge"), "evidence");
  await user.click(screen.getByRole("button", { name: "Find" }));
  const link = await screen.findByRole("link", { name: "External evidence" });
  expect(link.closest("button")).toBeNull();
  expect(screen.getByText("Preview evidence").tagName).toBe("STRONG");
  expect(screen.getByText("check").tagName).toBe("CODE");
});

it('shows streamed diagnostics before the answer and aborts when project changes', async () => {
  const user = userEvent.setup();
  let finish!: (value: any) => void;
  vi.mocked(wiki.aiSearchWiki).mockImplementation((_path, _query, _project, options) => {
    options?.onProgress?.({ kind: 'stderr', text: 'Inspecting selected documents' });
    return new Promise(resolve => { finish = resolve; });
  });
  render(<MemoryRouter><WorkspaceRefreshProvider><WikiExplorerPage /></WorkspaceRefreshProvider></MemoryRouter>);
  await screen.findByRole('button', {name:'Engineering rules'});
  await user.click(screen.getByRole('radio', {name:'Answer with sources'}));
  await user.type(screen.getByLabelText('Search knowledge'),'rules');
  await user.click(screen.getByRole('button', {name:'Ask'}));
  expect(await screen.findByText('Inspecting selected documents')).toBeTruthy();
  const signal = vi.mocked(wiki.aiSearchWiki).mock.calls[0][3]?.signal;
  act(()=>useAppStore.setState({activeProjectKey:'workspace:other:/other-project',activeProjectOrigin:'workspace',activeProjectId:'other',activeProjectDir:'/other-project'}));
  expect(signal?.aborted).toBe(true);
  await act(async()=>finish({answer:'Old answer',results:[]}));
  expect(screen.queryByText('Old answer')).toBeNull();
});
