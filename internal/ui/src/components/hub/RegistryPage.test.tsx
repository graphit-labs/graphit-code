import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { hubApi } from "@/api/hub";
import { useAppStore } from "@/store/appStore";
import RegistryPage from "./RegistryPage";
vi.mock("@/api/hub", () => ({
  hubApi: { getRegistry: vi.fn(), getGitAuthor: vi.fn(), install: vi.fn() },
}));
vi.mock("./ArtifactCard", () => ({
  ArtifactCard: ({ entry, onInstall }: any) => (
    <button onClick={() => onInstall(entry.id, entry.type, true)}>
      Choose alias
    </button>
  ),
}));
const entry = {
  id: "team/guide",
  name: "Engineering guide",
  type: "knowledge",
  version: "1.0.0",
};
const data = {
  entries: [entry],
  installed: [],
  active_project_name: "Project",
};
beforeEach(() => {
  vi.resetAllMocks();
  useAppStore.setState({
    activeProjectDir: "/a",
    activeAgent: "codex",
    search: "",
    typeFilter: "all",
    projectFilter: "all",
    webMode: false,
  });
  vi.mocked(hubApi.getRegistry).mockResolvedValue(data as any);
  vi.mocked(hubApi.getGitAuthor).mockResolvedValue({ author: "test" });
});
afterEach(cleanup);
it("dismisses scoped install intent when the target project changes", async () => {
  const user = userEvent.setup();
  render(<RegistryPage />);
  await user.click(
    await screen.findByRole("button", { name: /Engineering guide/ }),
  );
  await user.click(screen.getByRole("button", { name: "Choose alias" }));
  expect(screen.getByRole("dialog")).toBeTruthy();
  act(() => useAppStore.setState({ activeProjectDir: "/b" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(hubApi.install).not.toHaveBeenCalled();
  expect(hubApi.getRegistry).toHaveBeenLastCalledWith("/b", "codex");
});
it("ignores old registry metadata after a project switch", async () => {
  let resolve: (value: any) => void = () => {};
  vi.mocked(hubApi.getRegistry).mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  render(<RegistryPage />);
  await waitFor(() =>
    expect(hubApi.getRegistry).toHaveBeenCalledWith("/a", "codex"),
  );
  act(() => useAppStore.setState({ activeProjectDir: "/b" }));
  await screen.findByRole("button", { name: /Engineering guide/ });
  await act(async () =>
    resolve({
      ...data,
      agent: "claude-code",
      active_project_id: "stale",
      entries: [{ ...entry, name: "Stale guide" }],
    }),
  );
  expect(screen.queryByRole("button", { name: /Stale guide/ })).toBeNull();
  expect(useAppStore.getState().activeAgent).toBe("codex");
});
