package mcpstdio

import (
	"context"
	"path/filepath"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
)

type dreamStatusInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type DreamStatusResult struct {
	Enabled        bool             `json:"enabled"`
	DaemonRunning  bool             `json:"daemon_running"`
	DaemonPID      int              `json:"daemon_pid,omitempty"`
	Status         string           `json:"status"`
	SessionID      string           `json:"session_id,omitempty"`
	LastDreamAt    time.Time        `json:"last_dream_at,omitempty"`
	LastUserEditAt time.Time        `json:"last_user_edit_at,omitempty"`
	IdleTimeout    string           `json:"idle_timeout"`
	MaxDuration    string           `json:"max_duration"`
	LastRun        *dream.RunRecord `json:"last_run,omitempty"`
}

func registerDreamTools(server *mcp.Server) {
	addTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("dream", "status"),
		Description: "Show status and configuration of the dream module.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input dreamStatusInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var res DreamStatusResult
		err = withProjectDir(projectDir, func() error {
			var projectCfg config.ConfigMap
			lp := filepath.Join(projectDir, brand.LockFileName())
			if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
				projectCfg = lf.Config
			}

			cfg := dream.ResolveDreamConfig(projectCfg)
			res.Enabled = cfg.Enabled
			res.IdleTimeout = cfg.IdleTimeout.String()
			if cfg.MaxDuration > 0 {
				res.MaxDuration = cfg.MaxDuration.String()
			} else {
				res.MaxDuration = "unlimited"
			}

			currentSessionID, lastUserMod, lastDreamAt, _, sleepingSince, exhausted, dreaming := dream.LoadStateFromDir(projectDir)
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
			_ = sleepingSince

			res.LastRun, _ = dream.LatestRun(ctx, projectDir)

			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(res)
		}
		return jsonResult(res)
	}))
}
