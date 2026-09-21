import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { astApi } from "@/api/ast";
import { QueryBar } from "./QueryBar";
vi.mock("@/api/ast", () => ({
  astApi: { generateCypher: vi.fn(), getGraph: vi.fn() },
}));
vi.mock("@/lib/utils", async (original) => ({ ...await original<typeof import('@/lib/utils')>(), agentFeaturesEnabled: () => true }));
afterEach(cleanup);
beforeEach(() => vi.resetAllMocks());
it("lets the user review and edit an AI draft before explicit scoped execution", async () => {
  const user = userEvent.setup(),
    onResult = vi.fn();
  vi.mocked(astApi.generateCypher).mockResolvedValue({
    cypher: "MATCH (n:Function) RETURN n LIMIT 30",
  });
  vi.mocked(astApi.getGraph).mockResolvedValue({
    nodes: [],
    links: [],
    files: [],
    fileContents: {},
  });
  render(
    <QueryBar
      contextId="library"
      projectDir="/project"
      onQueryResult={onResult}
      loading={false}
      setLoading={vi.fn()}
    />,
  );
  await user.click(screen.getByRole("tab", { name: "Draft with AI" }));
  await user.type(
    screen.getByLabelText("What do you want to understand?"),
    "Find functions",
  );
  await user.click(screen.getByRole("button", { name: "Generate draft" }));
  expect(await screen.findByText("Draft ready for review")).toBeTruthy();
  expect(astApi.getGraph).not.toHaveBeenCalled();
  await user.clear(screen.getByLabelText("Cypher query"));
  await user.type(
    screen.getByLabelText("Cypher query"),
    "MATCH (n:File) RETURN n LIMIT 10",
  );
  await user.click(screen.getByRole("button", { name: "Run query" }));
  await waitFor(() => expect(onResult).toHaveBeenCalled());
  expect(astApi.getGraph).toHaveBeenCalledWith({
    context: "library",
    project_dir: "/project",
    cypher_query: "MATCH (n:File) RETURN n LIMIT 10",
  });
});

it('shows draft progress and can cancel without executing the query', async () => {
  const user=userEvent.setup();
  vi.mocked(astApi.generateCypher).mockImplementation((_q,_c,_p,options)=>{
    options?.onProgress?.({kind:'thinking',text:'Inspecting schema'});
    return new Promise(()=>{});
  });
  render(<QueryBar projectDir="/project" onQueryResult={vi.fn()} loading={false} setLoading={vi.fn()}/>);
  await user.click(screen.getByRole('tab',{name:'Draft with AI'}));
  await user.type(screen.getByLabelText('What do you want to understand?'),'Find callers');
  await user.click(screen.getByRole('button',{name:'Generate draft'}));
  expect(await screen.findByText('Inspecting schema')).toBeTruthy();
  await user.click(screen.getByRole('button',{name:'Stop generation'}));
  expect(vi.mocked(astApi.generateCypher).mock.calls[0][3]?.signal?.aborted).toBe(true);
  expect(astApi.getGraph).not.toHaveBeenCalled();
});
