import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import { useEffect, useState, useCallback, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { fetchModules, type WikiModule } from "@/api/wiki";
import { useAppStore } from "@/store/appStore";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import {
  WorkPage,
  WorkHeader,
  WorkSearch,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
export default function WikiContextsPage({
  moduleFilter,
}: {
  moduleFilter: string;
}) {
  const navigate = useNavigate(),
    { activeProjectDir } = useAppStore();
  const [modules, setModules] = useState<WikiModule[]>([]),
    [query, setQuery] = useState(""),
    [selected, setSelected] = useState<WikiModule | null>(null),
    [loading, setLoading] = useState(false),
    [error, setError] = useState("");
  const request = useRef(0);
  const load = useCallback(async () => {
    const id = ++request.current;
    setLoading(true);
    setError("");
    try {
      const ms = await fetchModules(activeProjectDir || undefined);
      if (id === request.current) {
        setSelected(current => ms.find(m => m.id === current?.id) || null);
        setModules(
          (ms || []).filter(
            (m) => m.id === moduleFilter || m.id.startsWith(moduleFilter + "/"),
          ),
        );
      }
    } catch (e) {
      if (id === request.current) setError((e as Error).message);
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, [activeProjectDir, moduleFilter]);
  useEffect(() => {
    setSelected(null);
    setModules([]);
    void load();
    return () => {
      request.current++;
    };
  }, [load]);
  const visible = modules.filter((m) =>
    (m.label + " " + m.context).toLowerCase().includes(query.toLowerCase()),
  );
  usePageRefresh(load);
  return (
    <WorkPage>
      <WorkHeader
        title="Knowledge contexts"
        description="Inspect where knowledge comes from, then choose the collection relevant to your work."
      />
      <div className="work-toolbar">
        <WorkSearch
          label="Find a knowledge context"
          value={query}
          onChange={setQuery}
        />
        <small>{modules.length} collections</small>
      </div>
      {error && (
        <WorkNotice title="Could not load contexts" tone="error">
          {error}
        </WorkNotice>
      )}
      {loading ? (
        <LoadingSpinner label="Loading contexts…" />
      ) : !modules.length ? (
        <WorkEmpty title="No contexts available">
          Index project documentation with graphit knowledge index docs/.
        </WorkEmpty>
      ) : (
        <div className="work-split">
          <div className="work-table-wrap">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Collection</th>
                  <th>Pages</th>
                  <th>Origin</th>
                </tr>
              </thead>
              <tbody>
                {visible.map((m) => (
                  <tr
                    key={m.id}
                    className={selected?.id === m.id ? "selected" : ""}
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => setSelected(m)}
                      >
                        {m.label}
                      </button>
                      <small>{m.id}</small>
                    </td>
                    <td>{m.pages}</td>
                    <td>{m.context}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <aside className="work-panel">
            {selected ? (
              <>
                <h2 className="text-xl font-semibold mb-5">{selected.label}</h2>
                <FactList
                  items={[
                    ["Context", selected.context],
                    ["Location", selected.path],
                    ["Pages", selected.pages],
                    [
                      "Changelog",
                      selected.hasLog ? "Available" : "Not present",
                    ],
                  ]}
                />
                <button
                  className="work-button primary mt-5"
                  onClick={() =>
                    navigate(
                      selected.context === "project"
                        ? "/knowledge/explorer"
                        : "/knowledge/explorer/" +
                            encodeURIComponent(selected.context),
                    )
                  }
                >
                  Read this collection
                </button>
              </>
            ) : (
              <WorkEmpty title="Choose a collection">
                Review its origin before opening the library.
              </WorkEmpty>
            )}
          </aside>
        </div>
      )}
    </WorkPage>
  );
}
