package uiserver

import (
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

type DaemonDreamHandler struct {
	hubSvc *hub.HubService
}

func NewDaemonDreamHandler(hubSvc *hub.HubService) *DaemonDreamHandler {
	return &DaemonDreamHandler{hubSvc: hubSvc}
}

func (h *DaemonDreamHandler) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/daemon/status", corsJSON(h.handleDaemonStatus))
	mux.HandleFunc("POST /api/daemon/stop", corsJSON(h.handleDaemonStop))
	mux.HandleFunc("GET /api/dream/status", corsJSON(h.handleDreamStatus))
	mux.HandleFunc("GET /api/dream/reports", corsJSON(h.handleDreamReports))
}

func (h *DaemonDreamHandler) handleDaemonStatus(w http.ResponseWriter, r *http.Request) {
	pid := daemon.NewPIDFile()
	alive := pid.IsAlive()

	var res struct {
		PID             int       `json:"pid"`
		Running         bool      `json:"running"`
		StartedAt       time.Time `json:"started_at,omitempty"`
		UptimeSeconds   int64     `json:"uptime_seconds,omitempty"`
		PIDFilePath     string    `json:"pid_file_path"`
		SchedulerStatus string    `json:"scheduler_status"`
		RecentLogs      []string  `json:"recent_logs,omitempty"`
		MCPPort         int       `json:"mcp_port,omitempty"`
		MCPEndpoint     string    `json:"mcp_endpoint,omitempty"`
		MCPKeyFile      string    `json:"mcp_key_file,omitempty"`
		// MCPKey is the bearer token itself, so the UI can offer a copy button — pointing an
		// external MCP client at this daemon needs the endpoint AND the key, and reading a
		// 0600 file out of the global directory is not something a browser can do.
		//
		// SECURITY: this endpoint has no authentication of its own; it is protected only by the
		// UI server's bind address and CORS policy. Anyone who can reach the UI port can
		// therefore read the MCP bearer key. That is a smaller step than it sounds — the same
		// port already exposes every project's graph, wiki and memory over unauthenticated
		// routes, so it grants no access the caller did not already have — but it is the reason
		// the UI must stay behind a loopback bind or an authenticating proxy. See
		// docs/guides/s3-and-ui-network.md.
		MCPKey string `json:"mcp_key,omitempty"`
	}
	res.PIDFilePath = pid.Path()
	res.SchedulerStatus = daemon.SchedulerStatus()

	if alive == nil {
		res.Running = false
		writeJSON(w, res)
		return
	}

	res.Running = true
	res.PID = alive.PID
	res.StartedAt = alive.StartedAt
	res.UptimeSeconds = int64(time.Since(alive.StartedAt).Seconds())

	logPath := filepath.Join(daemon.GlobalDaemonDir(), "daemon.log")
	if data, err := os.ReadFile(logPath); err == nil {
		res.RecentLogs = splitLastNLocal(string(data), 50)
	}

	if port, err := mcpproxy.ReadPort(daemonctl.PortFilePath()); err == nil {
		res.MCPPort = port
		mcpHost := advertisedMCPHost(config.ResolveMCPHost(nil, nil), r.Host)
		res.MCPEndpoint = fmt.Sprintf("http://%s/mcp", net.JoinHostPort(mcpHost, strconv.Itoa(port)))
	}
	res.MCPKeyFile = daemonctl.KeyFilePath()
	if key, err := mcpproxy.ReadKey(daemonctl.KeyFilePath()); err == nil {
		res.MCPKey = key
	}

	writeJSON(w, res)
}

func advertisedMCPHost(bindHost, requestHost string) string {
	switch strings.Trim(strings.TrimSpace(bindHost), "[]") {
	case "", "0.0.0.0", "::":
		if host, _, err := net.SplitHostPort(requestHost); err == nil {
			return strings.Trim(host, "[]")
		}
		if host := strings.Trim(strings.TrimSpace(requestHost), "[]"); host != "" {
			return host
		}
		return "127.0.0.1"
	default:
		return strings.Trim(bindHost, "[]")
	}
}

