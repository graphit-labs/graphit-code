import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import { fetchPage } from "@/api/wiki";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useCallback, useEffect, useState, useRef } from "react";
import { dreamApi, type DreamReport, type DreamStatus } from "@/api/dream";
import { useAppStore } from "@/store/appStore";
import {
  AlertCircle,
  Calendar,
  CheckCircle2,
  Clock,
  FileText,
  Inbox,
  Moon,
  RefreshCw,
  Settings,
  Sparkles,
} from "lucide-react";
import { showToast } from "@/hooks/useToast";
import { WikiMarkdown } from "../wiki/WikiMarkdown";

export default function DreamDashboard() {
  const { activeProjectDir } = useAppStore();
  const [status, setStatus] = useState<DreamStatus | null>(null);
  const [reports, setReports] = useState<DreamReport[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedReport, setSelectedReport] = useState<DreamReport | null>(
    null,
  );
  const [reportContent, setReportContent] = useState("");
  const [loadingReport, setLoadingReport] = useState(false);

  const [query, setQuery] = useState("");
  const scopeRef = useRef(activeProjectDir);
  scopeRef.current = activeProjectDir;
  const reportRequest = useRef(0);
  useEffect(() => {
    reportRequest.current++;
    setStatus(null);
    setReports([]);
    setSelectedReport(null);
    setReportContent("");
    setLoadingReport(false);
    if (!activeProjectDir) setLoading(false);
  }, [activeProjectDir]);

  const fetchData = useCallback(
    async (silent = false) => {
      if (!activeProjectDir) return;
      if (!silent) setLoading(true);
      try {
        const [statusData, reportsData] = await refreshAll([
          dreamApi.getStatus(activeProjectDir),
          dreamApi.getReports(activeProjectDir),
        ] as const);
        if (scopeRef.current !== activeProjectDir) return;
        setStatus(statusData);
        setReports(reportsData || []);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        showToast(`Failed to load dream data: ${msg}`, "error");
      } finally {
        if (!silent) setLoading(false);
      }
    },
    [activeProjectDir],
  );

  useEffect(() => {
    const timer = setTimeout(() => fetchData(), 0);
    const interval = setInterval(() => fetchData(true), 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [activeProjectDir, fetchData]);

  const viewReport = async (report: DreamReport) => {
    const request = ++reportRequest.current;
    setSelectedReport(report);
    setLoadingReport(true);
    setReportContent("");
    try {
      const sep = report.path.lastIndexOf("/");
      const dir = report.path.slice(0, sep);
      const path = report.path.slice(sep + 1);
      const data = await fetchPage(dir, path);
      if (request === reportRequest.current)
        setReportContent(data.content || "");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast(`Failed to load report content: ${msg}`, "error");
    } finally {
      if (request === reportRequest.current) setLoadingReport(false);
    }
  };

  const visible = reports.filter((r) =>
    (r.title + " " + r.id).toLowerCase().includes(query.toLowerCase()),
  );
  usePageRefresh(() => refreshAll([fetchData(true), selectedReport ? viewReport(selectedReport) : Promise.resolve()]));
  return (
    <WorkPage>
      <WorkHeader
        title="Dream review"
        description="Review the knowledge and artifacts produced during autonomous maintenance."
      />
      {!activeProjectDir ? (
        <WorkEmpty title="Select a project">
          Dream reports belong to a project.
        </WorkEmpty>
      ) : loading && !status ? (
        <LoadingSpinner label="Loading Dream…" />
      ) : (
        <>
          <div className="runtime-strip">
            <strong>{status?.status || "Unknown"}</strong>
            <span>
              {status
                ? status.enabled
                  ? "Enabled in configuration"
                  : "Disabled in configuration"
                : "Status unavailable"}
            </span>
            <span>{status?.total_reports ?? "—"} reports</span>
            <small>Refreshes every 10 seconds</small>
          </div>
          <div className="operations-layout">
            <section>
              <WorkSection
                title="Maintenance reports"
                description="Open a report to review the recorded work."
              >
                <div className="work-toolbar">
                  <WorkSearch
                    label="Find a Dream report"
                    value={query}
                    onChange={setQuery}
                  />
                </div>
                <div className="work-table-wrap work-catalogue">
                  <table className="work-table">
                    <thead>
                      <tr>
                        <th>Report</th>
                        <th>Created</th>
                        <th>Size</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visible.map((r) => (
                        <tr
                          key={r.id}
                          className={
                            selectedReport?.id === r.id ? "selected" : ""
                          }
                        >
                          <td>
                            <button
                              className="record-title"
                              onClick={() => void viewReport(r)}
                            >
                              {r.title || "Untitled report"}
                            </button>
                            <small>
                              {r.has_deep_sleep ? "Includes deep sleep · " : ""}
                              {r.id}
                            </small>
                          </td>
                          <td>{new Date(r.created).toLocaleString()}</td>
                          <td>{(r.size / 1024).toFixed(1)} KB</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {!visible.length && (
                  <WorkEmpty title="No reports match this view">
                    Reports appear after a Dream session completes.
                  </WorkEmpty>
                )}
              </WorkSection>
              {loadingReport ? (
                <LoadingSpinner label="Loading report…" />
              ) : selectedReport ? (
                <article className="work-panel">
                  <header className="mb-6">
                    <small className="text-muted-foreground">
                      {selectedReport.id}
                    </small>
                    <h2 className="text-xl font-semibold mt-2">
                      {selectedReport.title}
                    </h2>
                  </header>
                  <WikiMarkdown content={reportContent} />
                </article>
              ) : visible.length > 0 ? (
                <WorkEmpty title="Select a session report">
                  Inspect the recorded changes before relying on the maintenance
                  result.
                </WorkEmpty>
              ) : null}
            </section>
            <aside>
              <WorkSection title="Execution conditions">
                <FactList
                  items={[
                    [
                      "Daemon",
                      status?.daemon_running ? "Running" : "Not running",
                    ],
                    ["Idle timeout", status?.idle_timeout],
                    ["Maximum duration", status?.max_duration],
                    ["Last Dream", status?.last_dream_at],
                    ["Last user edit", status?.last_user_edit_at],
                    ["Current session", status?.session_id],
                  ]}
                />
              </WorkSection>
              <WorkNotice title="Review the result">
                A completed maintenance run is an opportunity to inspect the
                produced knowledge and artifacts.
              </WorkNotice>
            </aside>
          </div>
        </>
      )}
    </WorkPage>
  );
}
