import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import {
  WorkPage,
  WorkHeader,
  WorkSearch,
  WorkEmpty,
} from "@/components/shared/EngineeringUI";
import { useEffect, useState, useCallback, useRef } from "react";
import { Search, RefreshCw, LogOut, Compass } from "lucide-react";
import { hubApi, type RegistryEntry, type InstalledArtifact } from "@/api/hub";
import { useAppStore } from "@/store/appStore";
import { showToast } from "@/hooks/useToast";
import { ArtifactCard } from "./ArtifactCard";
import { AliasModal } from "./modals/AliasModal";
import { ConfirmModal } from "./modals/ConfirmModal";
import { SubmitModal } from "./modals/SubmitModal";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { PackageOpen } from "lucide-react";

export default function RegistryPage() {
  const { activeProjectDir, activeAgent } = useAppStore();
  return (
    <RegistryWorkspace key={JSON.stringify([activeProjectDir, activeAgent])} />
  );
}
function RegistryWorkspace() {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const {
    typeFilter,
    projectFilter,
    search,
    setSearch,
    activeAgent,
    setActiveAgent,
    setActiveProjectId,
    activeProjectId,
    webMode,
    activeProjectDir,
    projectName,
  } = useAppStore();

  const [selection, setSelection] = useState("");
  const { setTypeFilter, setProjectFilter } = useAppStore();
  const request = useRef(0);
  const [entries, setEntries] = useState<RegistryEntry[]>([]);
  const [installed, setInstalled] = useState<InstalledArtifact[]>([]);
  const [, setProjects] = useState<Array<{ name: string; remote_id: string }>>(
    [],
  );
  const [gitAuthor, setGitAuthor] = useState("");
  const [loading, setLoading] = useState(true);
  const [clusterLabels, setClusterLabels] = useState<Record<string, string>>(
    {},
  );

  const [aliasModal, setAliasModal] = useState<{
    open: boolean;
    id: string;
    type: string;
    require: boolean;
    version?: string;
  }>({
    open: false,
    id: "",
    type: "",
    require: false,
  });
  const [confirmModal, setConfirmModal] = useState<{
    open: boolean;
    entry: RegistryEntry | null;
    installed: InstalledArtifact | null;
  }>({
    open: false,
    entry: null,
    installed: null,
  });
  const [submitModal, setSubmitModal] = useState<{
    open: boolean;
    artifact: InstalledArtifact | null;
  }>({
    open: false,
    artifact: null,
  });

  const loadData = useCallback(async () => {
    if (!mounted.current) return;
    const id = ++request.current;
    setEntries([]);
    setLoading(true);
    try {
      const data = await hubApi.getRegistry(activeProjectDir, activeAgent);
      if (!mounted.current || id !== request.current) return;
      setEntries(data.entries ?? []);
      setInstalled(data.installed ?? []);
      if (data.projects) setProjects(data.projects);
      if (data.agent) setActiveAgent(data.agent);
      if (data.active_project_id) setActiveProjectId(data.active_project_id);
      setClusterLabels(data.project_cluster ?? {});
      const name = webMode
        ? `@${window.__WEB_USER__}`
        : data.active_project_name ||
          data.active_project ||
          projectName ||
          "Global";
      document.title = `Graphit Hub — ${name}`;
    } catch {
      showToast("Failed to connect to Hub API", "error");
    } finally {
      if (mounted.current && id === request.current) setLoading(false);
    }
  }, [
    activeProjectDir,
    activeAgent,
    webMode,
    setActiveAgent,
    setActiveProjectId,
    projectName,
  ]);

  useEffect(() => {
    queueMicrotask(loadData);
  }, [loadData]);

  useEffect(() => {
    if (!webMode) {
      hubApi
        .getGitAuthor()
        .then((d) => setGitAuthor(d.author))
        .catch(() => {});
    }
  }, [webMode]);

  const filteredEntries = entries.filter((e) => {
    const matchType = typeFilter === "all" || e.type === typeFilter;
    const matchProject =
      projectFilter === "all" || e.project_id === projectFilter;
    const q = search.toLowerCase();
    const matchSearch =
      !q ||
      e.name.toLowerCase().includes(q) ||
      (e.description || "").toLowerCase().includes(q);
    return matchType && matchProject && matchSearch;
  });

  const groups = filteredEntries.reduce<Record<string, RegistryEntry[]>>(
    (acc, e) => {
      const grp = e.type || "other";
      if (!acc[grp]) acc[grp] = [];
      acc[grp].push(e);
      return acc;
    },
    {},
  );

  const handleInstall = async (
    id: string,
    type: string,
    withAlias = false,
    version?: string,
  ) => {
    if (webMode) return;
    const nameCollision = installed.some(
      (d) =>
        (d.alias || d.local_id) === id.split("/").pop() && d.remote_id !== id,
    );
    if (withAlias || nameCollision) {
      setAliasModal({ open: true, id, type, require: nameCollision, version });
      return;
    }
    await doInstall(id, type, null, version);
  };

  const doInstall = async (
    id: string,
    type: string,
    alias: string | null,
    version?: string,
  ) => {
    setAliasModal((m) => ({ ...m, open: false }));
    try {
      const res = await hubApi.install(
        id,
        alias,
        activeAgent,
        type,
        activeProjectDir,
        version,
      );
      if (!mounted.current) return;
      if (res.success) {
        showToast(`Installed ${id}`, "success");
        await loadData();
      } else {
        showToast(`Install failed: ${res.error ?? "unknown error"}`, "error");
      }
    } catch {
      showToast("Connection error!", "error");
    }
  };

  const handleUninstall = (entry: RegistryEntry, inst: InstalledArtifact) => {
    setConfirmModal({ open: true, entry, installed: inst });
  };

  const doUninstall = async () => {
    const { entry, installed: inst } = confirmModal;
    if (!entry || !inst) return;
    setConfirmModal((m) => ({ ...m, open: false }));
    try {
      const res = await hubApi.uninstall(
        entry.id,
        inst.local_id,
        activeAgent,
        entry.type,
        activeProjectDir,
      );
      if (!mounted.current) return;
      if (res.success) {
        showToast(`Removed ${inst.local_id || entry.id}`, "success");
        await loadData();
      } else {
        showToast("Uninstall failed", "error");
      }
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
        showToast(`Published ${payload.name} v${payload.version}!`, "success");
        setSubmitModal({ open: false, artifact: null });
        await loadData();
      } else {
        showToast(`Submit failed: ${res.error ?? "unknown"}`, "error");
      }
    } catch {
      showToast("Connection error!", "error");
    }
  };

  const selected = filteredEntries.find(
    (e) => e.type + "/" + e.id === selection,
  );
  usePageRefresh(loadData);
  return (
    <WorkPage>
      <WorkHeader
        title="Artifact registry"
        description="Inspect reusable engineering context before bringing it into a project."
        actions={
          <>

            {webMode && window.__WEB_USER__ && (
              <button
                className="work-button"
                onClick={() => {
                  window.location.href = window.__LOGOUT_URL__ ?? "/logout";
                }}
              >
                Logout
              </button>
            )}
          </>
        }
      />
      <div className="runtime-strip">
        <small>Authorized registry catalogue</small>
      </div>
      <div className="work-toolbar">
        <WorkSearch
          label="Search artifacts"
          value={search}
          onChange={setSearch}
        />
        <StyledSelect
          aria-label="Artifact type"
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value)}
        >
          <option value="all">All artifact types</option>
          {Array.from(new Set(entries.map((e) => e.type)))
            .sort()
            .map((t) => (
              <option key={t}>{t}</option>
            ))}
        </StyledSelect>
        <StyledSelect
          aria-label="Publisher project"
          value={projectFilter}
          onChange={(e) => setProjectFilter(e.target.value)}
        >
          <option value="all">All publisher projects</option>
          {Array.from(
            new Set(entries.map((e) => e.project_id).filter(Boolean)),
          ).map((id) => (
            <option key={id}>{id}</option>
          ))}
        </StyledSelect>
      </div>
      {loading ? (
        <LoadingSpinner label="Loading registry…" />
      ) : (
        <div className="artifact-directory">
          <section className="work-table-wrap">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Artifact</th>
                  <th>Type</th>
                  <th>Latest</th>
                  <th>Local state</th>
                </tr>
              </thead>
              <tbody>
                {filteredEntries.map((e) => (
                  <tr
                    key={e.type + "/" + e.id}
                    className={
                      selection === e.type + "/" + e.id ? "selected" : ""
                    }
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => setSelection(e.type + "/" + e.id)}
                      >
                        {e.name}
                      </button>
                      <small>{e.description}</small>
                    </td>
                    <td>{e.type}</td>
                    <td>{e.latest}</td>
                    <td>
                      {installed.some(
                        (a) => a.remote_id === e.id && a.type === e.type,
                      )
                        ? "Installed"
                        : "Available"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!filteredEntries.length && (
              <WorkEmpty title="No artifacts found">
                Adjust your filters or refresh the registry.
              </WorkEmpty>
            )}
          </section>
          <aside className="work-panel">
            {selected ? (
              <ArtifactCard
                key={selected.type + "/" + selected.id}
                variant="registry"
                entry={selected}
                installedInfo={
                  installed.find(
                    (i) =>
                      i.remote_id === selected.id && i.type === selected.type,
                  ) || null
                }
                webMode={webMode}
                activeProjectId={activeProjectId}
                clusterLabels={clusterLabels}
                onInstall={handleInstall}
                onUninstall={handleUninstall}
                onSubmit={(art) =>
                  setSubmitModal({ open: true, artifact: art })
                }
              />
            ) : (
              <WorkEmpty title="Inspect before installing">
                Select an artifact to review its identity, version and
                publisher.
              </WorkEmpty>
            )}
          </aside>
        </div>
      )}
      <AliasModal
        open={aliasModal.open}
        artifactId={aliasModal.id}
        requireAlias={aliasModal.require}
        onConfirm={(alias) =>
          doInstall(aliasModal.id, aliasModal.type, alias, aliasModal.version)
        }
        onCancel={() => setAliasModal((m) => ({ ...m, open: false }))}
      />
      <ConfirmModal
        open={confirmModal.open}
        title="Remove Artifact"
        message={`Remove "${confirmModal.installed?.local_id || confirmModal.entry?.id}" from your project?`}
        confirmLabel="Remove"
        onConfirm={doUninstall}
        onCancel={() => setConfirmModal((m) => ({ ...m, open: false }))}
      />
      <SubmitModal
        open={submitModal.open}
        artifact={submitModal.artifact}
        activeProjectId={activeProjectId}
        gitAuthor={gitAuthor}
        onSubmit={handleSubmit}
        onClose={() => setSubmitModal({ open: false, artifact: null })}
      />
    </WorkPage>
  );
}
