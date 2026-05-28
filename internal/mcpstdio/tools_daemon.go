package mcpstdio

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
)

type daemonStatusInput struct{}
type daemonStopInput struct{}

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

		return jsonResult(res)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("daemon", "stop"),
		Description: "Stop the running global daemon process.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input daemonStopInput) (*mcp.CallToolResult, any, error) {
		pid := daemon.NewPIDFile()
		alive := pid.IsAlive()
		if alive == nil {
			return textResult("No daemon running.")
		}

		pidNum := alive.PID
		if err := pid.Signal(syscall.SIGTERM); err != nil {
			return errResult(fmt.Errorf("sending SIGTERM: %w", err))
		}

		for i := 0; i < 20; i++ {
			time.Sleep(500 * time.Millisecond)
			if pid.IsAlive() == nil {
				return textResult(fmt.Sprintf("Daemon (PID %d) stopped successfully.", pidNum))
			}
		}

		// Fallback to SIGKILL
		if err := pid.Signal(syscall.SIGKILL); err != nil {
			return errResult(fmt.Errorf("sending SIGKILL: %w", err))
		}
		pid.Remove()
		return textResult(fmt.Sprintf("Daemon (PID %d) did not stop within 10s. Killed via SIGKILL.", pidNum))
	}))
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
