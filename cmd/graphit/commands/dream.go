package commands

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
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

The Dream module runs during idle periods, analyzing and improving the codebase.
Reports are stored in ` + brand.DotDir() + `/dream/.

Commands:
  status        Show current dream state (active, idle, last dream, config)
  reports       List dream session reports
  subject list  List dream subjects (instructions for future sessions)
  subject add   Add a new subject for a future dream session
  subject rm    Remove a subject

Examples:
  ` + brand.BinName() + ` dream status
  ` + brand.BinName() + ` dream reports
  ` + brand.BinName() + ` dream subject add "Refactor the auth module"
  ` + brand.BinName() + ` dream subject list`,
	}

	cmd.AddCommand(
		newDreamStatusCmd(),
		newDreamReportsCmd(),
		newDreamSubjectCmd(),
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
  • Current session ULID and exhaustion state
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

	currentULID, lastUserMod, lastDreamAt, dreamStartedAt, sleepingSince, exhausted, dreaming := dream.LoadStateFromDir(projectDir)

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

	if currentULID != "" {
		p.KeyValue("Session", currentULID)
	}

	if !lastDreamAt.IsZero() {
		ago := time.Since(lastDreamAt).Truncate(time.Second)
		p.KeyValue("Last dream", fmt.Sprintf("%s (%s ago)", lastDreamAt.Format("2006-01-02 15:04:05"), ago))
	} else {

		dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
		if entries, err := scanDreamReports(dreamDir); err == nil && len(entries) > 0 {
			sort.Slice(entries, func(i, j int) bool {
				return entries[i].Created.After(entries[j].Created)
			})
			latest := entries[0]
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

	dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
	if entries, err := scanDreamReports(dreamDir); err == nil && len(entries) > 0 {
		p.KeyValue("Total reports", fmt.Sprintf("%d", len(entries)))
	}

	if pending, err := dream.PendingSubjects(projectDir); err == nil && len(pending) > 0 {
		p.KeyValue("Pending subjects", fmt.Sprintf("%d", len(pending)))
		for _, s := range pending {
			p.Step("%s", s.Title)
		}
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

type dreamReportEntry struct {
	ID           string
	Path         string
	Created      time.Time
	Title        string
	Size         int64
	HasDeepSleep bool
}

type dreamLastSeen struct {
	LastViewed time.Time `json:"last_viewed"`
}

func newDreamReportsCmd() *cobra.Command {
	var showAll bool

	cmd := &cobra.Command{
		Use:   "reports",
		Short: "List dream session reports",
		Long: `List dream reports produced by autonomous dream sessions.

By default, only reports created since the last time this command was run
are shown. Use --all to show all reports.

Each report is a markdown file in ` + brand.DotDir() + `/dream/<id>.md.

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

	dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")

	info, err := os.Stat(dreamDir)
	if err != nil || !info.IsDir() {
		p.Info("No dream reports found. The dream module has not run yet.")
		p.Info("Enable dreaming with: %s config modules.dream true", brand.BinName())
		return nil
	}

	lastSeen := loadDreamLastSeen(projectDir)

	entries, err := scanDreamReports(dreamDir)
	if err != nil {
		return fmt.Errorf("scanning dream reports: %w", err)
	}

	if len(entries) == 0 {
		p.Info("No dream reports found in %s", dreamDir)
		return nil
	}

	var display []dreamReportEntry
	if showAll {
		display = entries
	} else {
		for _, e := range entries {
			if e.Created.After(lastSeen.LastViewed) {
				display = append(display, e)
			}
		}
	}

	sort.Slice(display, func(i, j int) bool {
		return display[i].Created.After(display[j].Created)
	})

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

	saveDreamLastSeen(projectDir)

	return nil
}

func scanDreamReports(dreamDir string) ([]dreamReportEntry, error) {
	dirEntries, err := os.ReadDir(dreamDir)
	if err != nil {
		return nil, err
	}

	var reports []dreamReportEntry
	for _, de := range dirEntries {
		name := de.Name()

		if de.IsDir() || !strings.HasSuffix(name, ".md") {
			continue
		}

		id := strings.TrimSuffix(name, ".md")
		path := filepath.Join(dreamDir, name)

		info, err := de.Info()
		if err != nil {
			continue
		}

		entry := dreamReportEntry{
			ID:      id,
			Path:    path,
			Created: info.ModTime(),
			Size:    info.Size(),
		}

		entry.Title = extractFrontmatterTitle(path)

		sentinelPath := filepath.Join(dreamDir, id+".exhausted")
		if _, err := os.Stat(sentinelPath); err == nil {
			entry.HasDeepSleep = true
		}

		reports = append(reports, entry)
	}

	return reports, nil
}

func extractFrontmatterTitle(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}

	content := string(data)
	if !strings.HasPrefix(content, "---\n") {
		return ""
	}

	endIdx := strings.Index(content[4:], "\n---")
	if endIdx < 0 {
		return ""
	}

	frontmatter := content[4 : 4+endIdx]
	for _, line := range strings.Split(frontmatter, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "title:") {
			title := strings.TrimPrefix(line, "title:")
			title = strings.TrimSpace(title)

			title = strings.Trim(title, "\"'")
			return title
		}
	}

	return ""
}

const dreamLastSeenFile = "dream_last_seen.json"

func dreamLastSeenPath(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "dream", dreamLastSeenFile)
}

