package dream

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

const (
	reportsDirName = "dream"

	reportExt = ".md"

	lastSeenFile = "dream_last_seen.json"
)

// Report is one dream session report. The JSON tags are the wire contract for
// both the MCP tools and the HTTP API.
type Report struct {
	ID           string    `json:"id"`
	Path         string    `json:"path"`
	Created      time.Time `json:"created"`
	Title        string    `json:"title"`
	Size         int64     `json:"size"`
	HasDeepSleep bool      `json:"has_deep_sleep"`
}

// LastSeen records when the reports were last read, so that listing only the
// new ones is possible across invocations.
type LastSeen struct {
	LastViewed time.Time `json:"last_viewed"`
}

// ReportsDir returns the directory holding dream session reports, the deep
// sleep sentinels, and the last-seen marker, honouring dream.reports_dir.
func ReportsDir(projectDir string) string {
	if configured := config.ResolveDreamReportsDir(nil, config.LoadProjectConfig(projectDir)); configured != "" {
		return filepath.Join(projectDir, configured)
	}
	return brand.ProjectRuntimePath(projectDir, reportsDirName)
}

// ListReports returns every dream report for a project, newest first.
//
// A missing reports directory is not an error — it means the module has never
// run — and yields no reports.
func ListReports(projectDir string) ([]Report, error) {
	dir := ReportsDir(projectDir)

	dirEntries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var reports []Report
	for _, de := range dirEntries {
		name := de.Name()

		if de.IsDir() || !strings.HasSuffix(name, reportExt) {
			continue
		}

		info, err := de.Info()
		if err != nil {
			continue
		}

		id := strings.TrimSuffix(name, reportExt)
		path := filepath.Join(dir, name)

		report := Report{
			ID:      id,
			Path:    path,
			Created: info.ModTime(),
			Size:    info.Size(),
			Title:   reportTitle(path),
		}

		if fileExists(filepath.Join(dir, id+exhaustedSentinel)) {
			report.HasDeepSleep = true
		}

		reports = append(reports, report)
	}

	sort.Slice(reports, func(i, j int) bool {
		return reports[i].Created.After(reports[j].Created)
	})

	return reports, nil
}

// ReportsSince returns the reports created after t, preserving order.
func ReportsSince(reports []Report, t time.Time) []Report {
	var recent []Report
	for _, r := range reports {
		if r.Created.After(t) {
			recent = append(recent, r)
		}
	}
	return recent
}

// LoadLastSeen reads the last-seen marker. A missing or malformed marker reads
// as the zero time, which makes every report new.
func LoadLastSeen(projectDir string) LastSeen {
	var ls LastSeen
	data, err := os.ReadFile(lastSeenPath(projectDir))
	if err != nil {
		return ls
	}
	_ = json.Unmarshal(data, &ls)
	return ls
}

// MarkReportsSeen advances the last-seen marker to now, so a subsequent
// ReportsSince returns only what arrives after this call.
func MarkReportsSeen(projectDir string) {
	data, err := json.MarshalIndent(LastSeen{LastViewed: time.Now()}, "", "  ")
	if err != nil {
		return
	}
	path := lastSeenPath(projectDir)
	_ = os.MkdirAll(filepath.Dir(path), 0o755)
	_ = os.WriteFile(path, data, 0o644)
}

func lastSeenPath(projectDir string) string {
	return filepath.Join(ReportsDir(projectDir), lastSeenFile)
}

// reportTitle reads `title:` out of a report's YAML frontmatter, returning ""
// when the file has no frontmatter or no title.
func reportTitle(path string) string {
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

	for _, line := range strings.Split(content[4:4+endIdx], "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "title:") {
			return strings.Trim(strings.TrimSpace(strings.TrimPrefix(line, "title:")), "\"'")
		}
	}

	return ""
}
