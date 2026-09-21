import { StyledSelect } from "@/components/shared/StyledSelect";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkTabs,
  WorkEmpty,
  FactList,
} from "@/components/shared/EngineeringUI";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useState } from "react";
import { useAppStore } from "@/store/appStore";
import { hubApi, type GlobalProject } from "@/api/hub";
import {
  Globe,
  Folder,
  Calendar,
  Tag,
  Plus,
  X,
  Trash2,
  ExternalLink,
  Copy,
  Check,
  Search,
  RefreshCw,
  Info,
} from "lucide-react";
import { showToast } from "@/hooks/useToast";

export default function EcosystemDashboard() {
  const {
    projects,
    activeProjectDir,
    loadProjects,
    projectsLoaded,
  } = useAppStore();

  const [selectedDir, setSelectedDir] = useState("");
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);

  const [activeAddLabelProject, setActiveAddLabelProject] = useState<
    string | null
  >(null);
  const [newLabelKey, setNewLabelKey] = useState("");
  const [newLabelValue, setNewLabelValue] = useState("");

  const [scopeFilter, setScopeFilter] = useState<"all" | "siblings">("all");
  const [labelKeyFilter, setLabelKeyFilter] = useState<string>("");

  const activeProject = projects.find((p) => p.dir === activeProjectDir);
  const inspected =
    projects.find((p) => p.dir === selectedDir) || activeProject;
  const activeLabelKeys = activeProject?.cluster
    ? Object.keys(activeProject.cluster)
    : [];

  const effectiveLabelKeyFilter =
    labelKeyFilter && activeLabelKeys.includes(labelKeyFilter)
      ? labelKeyFilter
      : "";

  const handleCopyPath = (path: string, index: number) => {
    navigator.clipboard.writeText(path);
    setCopiedIndex(index);
    showToast("Path copied to clipboard", "success");
    setTimeout(() => setCopiedIndex(null), 2000);
  };

  const handleUnregisterProject = async (project: GlobalProject) => {
    if (
      !window.confirm(
        `Are you sure you want to unregister "${project.name}"? This removes it from the managed ecosystem list but does not delete files.`,
      )
    ) {
      return;
    }

    setLoading(true);
    try {
      const res = await hubApi.unregisterProject(project.id, project.dir);
      if (res.success) {
        showToast(`Successfully unregistered ${project.name}`, "success");
        await loadProjects();
      } else {
        showToast(res.error || "Failed to unregister project", "error");
      }
    } catch {
      showToast("Error unregistering project", "error");
    } finally {
      setLoading(false);
    }
  };

  const handleAddClusterLabel = async (project: GlobalProject) => {
    if (!newLabelKey.trim() || !newLabelValue.trim()) {
      showToast("Key and Value are required", "info");
      return;
    }

    try {
      const res = await hubApi.setClusterLabel(
        project.id,
        project.dir,
        newLabelKey.trim(),
        newLabelValue.trim(),
      );
      if (res.success) {
        showToast(`Added label ${newLabelKey}:${newLabelValue}`, "success");
        setNewLabelKey("");
        setNewLabelValue("");
        setActiveAddLabelProject(null);
        await loadProjects();
      } else {
        showToast(res.error || "Failed to add cluster label", "error");
      }
    } catch {
      showToast("Error adding cluster label", "error");
    }
  };

  const handleRemoveClusterLabel = async (
    project: GlobalProject,
    key: string,
  ) => {
    if (!window.confirm(`Remove cluster label "${key}"?`)) {
      return;
    }

    try {
      const res = await hubApi.unsetClusterLabel(project.id, project.dir, key);
      if (res.success) {
        showToast(`Removed label "${key}"`, "success");
        await loadProjects();
      } else {
        showToast(res.error || "Failed to remove cluster label", "error");
      }
    } catch {
      showToast("Error removing cluster label", "error");
    }
  };

  const formatDate = (dateStr?: string) => {
    if (!dateStr) return "N/A";
    return new Date(dateStr).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  };

  const filteredProjects = projects.filter((p) => {
    const query = searchQuery.toLowerCase();
    const matchSearch =
      !query ||
      p.name.toLowerCase().includes(query) ||
      p.dir.toLowerCase().includes(query) ||
      Object.entries(p.cluster || {}).some(
        ([k, vals]) =>
          k.toLowerCase().includes(query) ||
          vals.some((v) => v.toLowerCase().includes(query)),
      );

    if (!matchSearch) return false;

    if (scopeFilter === "siblings") {
      if (!activeProject) return true;

      const activeHasLabels =
        activeProject.cluster && Object.keys(activeProject.cluster).length > 0;
      const candidateHasLabels = p.cluster && Object.keys(p.cluster).length > 0;

      if (effectiveLabelKeyFilter) {
        const activeVals = activeProject.cluster?.[effectiveLabelKeyFilter];
        const candidateVals = p.cluster?.[effectiveLabelKeyFilter];
        if (!activeVals || !candidateVals) return false;
        const shares = activeVals.some((av) => candidateVals.includes(av));
        if (!shares) return false;
      } else {
        if (!activeHasLabels) {
          if (candidateHasLabels) return false;
        } else {
          if (!candidateHasLabels) return false;
          let sharesAny = false;
          for (const [key, activeVals] of Object.entries(
            activeProject.cluster || {},
          )) {
            const candidateVals = p.cluster?.[key];
            if (
              candidateVals &&
              activeVals.some((av) => candidateVals.includes(av))
            ) {
              sharesAny = true;
              break;
            }
          }
          if (!sharesAny) return false;
        }
      }
    }

    return true;
  });

  return (
    <WorkPage>
      <WorkHeader
        title="Project ecosystem"
        description="Select a working context and maintain the relationships that connect your local projects."
      />
      <WorkTabs
        label="Project scope"
        value={scopeFilter}
        onChange={(id) => {
          setScopeFilter(id as "all" | "siblings");
          setLabelKeyFilter("");
        }}
        items={[
          ["all", "All Projects"],
          ["siblings", "Same Cluster"],
        ]}
      />
      <div className="work-toolbar">
        <WorkSearch
          label="Search projects or labels"
          value={searchQuery}
          onChange={setSearchQuery}
        />
        {scopeFilter === "siblings" && (
          <StyledSelect
            aria-label="Cluster label key"
            value={effectiveLabelKeyFilter}
            onChange={(e) => setLabelKeyFilter(e.target.value)}
          >
            <option value="">Any shared label</option>
            {activeLabelKeys.map((k) => (
              <option key={k}>{k}</option>
            ))}
          </StyledSelect>
        )}
        <small>{filteredProjects.length} projects · local registry</small>
      </div>
      {!projectsLoaded || loading ? (
        <LoadingSpinner label="Loading projects…" />
      ) : (
        <div className="ecosystem-layout">
          <section className="work-table-wrap">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Project</th>
                  <th>Cluster labels</th>
                  <th>Workspace</th>
                </tr>
              </thead>
              <tbody>
                {filteredProjects.map((p) => (
                  <tr
                    key={p.dir}
                    className={inspected?.dir === p.dir ? "selected" : ""}
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => {
                          setSelectedDir(p.dir);
                          setActiveAddLabelProject(null);
                        }}
                      >
                        {p.name}
                      </button>
                      <small>{p.dir}</small>
                    </td>
                    <td>
                      {Object.entries(p.cluster || {}).map(([k, vs]) => (
                        <small key={k}>
                          {k}: {vs.join(", ")}
                        </small>
                      ))}
                    </td>
                    <td>
                      {p.dir === activeProjectDir ? "Active" : "Available"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!filteredProjects.length && (
              <WorkEmpty title="No projects match">
                Try another label or search term.
              </WorkEmpty>
            )}
          </section>
          <aside className="work-panel">
            {inspected ? (
              <>
                <WorkSection
                  title={inspected.name}
                  description={
                    inspected.description ||
                    "A registered local engineering context."
                  }
                >
                  <FactList
                    items={[
                      ["Project ID", inspected.id],
                      ["Directory", inspected.dir],
                      ["Registered", formatDate(inspected.registered_at)],
                    ]}
                  />
                  <div className="work-actions mt-5">
                    <button
                      className="work-button"
                      onClick={() => handleCopyPath(inspected.dir, 0)}
                    >
                      {copiedIndex === 0 ? "Copied" : "Copy path"}
                    </button>
                  </div>
                </WorkSection>
                <WorkSection
                  title="Cluster relationships"
                  description="Shared key/value labels connect related projects."
                >
                  {Object.entries(inspected.cluster || {}).map(
                    ([key, values]) => (
                      <div className="cluster-label-row" key={key}>
                        <span>
                          <strong>{key}</strong>
                          <small>{values.join(", ")}</small>
                        </span>
                        <button
                          className="work-button danger"
                          aria-label={"Remove label " + key}
                          onClick={() =>
                            void handleRemoveClusterLabel(inspected, key)
                          }
                        >
                          Remove
                        </button>
                      </div>
                    ),
                  )}
                  {activeAddLabelProject === inspected.dir ? (
                    <form
                      className="work-form mt-4"
                      onSubmit={(e) => {
                        e.preventDefault();
                        void handleAddClusterLabel(inspected);
                      }}
                    >
                      <label className="work-field">
                        <span>Label key</span>
                        <input
                          value={newLabelKey}
                          onChange={(e) => setNewLabelKey(e.target.value)}
                          autoFocus
                        />
                      </label>
                      <label className="work-field">
                        <span>Label value</span>
                        <input
                          value={newLabelValue}
                          onChange={(e) => setNewLabelValue(e.target.value)}
                        />
                      </label>
                      <div className="work-actions">
                        <button className="work-button primary">
                          Save label
                        </button>
                        <button
                          type="button"
                          className="work-button"
                          onClick={() => setActiveAddLabelProject(null)}
                        >
                          Cancel
                        </button>
                      </div>
                    </form>
                  ) : (
                    <button
                      className="work-button mt-4"
                      onClick={() => setActiveAddLabelProject(inspected.dir)}
                    >
                      Add cluster label
                    </button>
                  )}
                </WorkSection>
                <details className="work-disclosure">
                  <summary>Project registration</summary>
                  <div>
                    <p className="text-xs text-muted-foreground mb-4">
                      Unregister removes the project from this directory. Its
                      source files remain on disk.
                    </p>
                    <button
                      className="work-button danger"
                      onClick={() => void handleUnregisterProject(inspected)}
                    >
                      Unregister project
                    </button>
                  </div>
                </details>
              </>
            ) : (
              <WorkEmpty title="Inspect a project">
                Choose a project to view its identity and cluster relationships.
              </WorkEmpty>
            )}
          </aside>
        </div>
      )}
    </WorkPage>
  );
}