func loadDreamLastSeen(projectDir string) dreamLastSeen {
	var ls dreamLastSeen
	data, err := os.ReadFile(dreamLastSeenPath(projectDir))
	if err != nil {
		return ls
	}
	_ = json.Unmarshal(data, &ls)
	return ls
}

func saveDreamLastSeen(projectDir string) {
	ls := dreamLastSeen{LastViewed: time.Now()}
	data, err := json.MarshalIndent(ls, "", "  ")
	if err != nil {
		return
	}
	fullPath := dreamLastSeenPath(projectDir)
	_ = os.MkdirAll(filepath.Dir(fullPath), 0o755)
	_ = os.WriteFile(fullPath, data, 0o644)
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

func newDreamSubjectListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List dream subjects",
		Long: `List all dream subjects — instructions left for future dream sessions.

Each subject is a markdown file in ` + brand.DotDir() + `/dream/subjects/.
Pending subjects are picked up automatically by the next dream session.

Examples:
  ` + brand.BinName() + ` dream subject list`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDreamSubjects()
		},
	}
}

func runDreamSubjects() error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	subjects, err := dream.ListSubjects(projectDir)
	if err != nil {
		return fmt.Errorf("listing subjects: %w", err)
	}

	if len(subjects) == 0 {
		p.Info("No dream subjects found.")
		p.Step("Add one with: %s dream subject add \"Title of your subject\"", brand.BinName())
		return nil
	}

	var pending, done int
	for _, s := range subjects {
		if s.Done {
			done++
		} else {
			pending++
		}
	}

	p.Header("Dream Subjects")
	p.Info("%d total (%d pending, %d done)", len(subjects), pending, done)
	p.Blank()

	for _, s := range subjects {
		statusLabel := "pending"
		if s.Done {
			statusLabel = "done"
		}

		p.Step("[%s] %s", strings.ToUpper(statusLabel), s.Title)
		p.Detail("Slug", s.Slug)
		p.Detail("Created", s.CreatedAt.Format("2006-01-02 15:04:05"))

		relPath := s.Path
		if rel, err := filepath.Rel(projectDir, s.Path); err == nil {
			relPath = rel
		}
		p.Detail("File", relPath)

		if s.Done {
			relRes := s.ResultPath
			if rel, err := filepath.Rel(projectDir, s.ResultPath); err == nil {
				relRes = rel
			}
			p.Detail("Result", relRes)
		}
	}
	p.Blank()

	return nil
}

func newDreamSubjectCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "subject",
		Short: "Manage dream subjects",
		Long: `Manage dream subjects — instructions for future dream sessions.

Subjects are picked up automatically by the dream module during idle periods.
Each dream session picks the oldest pending subject and works on it.

When the dream agent completes a subject, it creates a corresponding .done.md
file with the results. Pending subjects are those without a .done.md counterpart.

Subcommands:
  list  List all subjects
  add   Create a new subject
  rm    Remove a subject

Examples:
  ` + brand.BinName() + ` dream subject list
  ` + brand.BinName() + ` dream subject add "Refactor the auth module"
  ` + brand.BinName() + ` dream subject add "Add error handling to API" --body "Focus on the /api/v2 endpoints"
  ` + brand.BinName() + ` dream subject rm refactor-the-auth-module`,
	}

	cmd.AddCommand(
		newDreamSubjectListCmd(),
		newDreamSubjectAddCmd(),
		newDreamSubjectRmCmd(),
	)

	return cmd
}

func newDreamSubjectAddCmd() *cobra.Command {
	var body string

	cmd := &cobra.Command{
		Use:   "add [title]",
		Short: "Add a new dream subject",
		Long: `Add a new subject for a future dream session.

The title becomes the filename (slugified). You can optionally provide a body
with detailed instructions using the --body flag.

Examples:
  ` + brand.BinName() + ` dream subject add "Refactor the auth module"
  ` + brand.BinName() + ` dream subject add "Fix API error handling" --body "Focus on /api/v2 endpoints, add proper HTTP status codes"`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDreamSubjectAdd(args[0], body)
		},
	}

	cmd.Flags().StringVar(&body, "body", "", "Detailed instructions for the dream agent")
	return cmd
}

func runDreamSubjectAdd(title, body string) error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	subj, err := dream.AddSubject(projectDir, title, body)
	if err != nil {
		return fmt.Errorf("adding subject: %w", err)
	}

	p.Success("Subject added: %s", subj.Title)
	p.KeyValue("Slug", subj.Slug)
	if rel, err := filepath.Rel(projectDir, subj.Path); err == nil {
		p.KeyValue("File", rel)
	} else {
		p.KeyValue("File", subj.Path)
	}
	p.Step("The next dream session will pick this up automatically.")
	p.Step("Edit the file to add more details if needed.")

	return nil
}

func newDreamSubjectRmCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "rm [slug]",
		Short: "Remove a dream subject",
		Long: `Remove a subject by its slug (filename without extension).

Use '` + brand.BinName() + ` dream subjects' to see available slugs.

Examples:
  ` + brand.BinName() + ` dream subject rm refactor-the-auth-module`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDreamSubjectRm(args[0])
		},
	}
	cmd.ValidArgsFunction = completionDreamSubjectSlugs()
	return cmd
}

func runDreamSubjectRm(slug string) error {
	p := output.NewPrinter("")

	projectDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("resolving project directory: %w", err)
	}

	if err := dream.RemoveSubject(projectDir, slug); err != nil {
		return fmt.Errorf("removing subject: %w", err)
	}

	p.Success("Subject removed: %s", slug)
	return nil
}
