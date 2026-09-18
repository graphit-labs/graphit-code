package mcpstdio

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

type daemonStatusInput struct {
	AiOptimized *bool `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}
type daemonStartInput struct{}
type daemonRestartInput struct{}

type DaemonStatusResult struct {
	PID             int       `json:"pid"`
	Running         bool      `json:"running"`
	StartedAt       time.Time `json:"started_at,omitempty"`
	UptimeSeconds   int64     `json:"uptime_seconds,omitempty"`
	PIDFilePath     string    `json:"pid_file_path"`
	SchedulerStatus string    `json:"scheduler_status"`
	RecentLogs      []string  `json:"recent_logs,omitempty"`
}

func registerDaemonTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("daemon", "status"),
		Description: "Check status of the global background daemon process.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input daemonStatusInput) (*mcp.CallToolResult, any, error) {
		pid := daemon.NewPIDFile()
		alive := pid.IsAlive()

		var res DaemonStatusResult
		res.PIDFilePath = pid.Path()
		res.SchedulerStatus = daemon.SchedulerStatus()

		if alive == nil {
			res.Running = false
			if aiOpt(input.AiOptimized) {
				return toonResult(res)
			}
			return jsonResult(res)
		}

		res.Running = true
		res.PID = alive.PID
		res.StartedAt = alive.StartedAt
		res.UptimeSeconds = int64(time.Since(alive.StartedAt).Seconds())

		logPath := filepath.Join(daemon.GlobalDaemonDir(), "daemon.log")
		if data, err := os.ReadFile(logPath); err == nil {
			res.RecentLogs = splitLastNLocal(string(data), 10)
		}

		if aiOpt(input.AiOptimized) {
			return toonResult(res)
		}
		return jsonResult(res)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("daemon", "start"),
		Description: "Start the global background daemon if it is not already running.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input daemonStartInput) (*mcp.CallToolResult, any, error) {
		return startDaemon()
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name: brand.MCPToolName("daemon", "restart"),
		Description: "Restart the global background daemon. The swap is carried out by a separate process, " +
			"so this returns right away instead of dying with the daemon it replaces; read the new PID with " +
			brand.MCPToolName("daemon", "status") + ".",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input daemonRestartInput) (*mcp.CallToolResult, any, error) {
		return restartDaemon()
	}))
}

// These tools run inside the daemon they control: the stdio entry point is a relay that forwards
// every call to the daemon's own MCP server. A handler that tears that process down therefore dies
// before it can write its own reply, which is why a stop tool could only ever hang. So nothing here
// stops the current process in the request path — start never stops anything, and restart hands the
// swap to a detached process that outlives this one.
var (
	ensureDaemonRunning = daemon.EnsureRunning
	triggerDaemonSwap   = spawnDetachedRestart
	livingDaemonPID     = currentDaemonPID
)

func currentDaemonPID() (int, bool) {
	if alive := daemon.NewPIDFile().IsAlive(); alive != nil {
		return alive.PID, true
	}
	return 0, false
}

func startDaemon() (*mcp.CallToolResult, any, error) {
	started, err := ensureDaemonRunning()
	if err != nil {
		return errResult(fmt.Errorf("starting the daemon: %w", err))
	}
	if pid, running := livingDaemonPID(); running {
		if started {
			return textResult(fmt.Sprintf("Daemon started (PID %d).", pid))
		}
		return textResult(fmt.Sprintf("Daemon was already running (PID %d).", pid))
	}
	if started {
		return textResult("Daemon start was requested, but no live process is recorded yet.")
	}
	return textResult("No daemon was started and none is recorded as running.")
}

func restartDaemon() (*mcp.CallToolResult, any, error) {
	pid, running := livingDaemonPID()
	if !running {
		return startDaemon()
	}
	if err := triggerDaemonSwap(); err != nil {
		return errResult(fmt.Errorf("triggering the daemon restart: %w", err))
	}
	return textResult(fmt.Sprintf(
		"Restart of the daemon (PID %d) was handed to a separate process, which stops it and starts a replacement. "+
			"This reply is written before that happens, so read the new PID with %s.",
		pid, brand.MCPToolName("daemon", "status")))
}

// spawnDetachedRestart runs the same stop-then-start the restart command performs, in a process
// that survives the daemon's death. Reusing that command keeps one implementation of the signal,
// grace period and cleanup rules instead of a second copy that can drift.
func spawnDetachedRestart() error {
	exe := daemonctl.ResolveExe()
	if exe == "" {
		return errors.New("cannot locate the executable needed to restart the daemon")
	}
	cmd := exec.Command(exe, "daemon", "restart")
	cmd.Stdin, cmd.Stdout = nil, nil
	closeLog := daemon.AttachLogStderr(cmd)
	defer closeLog()
	sysutil.DetachProcess(cmd)
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("spawning the restart process: %w", err)
	}
	// Nothing here waits for it: this process is what it is about to replace.
	go func() { _ = cmd.Wait() }()
	return nil
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
