import { useEffect, useState } from 'react'
import { daemonApi, type DaemonStatus } from '@/api/daemon'
import { Terminal, Square, Activity, FileText, RefreshCw, Cpu, Calendar, Server, Copy, Check, Plug } from 'lucide-react'
import { showToast } from '@/hooks/useToast'

export default function DaemonDashboard() {
  const [status, setStatus] = useState<DaemonStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [stopping, setStopping] = useState(false)
  const [copiedKey, setCopiedKey] = useState(false)

  const copyMcpKey = async () => {
    if (!status?.mcp_key) return
    try {
      await navigator.clipboard.writeText(status.mcp_key)
      setCopiedKey(true)
      showToast('MCP auth key copied to clipboard', 'success')
      setTimeout(() => setCopiedKey(false), 2000)
    } catch {
      showToast('Could not copy the MCP auth key', 'error')
    }
  }

  const fetchStatus = async (silent = false) => {
    if (!silent) setLoading(true)
    try {
      const data = await daemonApi.getStatus()
      setStatus(data)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to load daemon status: ${msg}`, 'error')
    } finally {
      if (!silent) setLoading(false)
    }
  }

  useEffect(() => {
    const timer = setTimeout(() => {
      fetchStatus()
    }, 0)
    const interval = setInterval(() => fetchStatus(true), 5000)
    return () => {
      clearTimeout(timer)
      clearInterval(interval)
    }
  }, [])

  const handleStop = async () => {
    if (!window.confirm('Are you sure you want to stop the background daemon?')) return
    setStopping(true)
    try {
      const res = await daemonApi.stop()
      showToast(res.message, 'success')
      await fetchStatus()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      showToast(`Failed to stop daemon: ${msg}`, 'error')
    } finally {
      setStopping(false)
    }
  }

  const formatUptime = (seconds?: number) => {
    if (seconds == null) return 'N/A'
    const h = Math.floor(seconds / 3600)
    const m = Math.floor((seconds % 3600) / 60)
    const s = Math.floor(seconds % 60)
    if (h > 0) return `${h}h ${m}m ${s}s`
    if (m > 0) return `${m}m ${s}s`
    return `${s}s`
  }

  const formatTime = (timeStr?: string) => {
    if (!timeStr) return 'N/A'
    return new Date(timeStr).toLocaleString()
  }

  if (loading && !status) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <div className="flex flex-col items-center gap-3">
          <RefreshCw className="w-8 h-8 text-primary animate-spin" />
          <p className="text-muted-foreground text-sm">Loading daemon status...</p>
        </div>
      </div>
    )
  }

  const isRunning = status?.running ?? false

  return (
    <div className="w-full max-w-7xl mx-auto px-1 sm:px-2 lg:px-4 py-8 lg:py-12 animate-in fade-in duration-300">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Server className="w-6 h-6 text-primary" />
          </div>
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-primary font-semibold mb-1">Runtime / service</p>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">Daemon</h1>
            <p className="text-[14px] text-muted-foreground mt-1">
              Monitor and manage the global background daemon process
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          <button
            onClick={() => fetchStatus()}
            className="flex items-center justify-center p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel"
            title="Refresh"
          >
            <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-5">
        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4">
          <div className={`p-3 rounded-xl ${isRunning ? 'bg-success/10 text-success' : 'bg-destructive/10 text-destructive'}`}>
            <Activity className="w-5 h-5 animate-pulse" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Status</p>
            <h3 className="text-lg font-bold font-heading">{isRunning ? 'Running' : 'Stopped'}</h3>
            <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold ${isRunning ? 'bg-success/15 text-success' : 'bg-destructive/15 text-destructive'}`}>
              {isRunning ? 'ACTIVE' : 'INACTIVE'}
            </span>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4">
          <div className="p-3 rounded-xl bg-primary/10 text-primary">
            <Cpu className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Process ID (PID)</p>
            <h3 className="text-lg font-bold font-heading">{isRunning ? status?.pid : '—'}</h3>
            <p className="text-[10px] text-muted-foreground truncate max-w-[200px]" title={status?.pid_file_path}>
              PID File: {status?.pid_file_path ? status.pid_file_path.split('/').pop() : 'N/A'}
            </p>
          </div>
        </div>

        <div className="glass-panel p-5 rounded-2xl flex items-start gap-4">
          <div className="p-3 rounded-xl bg-info/10 text-info">
            <Calendar className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-semibold uppercase tracking-wider">Uptime</p>
            <h3 className="text-lg font-bold font-heading">{isRunning ? formatUptime(status?.uptime_seconds) : '—'}</h3>
            <p className="text-[10px] text-muted-foreground truncate" title={status?.started_at}>
              Started: {isRunning ? formatTime(status?.started_at) : 'N/A'}
            </p>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6 mt-6">
        <div className="lg:col-span-3 space-y-4">
          <div className="glass-panel rounded-2xl overflow-hidden flex flex-col h-[500px]">
            <div className="flex items-center justify-between px-5 py-4 border-b border-border/40 bg-card/40">
              <div className="flex items-center gap-2.5">
                <Terminal className="w-4 h-4 text-primary" />
                <span className="font-heading font-semibold text-sm">Recent Daemon Logs</span>
              </div>
              <span className="text-[10px] text-muted-foreground">Showing last 50 lines</span>
            </div>
            <div className="flex-1 p-5 font-mono text-xs overflow-y-auto bg-black/35 text-foreground/80 space-y-1.5 scrollbar-thin">
              {status?.recent_logs && status.recent_logs.length > 0 ? (
                status.recent_logs.map((log, i) => (
                  <div key={i} className="whitespace-pre-wrap leading-relaxed hover:text-foreground transition-colors">
                    {log}
                  </div>
                ))
              ) : (
                <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-2">
                  <FileText className="w-8 h-8 opacity-40" />
                  <p>No log records available.</p>
                </div>
              )}
            </div>
          </div>
        </div>

        <div className="lg:col-span-1 space-y-6">
          <div className="glass-panel p-5 rounded-2xl space-y-4">
            <h3 className="font-heading font-semibold text-sm">Control Panel</h3>
            <p className="text-xs text-muted-foreground leading-relaxed">
              If the daemon behaves abnormally or if you want to stop background sync, click below to shut it down.
            </p>
            {isRunning ? (
              <button
                onClick={handleStop}
                disabled={stopping}
                className="flex items-center justify-center gap-2 w-full px-4 py-2.5 rounded-xl border border-destructive bg-destructive/15 hover:bg-destructive/25 text-destructive font-medium text-sm transition-all duration-300 disabled:opacity-50"
              >
                <Square className="w-4 h-4 fill-current" />
                {stopping ? 'Stopping...' : 'Stop Daemon'}
              </button>
            ) : (
              <div className="p-4 rounded-xl bg-accent/40 border border-border/50 text-xs text-muted-foreground leading-relaxed text-center">
                Daemon is stopped. Start it from your terminal using:
                <div className="bg-black/30 p-2 rounded-lg font-mono text-[10px] text-foreground mt-2 select-all">
                  graphit daemon
                </div>
              </div>
            )}
          </div>

          <div className="glass-panel p-5 rounded-2xl space-y-3">
            <h3 className="font-heading font-semibold text-sm">System Properties</h3>
            <div className="space-y-2 text-xs">
              <div className="flex justify-between py-1 border-b border-border/30">
                <span className="text-muted-foreground">Scheduler</span>
                <span className="font-medium">{status?.scheduler_status ?? 'Unknown'}</span>
              </div>
              <div className="flex justify-between py-1 border-b border-border/30">
                <span className="text-muted-foreground">Log File</span>
                <span className="font-medium truncate max-w-[120px]" title="daemon.log">daemon.log</span>
              </div>
              <div className="flex justify-between py-1">
                <span className="text-muted-foreground">Config Type</span>
                <span className="font-medium">Global</span>
              </div>
              {status?.mcp_port && (
                <>
                  <div className="flex justify-between py-1 border-b border-border/30">
                    <span className="text-muted-foreground">MCP Port</span>
                    <span className="font-medium font-mono text-primary">{status.mcp_port}</span>
                  </div>
                  <div className="flex justify-between py-1 border-b border-border/30">
                    <span className="text-muted-foreground">MCP Endpoint</span>
                    <span className="font-medium font-mono text-[10px] truncate max-w-[140px]" title={status.mcp_endpoint}>
                      {status.mcp_endpoint}
                    </span>
                  </div>
                  <div className="flex justify-between items-center gap-2 py-1">
                    <span className="text-muted-foreground shrink-0">MCP bearer key</span>
                    {status.mcp_key ? (
                      <button
                        onClick={copyMcpKey}
                        title="Copy the MCP bearer key to the clipboard"
                        className="flex items-center gap-1.5 min-w-0 px-2 py-1 rounded-lg border border-border/50 hover:bg-accent/50 transition-all font-mono text-[10px] text-primary"
                      >
                        {copiedKey ? <Check className="w-3 h-3 shrink-0 text-success" /> : <Copy className="w-3 h-3 shrink-0" />}
                        <span className="truncate">{copiedKey ? 'Copied' : maskKey(status.mcp_key)}</span>
                      </button>
                    ) : (
                      <span className="font-medium text-success">Bearer Key</span>
                    )}
                  </div>
                </>
              )}
            </div>
          </div>

          {status?.mcp_endpoint && (
            <div className="glass-panel p-5 rounded-2xl space-y-3">
              <div className="flex items-center gap-2">
                <Plug className="w-4 h-4 text-primary shrink-0" />
                <h3 className="font-heading font-semibold text-sm">Connect an AI agent</h3>
              </div>
              <p className="text-xs text-muted-foreground leading-relaxed">
                Any MCP-capable agent can use this server — Claude Code, Codex, Gemini, Cursor,
                OpenCode, Copilot, Kiro, or your own client. The agent brings its own model; this
                server supplies the code graphs, documentation wikis and memory it reasons over.
              </p>
              <div className="bg-black/30 p-3 rounded-lg font-mono text-[10px] text-foreground overflow-x-auto">
                <pre className="whitespace-pre">{`{
  "mcpServers": {
    "graphit": {
      "url": "${status.mcp_endpoint}",
      "headers": { "Authorization": "Bearer <key>" }
    }
  }
}`}</pre>
              </div>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Copy the full key with the <span className="text-foreground font-medium">MCP bearer key</span> button above.
                It is regenerated every time the daemon restarts, which also revokes every key handed
                out before.
              </p>
              <p className="text-[11px] text-warning/90 leading-relaxed">
                Reaching this from another machine means publishing the port. The endpoint checks the
                bearer key; this UI does not check anything, so keep it off untrusted networks.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function maskKey(key: string): string {
  if (key.length <= 12) return '••••••••'
  return `${key.slice(0, 6)}…${key.slice(-4)}`
}
