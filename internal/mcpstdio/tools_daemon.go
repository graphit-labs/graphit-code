package mcpstdio

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/daemonservice"
)

type daemonStatusInput struct {
	AiOptimized *bool `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}
type DaemonStatusResult struct {
	PID             int       `json:"pid"`
	Running         bool      `json:"running"`
	StartedAt       time.Time `json:"started_at,omitempty"`
	UptimeSeconds   int64     `json:"uptime_seconds,omitempty"`
	PIDFilePath     string    `json:"pid_file_path"`
	SchedulerStatus string    `json:"scheduler_status"`
	ServiceStatus   string    `json:"service_status"`
	RecentLogs      []string  `json:"recent_logs,omitempty"`
}

func registerDaemonTools(server *mcp.Server) {
	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("daemon", "status"),
		Description: "Check status of the global background daemon process.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input daemonStatusInput) (*mcp.CallToolResult, any, error) {
		pid := daemon.NewPIDFile()
		alive := pid.IsAlive()

		var res DaemonStatusResult
		res.PIDFilePath = pid.Path()
		if service, err := daemonservice.GetStatus(); err == nil {
			res.ServiceStatus = service.String()
			res.SchedulerStatus = res.ServiceStatus
		} else {
			res.ServiceStatus = "unavailable: " + err.Error()
			res.SchedulerStatus = res.ServiceStatus
		}

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