// This server is a module of the daemon it controls, so a handler that signals the daemon is
// signalling its own process: it dies mid-request and the browser is left waiting on a connection
// that will never carry a reply. The stop is therefore handed to a detached process and the reply
// goes out while this one is still up.
var (
	stopDaemonDetached = spawnDetachedStop
	livingDaemonPID    = currentDaemonPID
)

func currentDaemonPID() (int, bool) {
	if alive := daemon.NewPIDFile().IsAlive(); alive != nil {
		return alive.PID, true
	}
	return 0, false
}

// spawnDetachedStop runs the stop command in a process that outlives this one. Reusing that
// command keeps the signal, grace period and cleanup rules in a single place.
func spawnDetachedStop() error {
	exe := daemonctl.ResolveExe()
	if exe == "" {
		return errors.New("cannot locate the executable needed to stop the daemon")
	}
	cmd := exec.Command(exe, "daemon", "stop")
	cmd.Stdin, cmd.Stdout = nil, nil
	closeLog := daemon.AttachLogStderr(cmd)
	defer closeLog()
	sysutil.DetachProcess(cmd)
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("spawning the stop process: %w", err)
	}
	// Nothing waits for it: this process is the one it is about to stop.
	go func() { _ = cmd.Wait() }()
	return nil
}

func (h *DaemonDreamHandler) handleDaemonStop(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	pidNum, running := livingDaemonPID()
	if !running {
		writeJSON(w, map[string]any{"success": true, "message": "No daemon running."})
		return
	}

	if err := stopDaemonDetached(); err != nil {
		http.Error(w, fmt.Sprintf("starting the stop process: %v", err), http.StatusInternalServerError)
		return
	}

	writeJSON(w, map[string]any{"success": true, "message": fmt.Sprintf(
		"Daemon (PID %d) is being stopped by a separate process. This reply is sent before it goes down, so the stop completes just after you read it.", pidNum)})
}

func (h *DaemonDreamHandler) handleDreamStatus(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	var projectCfg config.ConfigMap
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		projectCfg = lf.Config
	}

	cfg := dream.ResolveDreamConfig(projectCfg)

	type DreamStatusResult struct {
		Enabled        bool      `json:"enabled"`
		DaemonRunning  bool      `json:"daemon_running"`
		DaemonPID      int       `json:"daemon_pid,omitempty"`
		Status         string    `json:"status"`
		SessionID      string    `json:"session_id,omitempty"`
		LastDreamAt    time.Time `json:"last_dream_at,omitempty"`
		LastUserEditAt time.Time `json:"last_user_edit_at,omitempty"`
		IdleTimeout    string    `json:"idle_timeout"`
		MaxDuration    string    `json:"max_duration"`
		TotalReports   int       `json:"total_reports"`
	}

	var res DreamStatusResult
	res.Enabled = cfg.Enabled
	res.IdleTimeout = cfg.IdleTimeout.String()
	if cfg.MaxDuration > 0 {
		res.MaxDuration = cfg.MaxDuration.String()
	} else {
		res.MaxDuration = "unlimited"
	}

	currentSessionID, lastUserMod, lastDreamAt, _, _, exhausted, dreaming := dream.LoadStateFromDir(projectDir)
	res.SessionID = currentSessionID
	res.LastDreamAt = lastDreamAt
	res.LastUserEditAt = lastUserMod

	pid := daemon.NewPIDFile()
	daemonAlive := pid.IsAlive()
	if daemonAlive != nil {
		res.DaemonRunning = true
		res.DaemonPID = daemonAlive.PID
	}

	if dreaming {
		res.Status = "dreaming"
	} else if exhausted {
		res.Status = "deep sleep"
	} else if cfg.Enabled && res.DaemonRunning {
		res.Status = "standby"
	} else {
		res.Status = "inactive"
	}

	if reports, err := dream.ListReports(projectDir); err == nil {
		res.TotalReports = len(reports)
	}

	writeJSON(w, res)
}

func (h *DaemonDreamHandler) handleDreamReports(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	reports, err := dream.ListReports(projectDir)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if reports == nil {
		reports = []dream.Report{}
	}

	writeJSON(w, reports)
}

func splitLastNLocal(s string, n int) []string {
	lines := strings.Split(s, "\n")
	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	if len(lines) <= n {
		return lines
	}
	return lines[len(lines)-n:]
}
