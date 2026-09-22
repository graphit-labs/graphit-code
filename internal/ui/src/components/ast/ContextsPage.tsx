import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import { useEffect, useLayoutEffect, useState, useCallback, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { astApi, type Context } from "@/api/ast";
import { hubApi } from "@/api/hub";
import { showToast } from "@/hooks/useToast";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { ConfirmModal } from "@/components/hub/modals/ConfirmModal";
import { useAppStore } from "@/store/appStore";
import { projectRequestScope } from "@/lib/projectScope";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
export default function ContextsPage() {
  const navigate = useNavigate();
  const { setActiveContextId, activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId } = useAppStore();
  const { key: projectKey, projectDir, projectId, remote } = projectRequestScope({
    activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId,
  });
  const [contexts, setContexts] = useState<Context[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const request = useRef(0);
  const scope = useRef(projectKey);
  useLayoutEffect(() => { scope.current = projectKey; }, [projectKey]);
  const [deleteModal, setDeleteModal] = useState<{
    open: boolean;
    id: string;
    name: string;
  }>({ open: false, id: "", name: "" });

  const load = useCallback(async () => {
    const id = ++request.current,
      project = projectKey;
    setLoading(true);
    setError("");
    try {
      const data = remote && projectId
        ? {
            contexts: (await hubApi.getProjectContexts(projectId)).entries
              .filter(entry => entry.type === "ast" && entry.qualified_latest)
              .map(entry => ({
                id: entry.qualified_latest!, name: entry.name || entry.id, type: "import" as const,
                database: "Hub published artifact", db_path: entry.qualified_latest,
              })),
            project_name: "Hub project",
          }
        : await astApi.getContexts(projectDir);
      if (id !== request.current || scope.current !== project) return;
      setContexts(data.contexts ?? []);
      document.title = `Graphit AST — ${data.project_name ?? "Explorer"}`;
    } catch {
      if (id === request.current && scope.current === project)
        setError("Could not load the indexed contexts. Refresh to try again.");
    } finally {
      if (id === request.current && scope.current === project)
        setLoading(false);
    }
  }, [projectKey, projectDir, projectId, remote]);

  const [dataScope, setDataScope] = useState(projectKey);
  if (dataScope !== projectKey) {
    setDataScope(projectKey);
    setContexts([]);
    setSelected("");
    setDeleteModal({ open: false, id: "", name: "" });
  }
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) void load(); });
    return () => {
      active = false;
      request.current++;
    };
  }, [load]);

  const handleExplore = (id: string) => {
    setActiveContextId(id);
    navigate(`/ast/explorer/${encodeURIComponent(id)}`);
  };

  const handleDelete = (id: string) => {
    const ctx = contexts.find((c) => c.id === id);
    setDeleteModal({ open: true, id, name: ctx?.name ?? id });
  };

  const confirmDelete = async () => {
    const { id } = deleteModal;
    setDeleteModal({ open: false, id: "", name: "" });
    try {
      await astApi.deleteContext(id, projectDir);
      showToast("Context unlinked from this project", "success");
      await load();
    } catch {
      showToast("Failed to remove context", "error");
    }
  };

  const visible = contexts.filter((c) =>
    (c.name + " " + c.id + " " + (c.path || ""))
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const current = contexts.find((c) => c.id === selected);
  usePageRefresh(load);
  return (
    <WorkPage>
      <WorkHeader
        title="Indexed code contexts"
        description="Choose the source boundary for an investigation. Project and imported indexes remain distinct."
      />
      <div className="work-toolbar">
        <WorkSearch
          label="Find an AST context"
          value={query}
          onChange={setQuery}
        />
        <span className="text-xs text-muted-foreground">
          {contexts.length} available contexts
        </span>
      </div>
      {error && (
        <WorkNotice tone="error" title="Contexts unavailable">
          {error}
        </WorkNotice>
      )}
      {loading ? (
        <LoadingSpinner label="Loading contexts…" />
      ) : !contexts.length ? (
        <WorkEmpty title="No contexts available">
          {remote ? "This Hub project has no published AST artifact." : "Index a project with graphit ast index, or install an AST context from the Hub."}
        </WorkEmpty>
      ) : (
        <div className="work-split">
          <section>
            <div className="work-table-wrap">
              <table className="work-table">
                <thead>
                  <tr>
                    <th>Context</th>
                    <th>Nodes</th>
                    <th>Relationships</th>
                  </tr>
                </thead>
                <tbody>
                  {visible.map((ctx) => (
                    <tr
                      key={ctx.id}
                      className={selected === ctx.id ? "selected" : ""}
                    >
                      <td>
                        <button
                          className="record-title"
                          onClick={() => setSelected(ctx.id)}
                        >
                          {ctx.name}
                        </button>
                        <small>
                          {remote ? "Published Hub index" : ctx.type === "project"
                            ? "Project index"
                            : "Imported index"}
                        </small>
                      </td>
                      <td>{ctx.node_count?.toLocaleString() ?? "—"}</td>
                      <td>{ctx.edge_count?.toLocaleString() ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {!visible.length && (
              <WorkEmpty title="No matching contexts">
                Try another name or path.
              </WorkEmpty>
            )}
          </section>
          <aside className="work-panel">
            {current ? (
              <>
                <WorkSection
                  title={current.name}
                  description={
                    remote
                      ? "A published, immutable code index resolved on demand from Hub."
                      : current.type === "project"
                      ? "The indexed implementation of this project."
                      : "A separately scoped imported code index."
                  }
                >
                  <FactList
                    items={[
                      [remote ? "Exact context" : "Context ID", current.id],
                      ["Source", current.path],
                      ["Store", current.db_path],
                      ["Database", current.database],
                      ["Imported", current.imported_at],
                    ]}
                  />
                </WorkSection>
                <div className="work-actions">
                  <button
                    className="work-button primary"
                    onClick={() => handleExplore(current.id)}
                  >
                    Investigate this context
                  </button>
                  {!remote && current.type !== "project" && (
                    <button
                      className="work-button danger"
                      onClick={() => handleDelete(current.id)}
                    >
                      Unlink context
                    </button>
                  )}
                </div>
              </>
            ) : (
              <WorkEmpty title="Choose a source boundary">
                Select a context to inspect its origin and open an
                investigation.
              </WorkEmpty>
            )}
          </aside>
        </div>
      )}
      <ConfirmModal
        open={deleteModal.open}
        title="Unlink context"
        message={`Unlink "${deleteModal.name}" from this project?`}
        warning="The shared index and source files remain available. This project stops using the imported context."
        confirmLabel="Unlink"
        onConfirm={confirmDelete}
        onCancel={() => setDeleteModal({ open: false, id: "", name: "" })}
      />
    </WorkPage>
  );
}
