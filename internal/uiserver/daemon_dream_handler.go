package uiserver

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
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
	mux.HandleFunc("GET /api/dream/subjects", corsJSON(h.handleDreamSubjects))
	mux.HandleFunc("POST /api/dream/subject", corsJSON(h.handleDreamSubjectAdd))
	mux.HandleFunc("DELETE /api/dream/subject/{slug}", corsJSON(h.handleDreamSubjectRemove))
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

	writeJSON(w, res)
}

func (h *DaemonDreamHandler) handleDaemonStop(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	pid := daemon.NewPIDFile()
	alive := pid.IsAlive()
	if alive == nil {
		writeJSON(w, map[string]any{"success": true, "message": "No daemon running."})
		return
	}

	pidNum := alive.PID
	if err := pid.Signal(syscall.SIGTERM); err != nil {
		http.Error(w, fmt.Sprintf("sending SIGTERM: %v", err), http.StatusInternalServerError)
		return
	}

	for i := 0; i < 20; i++ {
		time.Sleep(200 * time.Millisecond)
		if pid.IsAlive() == nil {
			writeJSON(w, map[string]any{"success": true, "message": fmt.Sprintf("Daemon (PID %d) stopped successfully.", pidNum)})
			return
		}
	}

	// Fallback to SIGKILL
	if err := pid.Signal(syscall.SIGKILL); err != nil {
		http.Error(w, fmt.Sprintf("sending SIGKILL: %v", err), http.StatusInternalServerError)
		return
	}
	pid.Remove()
	writeJSON(w, map[string]any{"success": true, "message": fmt.Sprintf("Daemon (PID %d) did not stop within 4s. Killed via SIGKILL.", pidNum)})
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

	var res DreamStatusResult
	res.Enabled = cfg.Enabled
	res.IdleTimeout = cfg.IdleTimeout.String()
	if cfg.MaxDuration > 0 {
		res.MaxDuration = cfg.MaxDuration.String()
	} else {
		res.MaxDuration = "unlimited"
	}

	currentULID, lastUserMod, lastDreamAt, _, _, exhausted, dreaming := dream.LoadStateFromDir(projectDir)
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

	dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
	if entries, err := scanDreamReportsLocal(dreamDir); err == nil {
		res.TotalReports = len(entries)
	}

	if pending, err := dream.PendingSubjects(projectDir); err == nil {
		for _, s := range pending {
			res.PendingSubjects = append(res.PendingSubjects, s.Title)
		}
	}

	writeJSON(w, res)
}

func (h *DaemonDreamHandler) handleDreamReports(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	dreamDir := filepath.Join(projectDir, brand.DotDir(), "dream")
	info, err := os.Stat(dreamDir)
	if err != nil || !info.IsDir() {
		writeJSON(w, []dreamReportEntry{})
		return
	}

	entries, err := scanDreamReportsLocal(dreamDir)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if entries == nil {
		entries = []dreamReportEntry{}
	}

	sort.Slice(entries, func(i, j int) bool {
		return entries[i].Created.After(entries[j].Created)
	})

	writeJSON(w, entries)
}

func (h *DaemonDreamHandler) handleDreamSubjects(w http.ResponseWriter, r *http.Request) {
	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	subjects, err := dream.ListSubjects(projectDir)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if subjects == nil {
		subjects = []dream.Subject{}
	}

	writeJSON(w, subjects)
}

func (h *DaemonDreamHandler) handleDreamSubjectAdd(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}

	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	var body struct {
		Title string `json:"title"`
		Body  string `json:"body"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if body.Title == "" {
		http.Error(w, "title is required", http.StatusBadRequest)
		return
	}

	subj, err := dream.AddSubject(projectDir, body.Title, body.Body)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, subj)
}

func (h *DaemonDreamHandler) handleDreamSubjectRemove(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodDelete {
		http.Error(w, "DELETE only", http.StatusMethodNotAllowed)
		return
	}

	projectDir := r.URL.Query().Get("project_dir")
	if projectDir == "" {
		http.Error(w, "project_dir required", http.StatusBadRequest)
		return
	}

	slug := r.PathValue("slug")
	if slug == "" {
		http.Error(w, "slug is required", http.StatusBadRequest)
		return
	}

	if err := dream.RemoveSubject(projectDir, slug); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, map[string]any{"success": true, "message": fmt.Sprintf("Subject %q removed.", slug)})
}

type dreamReportEntry struct {
	ID           string    `json:"id"`
	Path         string    `json:"path"`
	Created      time.Time `json:"created"`
	Title        string    `json:"title"`
	Size         int64     `json:"size"`
	HasDeepSleep bool      `json:"has_deep_sleep"`
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
