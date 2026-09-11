package commands

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
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
		Short: "Dream module — autonomous idle-triggered code improvement.",
		Long: brand.DisplayName + ` Dream — autonomous reflection & improvement module.

The Dream module runs during idle periods, improving project knowledge and agent artifacts.
By default, reports are stored in ` + brand.ProjectRuntimePath(".", "dream") + `.
Set dream.reports_dir to publish them elsewhere.

Commands:
  status   Show current dream state (active, idle, last dream, config)
  reports  List dream session reports

Examples:
  ` + brand.BinName() + ` dream status
  ` + brand.BinName() + ` dream reports`,
	}

	cmd.AddCommand(
		newDreamStatusCmd(),
		newDreamReportsCmd(),
	)

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

		if reports, err := dream.ListReports(projectDir); err == nil && len(reports) > 0 {
			latest := reports[0]
			ago := time.Since(latest.Created).Truncate(time.Second)
			p.KeyValue("Last dream", fmt.Sprintf("%s (%s ago)", latest.Created.Format("2006-01-02 15:04:05"), ago))
		} else {
			p.KeyValue("Last dream", "never")
		}
	}

	if !lastUserMod.IsZero() {
		ago := time.Since(lastUserMod).Truncate(time.Second)
		p.KeyValue("Last user edit", fmt.Sprintf("%s (%s ago)", lastUserMod.Format("2006-01-02 15:04:05"), ago))
	}

	p.Header("Configuration")
	p.KeyValue("Idle timeout", cfg.IdleTimeout.String())
	if cfg.MaxDuration > 0 {
		p.KeyValue("Max duration", cfg.MaxDuration.String())
	} else {
		p.KeyValue("Max duration", "unlimited")
	}

	if reports, err := dream.ListReports(projectDir); err == nil && len(reports) > 0 {
		p.KeyValue("Total reports", fmt.Sprintf("%d", len(reports)))
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

func newDreamReportsCmd() *cobra.Command {
	var showAll bool

	cmd := &cobra.Command{
		Use:   "reports",
		Short: "List dream session reports",
		Long: `List dream reports produced by autonomous dream sessions.

By default, only reports created since the last time this command was run
are shown. Use --all to show all reports.

Each report is a markdown file in ` + filepath.Join(brand.ProjectRuntimePath(".", "dream"), "<id>.md") + ` by default.
Set dream.reports_dir to use another directory.

Examples:
  ` + brand.BinName() + ` dream reports          # new reports since last check
  ` + brand.BinName() + ` dream reports --all     # all reports`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDreamReports(showAll)
		},
	}

	cmd.Flags().BoolVar(&showAll, "all", false, "Show all reports (not just new ones)")
	return cmd
}

func runDreamReports(showAll bool) error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	dreamDir := dream.ReportsDir(projectDir)

	info, err := os.Stat(dreamDir)
	if err != nil || !info.IsDir() {
		p.Info("No dream reports found. The dream module has not run yet.")
		p.Info("Enable dreaming with: %s config modules.dream true", brand.BinName())
		return nil
	}

	lastSeen := dream.LoadLastSeen(projectDir)

	entries, err := dream.ListReports(projectDir)
	if err != nil {
		return fmt.Errorf("scanning dream reports: %w", err)
	}

	if len(entries) == 0 {
		p.Info("No dream reports found in %s", dreamDir)
		return nil
	}

	display := entries
	if !showAll {
		display = dream.ReportsSince(entries, lastSeen.LastViewed)
	}

	if len(display) == 0 {
		p.Success("No new dream reports since last check (%s)", lastSeen.LastViewed.Format("2006-01-02 15:04"))
		p.Info("Use --all to see all %d report(s)", len(entries))
	} else {
		if showAll {
			p.Info("All dream reports (%d total):", len(display))
		} else {
			p.Info("New dream reports since %s (%d new, %d total):",
				lastSeen.LastViewed.Format("2006-01-02 15:04"),
				len(display), len(entries))
		}
		p.Blank()

		for _, e := range display {
			status := "active"
			if e.HasDeepSleep {
				status = "deep sleep"
			}

			title := e.Title
			if title == "" {
				title = "Dream Report"
			}
			p.Step("[%s] %s (%s)", strings.ToUpper(status), title, e.ID)
			p.Detail("Created", e.Created.Format("2006-01-02 15:04:05"))
			p.Detail("Size", humanSize(e.Size))

			relPath := e.Path
			if rel, err := filepath.Rel(projectDir, e.Path); err == nil {
				relPath = rel
			}
			p.Detail("File", relPath)

			if e.HasDeepSleep {
				p.Detail("Status", "Deep sleep (no further improvements found)")
			}
		}
		p.Blank()
	}

	dream.MarkReportsSeen(projectDir)

	return nil
}

func humanSize(bytes int64) string {
	switch {
	case bytes >= 1<<20:
		return fmt.Sprintf("%.1f MB", float64(bytes)/float64(1<<20))
	case bytes >= 1<<10:
		return fmt.Sprintf("%.1f KB", float64(bytes)/float64(1<<10))
	default:
		return fmt.Sprintf("%d B", bytes)
	}
}
