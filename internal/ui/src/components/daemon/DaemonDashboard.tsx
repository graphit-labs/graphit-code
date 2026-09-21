import { usePageRefresh } from "@/components/layout/WorkspaceRefresh";
import { ConfirmModal } from "@/components/hub/modals/ConfirmModal";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useEffect, useState } from "react";
import { daemonApi, type DaemonStatus } from "@/api/daemon";
import {
  Terminal,
  Square,
  Activity,
  FileText,
  RefreshCw,
  Cpu,
  Calendar,
  Server,
  Copy,
  Check,
  Plug,
} from "lucide-react";
import { showToast } from "@/hooks/useToast";

export default function DaemonDashboard() {
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [stopConfirm, setStopConfirm] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [copiedKey, setCopiedKey] = useState(false);

  const copyMcpKey = async () => {
    if (!status?.mcp_key) return;
    try {
      await navigator.clipboard.writeText(status.mcp_key);
      setCopiedKey(true);
      showToast("MCP auth key copied to clipboard", "success");
      setTimeout(() => setCopiedKey(false), 2000);
    } catch {
      showToast("Could not copy the MCP auth key", "error");
    }
  };

  const fetchStatus = async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      const data = await daemonApi.getStatus();
      setStatus(data);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast(`Failed to load daemon status: ${msg}`, "error");
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => {
    const timer = setTimeout(() => {
      fetchStatus();
    }, 0);
    const interval = setInterval(() => fetchStatus(true), 5000);
    return () => {
      clearTimeout(timer);
      clearInterval(interval);
    };
  }, []);

  const handleStop = async () => {
    setStopConfirm(false);
    setStopping(true);
    try {
      const res = await daemonApi.stop();
      showToast(res.message, "success");
      await fetchStatus();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast(`Failed to stop daemon: ${msg}`, "error");
    } finally {
      setStopping(false);
    }
  };

  const formatUptime = (seconds?: number) => {
    if (seconds == null) return "N/A";
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  };

  const formatTime = (timeStr?: string) => {
    if (!timeStr) return "N/A";
    return new Date(timeStr).toLocaleString();
  };

  usePageRefresh(() => fetchStatus(true));
  return (
    <WorkPage>
      <WorkHeader
        title="Daemon operations"
        description="Inspect the local service, connect an agent and diagnose background synchronization."
      />
      {loading && !status ? (
        <LoadingSpinner label="Loading daemon status…" />
      ) : status ? (
        <>
          <div className="runtime-strip">
            <strong>{status.running ? "Running" : "Stopped"}</strong>
            <span>PID {status.running ? status.pid : "—"}</span>
            <span>
              Uptime{" "}
              {status.running ? formatUptime(status.uptime_seconds) : "—"}
            </span>
            <span>Scheduler: {status.scheduler_status || "Unknown"}</span>
            <small>Global service · refreshes every 5 seconds</small>
          </div>
          <div className="operations-layout">
            <section>
              <WorkSection
                title="Recent service logs"
                description="Inspect the latest records returned by the daemon."
              >
                <div className="service-logs">
                  {status.recent_logs?.length ? (
                    <pre>{status.recent_logs.join("\n")}</pre>
                  ) : (
                    <WorkEmpty title="No log records available">
                      Refresh after the daemon has recorded an event.
                    </WorkEmpty>
                  )}
                </div>
              </WorkSection>
              <WorkSection title="Process details">
                <FactList
                  items={[
                    ["Started", formatTime(status.started_at)],
                    ["PID file", status.pid_file_path],
                    ["MCP key file", status.mcp_key_file],
                  ]}
                />
              </WorkSection>
            </section>
            <aside>
              <WorkSection
                title="Connect an agent"
                description="The agent brings its model. Graphit supplies the engineering context."
              >
                {status.mcp_endpoint ? (
                  <>
                    <FactList
                      items={[
                        ["Endpoint", status.mcp_endpoint],
                        ["Port", status.mcp_port],
                      ]}
                    />
                    <pre className="work-code mt-4">
                      {JSON.stringify(
                        {
                          mcpServers: {
                            graphit: {
                              url: status.mcp_endpoint,
                              headers: { Authorization: "Bearer <key>" },
                            },
                          },
                        },
                        null,
                        2,
                      )}
                    </pre>
                    {status.mcp_key && (
                      <button
                        className="work-button mt-4"
                        onClick={() => void copyMcpKey()}
                      >
                        {copiedKey ? "Copied" : "Copy MCP bearer key"}
                      </button>
                    )}
                    <p className="text-xs text-muted-foreground mt-4 leading-relaxed">
                      The runtime key changes after a restart. An OIDC broker
                      provider uses each caller's access token to preserve
                      request identity.
                    </p>
                  </>
                ) : (
                  <WorkNotice title="No MCP endpoint reported">
                    Start the daemon to expose its connection details.
                  </WorkNotice>
                )}
              </WorkSection>
              <WorkSection
                title="Service control"
                description="Stopping the daemon interrupts background synchronization."
              >
                {status.running ? (
                  <button
                    className="work-button danger"
                    disabled={stopping}
                    onClick={() => setStopConfirm(true)}
                  >
                    {stopping ? "Stopping…" : "Stop Daemon"}
                  </button>
                ) : (
                  <WorkNotice title="Start from your terminal">
                    <code>graphit daemon</code>
                  </WorkNotice>
                )}
              </WorkSection>
            </aside>
          </div>
        </>
      ) : (
        <WorkEmpty
          title="Daemon status unavailable"
          action={
            <button className="work-button" onClick={() => void fetchStatus()}>
              Try again
            </button>
          }
        >
          The service could not be reached.
        </WorkEmpty>
      )}
      <ConfirmModal
        open={stopConfirm}
        title="Stop the daemon?"
        message="The local background service will stop."
        warning="Background synchronization and agent connections may be interrupted. Restart the daemon from your terminal when ready."
        confirmLabel="Stop daemon"
        onConfirm={() => void handleStop()}
        onCancel={() => setStopConfirm(false)}
      />
    </WorkPage>
  );
}
