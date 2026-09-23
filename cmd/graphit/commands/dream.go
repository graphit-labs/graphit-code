package commands

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newDreamCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "dream",
		Short: "Dream module — autonomous idle-triggered Memory consolidation.",
		Long: brand.DisplayName + ` Dream — autonomous Memory consolidation module.

Dream runs during idle periods and lets one constrained agent inspect contextual MCP tools and
apply verified Memory changes directly. Current runs do not create reports or other artifacts.

Commands:
  status   Show current state, the latest operational run, and configuration

Examples:
  ` + brand.BinName() + ` dream status`,
	}

	cmd.AddCommand(newDreamStatusCmd())

	return cmd
}

func newDreamStatusCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show current dream module status",
		Long: `Show the current state of the dream module for this project.

Displays:
  • Whether dream is enabled or disabled
  • Whether the daemon is running
  • Whether a dream session is currently active
  • When the last dream session completed
  • The latest operational run and its tool-attempt counters
  • Current session id and exhaustion state
  • Configured idle timeout and max duration

Examples:
  ` + brand.BinName() + ` dream status`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDreamStatus()
		},
	}
}

func runDreamStatus() error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	var projectCfg config.ConfigMap
	lp := filepath.Join(projectDir, brand.LockFileName())
	if lf, err := hub.LoadLockfile(lp); err == nil && lf != nil {
		projectCfg = lf.Config
	}

	cfg := dream.ResolveDreamConfig(projectCfg)

	currentSessionID, lastUserMod, lastDreamAt, dreamStartedAt, sleepingSince, exhausted, dreaming := dream.LoadStateFromDir(projectDir)

	pid := daemon.NewPIDFile()
	daemonAlive := pid.IsAlive()

	p.Header("Dream Status")

	if cfg.Enabled {
		p.KeyValue("Module", "enabled")
	} else {
		p.KeyValue("Module", "disabled")
		p.Step("Enable with: %s config modules.dream true", brand.BinName())
	}

	if daemonAlive != nil {
		p.KeyValue("Daemon", fmt.Sprintf("running (pid %d)", daemonAlive.PID))
	} else {
		p.KeyValue("Daemon", "not running")
		if cfg.Enabled {
			p.Step("Dream requires the daemon. Start with: %s daemon", brand.BinName())
		}
	}

	if dreaming {
		p.KeyValue("Status", "dreaming")
		if !dreamStartedAt.IsZero() {
			elapsed := time.Since(dreamStartedAt).Truncate(time.Second)
			p.KeyValue("Running for", formatDuration(elapsed))
		}
	} else if exhausted {
		p.KeyValue("Status", "deep sleep (no more improvements)")
		if !sleepingSince.IsZero() {
			elapsed := time.Since(sleepingSince).Truncate(time.Second)
			p.KeyValue("Since", formatDuration(elapsed))
		}
	} else if cfg.Enabled && daemonAlive != nil {
		p.KeyValue("Status", "standby — watching for inactivity")

		if !lastUserMod.IsZero() {
			idleSoFar := time.Since(lastUserMod)
			remaining := cfg.IdleTimeout - idleSoFar
			if remaining > 0 {
				p.KeyValue("Next dream in", formatDuration(remaining.Truncate(time.Second)))
			} else {
				p.KeyValue("Next dream in", "ready (next check cycle)")
			}
		}
	} else {
		p.KeyValue("Status", "inactive")
	}

	if currentSessionID != "" {
		p.KeyValue("Session", currentSessionID)
	}

	if !lastDreamAt.IsZero() {
		ago := time.Since(lastDreamAt).Truncate(time.Second)
		p.KeyValue("Last dream", fmt.Sprintf("%s (%s ago)", lastDreamAt.Format("2006-01-02 15:04:05"), ago))
	} else {
		p.KeyValue("Last dream", "never")
	}

	if !lastUserMod.IsZero() {
		ago := time.Since(lastUserMod).Truncate(time.Second)
		p.KeyValue("Last user edit", fmt.Sprintf("%s (%s ago)", lastUserMod.Format("2006-01-02 15:04:05"), ago))
	}

	if lastRun, err := dream.LatestRun(context.Background(), projectDir); err == nil && lastRun != nil {
		p.Header("Last Agentic Run")
		p.KeyValue("Run", lastRun.RunID)
		p.KeyValue("Status", lastRun.Status)
		if lastRun.CLI != "" {
			p.KeyValue("CLI", lastRun.CLI)
		}
		p.KeyValue("Tool calls", fmt.Sprintf("%d", lastRun.ToolCalls))
		p.KeyValue("Memory mutation attempts", fmt.Sprintf("%d", lastRun.MemoryMutationAttempts))
	}

	p.Header("Configuration")
	p.KeyValue("Idle timeout", cfg.IdleTimeout.String())
	if cfg.MaxDuration > 0 {
		p.KeyValue("Max duration", cfg.MaxDuration.String())
	} else {
		p.KeyValue("Max duration", "unlimited")
	}

	return nil
}

func formatDuration(d time.Duration) string {
	if d < 0 {
		d = 0
	}

	days := int(d.Hours()) / 24
	hours := int(d.Hours()) % 24
	minutes := int(d.Minutes()) % 60
	seconds := int(d.Seconds()) % 60

	switch {
	case days > 0:
		if hours > 0 {
			return fmt.Sprintf("%dd%dh", days, hours)
		}
		return fmt.Sprintf("%dd", days)
	case hours > 0:
		if minutes > 0 {
			return fmt.Sprintf("%dh%dm", hours, minutes)
		}
		return fmt.Sprintf("%dh", hours)
	case minutes > 0:
		if seconds > 0 {
			return fmt.Sprintf("%dm%ds", minutes, seconds)
		}
		return fmt.Sprintf("%dm", minutes)
	default:
		return fmt.Sprintf("%ds", seconds)
	}
}
