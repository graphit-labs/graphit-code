import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { dreamApi, type DreamStatus } from "@/api/dream";
import { useAppStore } from "@/store/appStore";

function displayTime(value?: string) {
  return value ? new Date(value).toLocaleString() : "—";
}

export default function DreamDashboard() {
  const { activeProjectDir } = useAppStore();
  const [status, setStatus] = useState<DreamStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const scopeRef = useRef(activeProjectDir);
  useLayoutEffect(() => { scopeRef.current = activeProjectDir; }, [activeProjectDir]);
  const [dataScope, setDataScope] = useState(activeProjectDir);
  if (dataScope !== activeProjectDir) {
    setDataScope(activeProjectDir);
    setStatus(null);
    setError(null);
    setLoading(!!activeProjectDir);
  }

  const fetchData = useCallback(
    async (silent = false) => {
      if (!activeProjectDir) return;
      if (!silent) setLoading(true);
      try {
        const [statusData] = await refreshAll([dreamApi.getStatus(activeProjectDir)] as const);
        if (scopeRef.current !== activeProjectDir) return;
        setStatus(statusData);
        setError(null);
      } catch (err) {
        if (scopeRef.current !== activeProjectDir) return;
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!silent && scopeRef.current === activeProjectDir) setLoading(false);
      }
    },
    [activeProjectDir],
  );

  useEffect(() => {
    const timer = setTimeout(() => void fetchData(), 0);
    const interval = setInterval(() => void fetchData(true), 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, [fetchData]);

  usePageRefresh(() => fetchData(true));
  const run = status?.last_run;

  return (
    <WorkPage>
      <WorkHeader
        title="Dream"
        description="Operational state and latest memory consolidation run."
      />
      {!activeProjectDir ? (
        <WorkEmpty title="Select a project">
          Dream status belongs to a project.
        </WorkEmpty>
      ) : loading && !status ? (
        <LoadingSpinner label="Loading Dream…" />
      ) : error && !status ? (
        <WorkNotice title="Dream status unavailable">{error}</WorkNotice>
      ) : (
        <>
          {error && <WorkNotice title="Unable to refresh Dream status">{error}</WorkNotice>}
          <div className="runtime-strip">
            <strong>{status?.status || "Unknown"}</strong>
            <span>{status?.enabled ? "Enabled in configuration" : "Disabled in configuration"}</span>
            <small>Refreshes every 10 seconds</small>
          </div>
          <div className="operations-layout">
            <section>
              <WorkSection
                title="Latest run"
                description="Execution metadata only. The memory table holds the consolidation result."
              >
                {run ? (
                  <FactList
                    items={[
                      ["Status", run.status],
                      ["Run ID", run.run_id],
                      ["Started", displayTime(run.started_at)],
                      ["Finished", displayTime(run.finished_at)],
                      ["Agent", run.agent],
                      ["CLI", run.cli],
                      ["Tool calls", String(run.tool_calls)],
                      ["Memory mutation attempts", String(run.memory_mutation_attempts)],
                      ["Target memory IDs", run.target_ids?.join(", ")],
                      ["Error", run.error_summary],
                    ]}
                  />
                ) : (
                  <WorkEmpty title="No Dream runs yet">
                    The latest run will appear here after Dream starts.
                  </WorkEmpty>
                )}
              </WorkSection>
            </section>
            <aside>
              <WorkSection title="Execution conditions">
                <FactList
                  items={[
                    ["Daemon", status?.daemon_running ? "Running" : "Not running"],
                    ["Idle timeout", status?.idle_timeout],
                    ["Maximum duration", status?.max_duration],
                    ["Last Dream", displayTime(status?.last_dream_at)],
                    ["Last user edit", displayTime(status?.last_user_edit_at)],
                    ["Current session", status?.session_id],
                  ]}
                />
              </WorkSection>
              <WorkNotice title="Where to review changes">
                Inspect Memory to review the consolidated knowledge. Dream does not create a report artifact.
              </WorkNotice>
            </aside>
          </div>
        </>
      )}
    </WorkPage>
  );
}
