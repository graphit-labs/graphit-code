package mcpstdio

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
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

type dreamReportsInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	All         bool   `json:"all,omitempty" jsonschema:"Show all reports (not just new ones)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type dreamSubjectListInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type dreamSubjectAddInput struct {
	ProjectDir  string `json:"project_dir" jsonschema:"Project directory (required)"`
	Title       string `json:"title" jsonschema:"Subject title/description (required)"`
	Body        string `json:"body,omitempty" jsonschema:"Detailed instructions for the dream agent"`
	AiOptimized *bool  `json:"ai_optimized,omitempty" jsonschema:"Set to false to get verbose JSON instead of compact TOON format (default: true)"`
}

type dreamSubjectRemoveInput struct {
	ProjectDir string `json:"project_dir" jsonschema:"Project directory (required)"`
	Slug       string `json:"slug" jsonschema:"Subject slug to remove (required)"`
}

type DreamStatusResult struct {
	Enabled         bool      `json:"enabled"`
	DaemonRunning   bool      `json:"daemon_running"`
	DaemonPID       int       `json:"daemon_pid,omitempty"`
	Status          string    `json:"status"`
	SessionID       string    `json:"session_id,omitempty"`
	LastDreamAt     time.Time `json:"last_dream_at,omitempty"`
	LastUserEditAt  time.Time `json:"last_user_edit_at,omitempty"`
	IdleTimeout     string    `json:"idle_timeout"`
	MaxDuration     string    `json:"max_duration"`
	TotalReports    int       `json:"total_reports"`
	PendingSubjects []string  `json:"pending_subjects,omitempty"`
}

type dreamReportEntry struct {
	ID           string    `json:"id"`
	Path         string    `json:"path"`
	Created      time.Time `json:"created"`
	Title        string    `json:"title"`
	Size         int64     `json:"size"`
	HasDeepSleep bool      `json:"has_deep_sleep"`
}

type dreamLastSeen struct {
	LastViewed time.Time `json:"last_viewed"`
}

func registerDreamTools(server *mcp.Server) {
	mcp.AddTool(server, &mcp.Tool{
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

			currentULID, lastUserMod, lastDreamAt, _, sleepingSince, exhausted, dreaming := dream.LoadStateFromDir(projectDir)
			res.SessionID = currentULID
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

			dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
			if entries, err := scanDreamReportsLocal(dreamDir); err == nil {
				res.TotalReports = len(entries)
			}

			if pending, err := dream.PendingSubjects(projectDir); err == nil {
				for _, s := range pending {
					res.PendingSubjects = append(res.PendingSubjects, s.Title)
				}
			}
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

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("dream", "reports"),
		Description: "List dream session reports.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input dreamReportsInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var display []dreamReportEntry
		err = withProjectDir(projectDir, func() error {
			dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
			info, err := os.Stat(dreamDir)
			if err != nil || !info.IsDir() {
				return nil
			}

			lastSeen := loadDreamLastSeenLocal(projectDir)
			entries, err := scanDreamReportsLocal(dreamDir)
			if err != nil {
				return err
			}

			if input.All {
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

			saveDreamLastSeenLocal(projectDir)
			return nil
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(display)
		}
		return jsonResult(display)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("dream", "subject_list"),
		Description: "List dream subjects — instructions left for future dream sessions.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input dreamSubjectListInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var subjects []dream.Subject
		err = withProjectDir(projectDir, func() error {
			var serr error
			subjects, serr = dream.ListSubjects(projectDir)
			return serr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(subjects)
		}
		return jsonResult(subjects)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("dream", "subject_add"),
		Description: "Add a new dream subject for a future dream session.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input dreamSubjectAddInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		var subj *dream.Subject
		err = withProjectDir(projectDir, func() error {
			var aerr error
			subj, aerr = dream.AddSubject(projectDir, input.Title, input.Body)
			return aerr
		})
		if err != nil {
			return errResult(err)
		}
		if aiOpt(input.AiOptimized) {
			return toonResult(subj)
		}
		return jsonResult(subj)
	}))

	mcp.AddTool(server, &mcp.Tool{
		Name:        brand.MCPToolName("dream", "subject_remove"),
		Description: "Remove a dream subject by slug.",
	}, safeTool(func(ctx context.Context, req *mcp.CallToolRequest, input dreamSubjectRemoveInput) (*mcp.CallToolResult, any, error) {
		projectDir, err := resolveProjectDir(input.ProjectDir)
		if err != nil {
			return errResult(err)
		}

		err = withProjectDir(projectDir, func() error {
			return dream.RemoveSubject(projectDir, input.Slug)
		})
		if err != nil {
			return errResult(err)
		}
		return textResult(fmt.Sprintf("Subject %q removed.", input.Slug))
	}))
}

func scanDreamReportsLocal(dreamDir string) ([]dreamReportEntry, error) {
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

		entry.Title = extractFrontmatterTitleLocal(path)
		sentinelPath := filepath.Join(dreamDir, id+".exhausted")
		if _, err := os.Stat(sentinelPath); err == nil {
			entry.HasDeepSleep = true
		}

		reports = append(reports, entry)
	}
	return reports, nil
}

func extractFrontmatterTitleLocal(path string) string {
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

func dreamLastSeenPathLocal(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "dream", "dream_last_seen.json")
}

func loadDreamLastSeenLocal(projectDir string) dreamLastSeen {
	var ls dreamLastSeen
	data, err := os.ReadFile(dreamLastSeenPathLocal(projectDir))
	if err != nil {
		return ls
	}
	_ = json.Unmarshal(data, &ls)
	return ls
}

func saveDreamLastSeenLocal(projectDir string) {
	ls := dreamLastSeen{LastViewed: time.Now()}
	data, err := json.MarshalIndent(ls, "", "  ")
	if err != nil {
		return
	}
	fullPath := dreamLastSeenPathLocal(projectDir)
	_ = os.MkdirAll(filepath.Dir(fullPath), 0o755)
	_ = os.WriteFile(fullPath, data, 0o644)
}
