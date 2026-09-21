import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import {
  WorkPage,
  WorkHeader,
  WorkSearch,
  WorkTabs,
  WorkEmpty,
} from "@/components/shared/EngineeringUI";
import { useEffect, useState, useCallback, useRef } from "react";
import { hubApi, type InstalledArtifact } from "@/api/hub";
import { useAppStore } from "@/store/appStore";
import { showToast } from "@/hooks/useToast";
import { ArtifactCard } from "./ArtifactCard";
import { ConfirmModal } from "./modals/ConfirmModal";
import { SubmitModal } from "./modals/SubmitModal";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import {
  FolderOpen,
  CloudUpload,
  RefreshCw,
  Download,
  Package,
} from "lucide-react";
import { cn } from "@/lib/utils";

export default function ProjectArtifactsPage() {
  const { activeProjectDir, activeAgent } = useAppStore();
  return (
    <ArtifactsWorkspace key={JSON.stringify([activeProjectDir, activeAgent])} />
  );
}
function ArtifactsWorkspace() {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const {
    activeAgent,
    webMode,
    activeProjectDir,
    projectName,
    setProjectName,
  } = useAppStore();
  const [view, setView] = useState("own"),
    [query, setQuery] = useState(""),
    [selection, setSelection] = useState("");
  const request = useRef(0);
  const [projectArtifacts, setProjectArtifacts] = useState<InstalledArtifact[]>(
    [],
  );
  const [importedArtifacts, setImportedArtifacts] = useState<
    InstalledArtifact[]
  >([]);
  const [projectPath, setProjectPath] = useState("");
  const [clusterLabels, setClusterLabels] = useState<Record<string, string>>(
    {},
  );
  const [gitAuthor, setGitAuthor] = useState("");
  const [loading, setLoading] = useState(true);
  const [confirmModal, setConfirmModal] = useState<{
    open: boolean;
    art: InstalledArtifact | null;
  }>({ open: false, art: null });
  const [submitModal, setSubmitModal] = useState<{
    open: boolean;
    art: InstalledArtifact | null;
  }>({ open: false, art: null });
  const [unpublishModal, setUnpublishModal] = useState<{
    open: boolean;
    id: string;
    type: string;
  }>({ open: false, id: "", type: "" });
  const [updating, setUpdating] = useState(false);

  const load = useCallback(async () => {
    if (!mounted.current) return;
    const id = ++request.current;
    setLoading(true);
    try {
      const data = await hubApi.getProjectArtifacts(
        activeProjectDir,
        activeAgent,
      );
      if (!mounted.current || id !== request.current) return;
      setProjectArtifacts(data.project_artifacts ?? []);
      setImportedArtifacts(data.imported_artifacts ?? []);
      setProjectName(data.project_name ?? "");
      setProjectPath(data.project_path ?? "");
      setClusterLabels(data.project_cluster ?? {});
    } catch {
      showToast("Failed to load project artifacts", "error");
    } finally {
      if (mounted.current && id === request.current) setLoading(false);
    }
  }, [activeProjectDir, activeAgent, setProjectName]);

  useEffect(() => {
    queueMicrotask(load);
  }, [load]);
  useEffect(() => {
    if (!webMode)
      hubApi
        .getGitAuthor()
        .then((d) => setGitAuthor(d.author))
        .catch(() => {});
  }, [webMode]);

  const handleUnlink = async (art: InstalledArtifact) => {
    try {
      await hubApi.unlinkLocal(
        art.local_id,
        art.type,
        activeAgent,
        projectPath,
      );
      showToast("Unlinked successfully", "success");
      await load();
    } catch {
      showToast("Failed to unlink", "error");
    }
  };

  const handleRemove = async () => {
    const art = confirmModal.art;
    if (!art) return;
    setConfirmModal({ open: false, art: null });
    try {
      const res = await hubApi.uninstall(
        art.local_id,
        art.local_id,
        activeAgent,
        art.type,
        activeProjectDir,
      );
      if (!mounted.current) return;
      if (res.success) {
        showToast(`Removed ${art.local_id}`, "success");
        await load();
      } else showToast("Remove failed", "error");
    } catch {
      showToast("Connection error!", "error");
    }
  };

  const handleSubmit = async (payload: Record<string, unknown>) => {
    try {
      const res = await hubApi.submit({
        ...payload,
        project_dir: activeProjectDir,
      });
      if (!mounted.current) return;
      if (res.success) {
        showToast("Published!", "success");
        setSubmitModal({ open: false, art: null });
        await load();
      } else showToast(`Submit failed: ${res.error}`, "error");
    } catch {
      showToast("Connection error!", "error");
    }
  };

  const handleUnpublish = (id: string, type: string) => {
    setUnpublishModal({ open: true, id, type });
  };

  const doUnpublish = async () => {
    const { id, type } = unpublishModal;
    setUnpublishModal((m) => ({ ...m, open: false }));
    try {
      await hubApi.unpublish(id, type, activeProjectDir);
      showToast("Unpublished", "success");
      await load();
    } catch {
      showToast("Unpublish failed", "error");
    }
  };

  const handleUpdate = async (id: string, type: string) => {
    try {
      const res = await hubApi.updateOne(
        id,
        type,
        activeAgent,
        activeProjectDir,
      );
      if (!mounted.current) return;
      if (res.success) {
        showToast(`Updated ${id}`, "success");
        await load();
      } else {
        showToast(`Update failed: ${res.error ?? "unknown error"}`, "error");
      }
    } catch {
      showToast("Connection error!", "error");
    }
  };

  const handleUpdateAll = async () => {
    setUpdating(true);
    try {
      const res = await hubApi.updateAll(activeAgent, activeProjectDir);
      if (!mounted.current) return;
      if (res.success) {
        showToast("All artifacts updated!", "success");
        await load();
      } else {
        showToast("Update failed", "error");
      }
    } catch {
      showToast("Connection error!", "error");
    } finally {
      setUpdating(false);
    }
  };

  const linkedArts = importedArtifacts.filter((a) => a.origin === "link");
  const hubArts = importedArtifacts.filter(
    (a) => a.origin !== "link" && a.origin !== "publish",
  );
  const totalCount =
    projectArtifacts.length + linkedArts.length + hubArts.length;
  const updatesCount = hubArts.filter((a) => a.has_update).length;
  const typeSummary = new Map<string, number>();
  for (const a of projectArtifacts) {
    typeSummary.set(a.type, (typeSummary.get(a.type) ?? 0) + 1);
  }

  const source =
    view === "own"
      ? projectArtifacts
      : view === "linked"
        ? linkedArts
        : hubArts;
  const visible = source.filter((a) =>
    (a.local_id + " " + (a.registry_name || "") + " " + a.type)
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const selected = visible.find((a) => a.type + "/" + a.local_id === selection);
  usePageRefresh(load);
  return (
    <WorkPage>
      <WorkHeader
        title="Project artifacts"
        description="Review what the agent uses, maintain installed versions and publish the context your project owns."
      />
      <div className="runtime-strip">
        <code>{projectPath}</code>
      </div>
      <WorkTabs
        label="Artifact origin"
        value={view}
        onChange={(id) => {
          setView(id);
          setSelection("");
        }}
        items={[
          ["own", "Owned · " + projectArtifacts.length],
          ["imported", "Installed · " + hubArts.length],
          ["linked", "Linked · " + linkedArts.length],
        ]}
      />
      <div className="work-toolbar">
        <WorkSearch
          label="Find a project artifact"
          value={query}
          onChange={setQuery}
        />
        {view === "imported" && updatesCount > 0 && (
          <button
            className="work-button primary"
            disabled={updating}
            onClick={() => void handleUpdateAll()}
          >
            {updating ? "Updating…" : "Update all (" + updatesCount + ")"}
          </button>
        )}
      </div>
      {loading ? (
        <LoadingSpinner label="Loading project artifacts…" />
      ) : (
        <div className="artifact-directory">
          <section className="work-table-wrap">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Artifact</th>
                  <th>Type</th>
                  <th>Version</th>
                  <th>State</th>
                </tr>
              </thead>
              <tbody>
                {visible.map((a) => (
                  <tr
                    key={a.type + "/" + a.local_id}
                    className={
                      selection === a.type + "/" + a.local_id ? "selected" : ""
                    }
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => setSelection(a.type + "/" + a.local_id)}
                      >
                        {a.registry_name || a.local_id}
                      </button>
                      <small>{a.local_id}</small>
                    </td>
                    <td>{a.type}</td>
                    <td>{a.version || "—"}</td>
                    <td>
                      {a.has_update
                        ? "Update available"
                        : view === "own"
                          ? a.published
                            ? "Published"
                            : "Draft"
                          : a.origin || "Installed"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!visible.length && (
              <WorkEmpty title="No artifacts in this view">
                Create a local artifact or install shared context from the
                registry.
              </WorkEmpty>
            )}
          </section>
          <aside className="work-panel">
            {selected ? (
              <ArtifactCard
                key={selected.type + "/" + selected.local_id}
                variant={view === "own" ? "project" : "imported"}
                installedInfo={selected}
                projectPath={projectPath}
                clusterLabels={clusterLabels}
                onSubmit={(art) => setSubmitModal({ open: true, art })}
                onUnpublish={handleUnpublish}
                onRemove={(art) => setConfirmModal({ open: true, art })}
                onUpdate={handleUpdate}
                onUnlink={handleUnlink}
              />
            ) : (
              <WorkEmpty title="Inspect an artifact">
                See its origin and choose the maintenance action that applies.
              </WorkEmpty>
            )}
          </aside>
        </div>
      )}
      <ConfirmModal
        open={confirmModal.open}
        title="Remove Artifact"
        message={`Remove "${confirmModal.art?.local_id}" from your project?`}
        confirmLabel="Remove"
        onConfirm={handleRemove}
        onCancel={() => setConfirmModal({ open: false, art: null })}
      />
      <ConfirmModal
        open={unpublishModal.open}
        title="Unpublish Artifact"
        message={`Remove "${unpublishModal.id}" from the global registry?`}
        confirmLabel="Unpublish"
        onConfirm={doUnpublish}
        onCancel={() => setUnpublishModal((m) => ({ ...m, open: false }))}
      />
      <SubmitModal
        open={submitModal.open}
        artifact={submitModal.art}
        activeProjectId=""
        gitAuthor={gitAuthor}
        onSubmit={handleSubmit}
        onClose={() => setSubmitModal({ open: false, art: null })}
      />
    </WorkPage>
  );
}
