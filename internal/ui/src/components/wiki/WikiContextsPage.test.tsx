import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { useAppStore } from "@/store/appStore";
import { fetchModules } from "@/api/wiki";
import WikiContextsPage from "./WikiContextsPage";

vi.mock("@/api/wiki", () => ({ fetchModules: vi.fn() }));
vi.mock("@/api/hub", () => ({ hubApi: { getProjectContexts: vi.fn() } }));

const module = {
  id: "knowledge", label: "Project knowledge", context: "project",
  path: "/wiki", pages: 2, hasLog: false,
};

beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({
    activeProjectKey: "", activeProjectOrigin: "", activeProjectDir: "", activeProjectId: "",
  });
});
afterEach(cleanup);

function renderPage() {
  return render(<MemoryRouter><WikiContextsPage moduleFilter="knowledge" /></MemoryRouter>);
}

it("prompts for a project without requesting an unscoped collection", async () => {
  renderPage();
  expect(await screen.findByText("Select a project")).toBeTruthy();
  expect(screen.getByText(/Use the Project menu in the header/)).toBeTruthy();
  expect(fetchModules).not.toHaveBeenCalled();
});

it("renders an empty collection when the API returns null", async () => {
  useAppStore.setState({
    activeProjectKey: "workspace:demo:/project", activeProjectOrigin: "workspace",
    activeProjectDir: "/project", activeProjectId: "demo",
  });
  vi.mocked(fetchModules).mockResolvedValue(null as never);
  renderPage();
  expect(await screen.findByText("No contexts available")).toBeTruthy();
  expect(fetchModules).toHaveBeenCalledWith("/project");
});

it("keeps collection inspection working and clears it when the project changes", async () => {
  const user = userEvent.setup();
  useAppStore.setState({
    activeProjectKey: "workspace:demo:/project", activeProjectOrigin: "workspace",
    activeProjectDir: "/project", activeProjectId: "demo",
  });
  vi.mocked(fetchModules).mockResolvedValue([module]);
  renderPage();
  await user.click(await screen.findByRole("button", { name: "Project knowledge" }));
  expect(screen.getByRole("button", { name: "Read this collection" })).toBeTruthy();

  act(() => useAppStore.setState({
    activeProjectKey: "", activeProjectOrigin: "", activeProjectDir: "", activeProjectId: "",
  }));
  await waitFor(() => expect(screen.getByText("Select a project")).toBeTruthy());
  expect(screen.queryByText("Project knowledge")).toBeNull();
  expect(screen.queryByRole("button", { name: "Read this collection" })).toBeNull();
  expect(fetchModules).toHaveBeenCalledTimes(1);
});
