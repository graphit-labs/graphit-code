package hub

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"

	"github.com/graphit-labs/graphit-code/internal/netutil"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	graphitui "github.com/graphit-labs/graphit-code/internal/ui"
)

type UIServer struct {
	Logger *slog.Logger
	svc    *HubService
	port   int
	mux    *http.ServeMux
	ide    string
}

func (u *UIServer) log() *slog.Logger { return slogutil.Resolve(u.Logger) }

func NewUIServer(svc *HubService, ide string) (*UIServer, error) {
	port, err := netutil.FindFreePort(8100)
	if err != nil {
		return nil, fmt.Errorf("no free port: %w", err)
	}
	s := &UIServer{svc: svc, port: port, mux: http.NewServeMux(), ide: ide}
	s.registerRoutes()
	return s, nil
}

func NewUIServerOnPort(svc *HubService, ide string, port int) (*UIServer, error) {
	s := &UIServer{svc: svc, port: port, mux: http.NewServeMux(), ide: ide}

	return s, nil
}

func (s *UIServer) Port() int { return s.port }

func (s *UIServer) Start(ctx context.Context) error {
	srv := &http.Server{
		Addr:    fmt.Sprintf(":%d", s.port),
		Handler: corsWrap(s.mux),
	}
	ln, _, err := netutil.ListenOnFreePort(s.port)
	if err != nil {
		return err
	}
	errCh := make(chan error, 1)
	go func() { errCh <- srv.Serve(ln) }()
	<-ctx.Done()
	shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutCtx)
	select {
	case e := <-errCh:
		if e != nil && e != http.ErrServerClosed {
			return e
		}
	default:
	}
	return nil
}

func (s *UIServer) RegisterAPIRoutes(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/registry", s.handleRegistry)
	mux.HandleFunc("GET /api/project-artifacts", s.handleProjectArtifacts)
	mux.HandleFunc("GET /api/git-author", s.handleGitAuthor)
	mux.HandleFunc("GET /api/projects", s.handleProjects)
	mux.HandleFunc("GET /api/global-projects", s.handleGlobalProjects)
	mux.HandleFunc("POST /api/install", s.handleInstall)
	mux.HandleFunc("POST /api/uninstall", s.handleUninstall)
	mux.HandleFunc("POST /api/unlink", s.handleUnlink)
	mux.HandleFunc("POST /api/update_all", s.handleUpdateAll)
	mux.HandleFunc("POST /api/update_one", s.handleUpdateOne)
	mux.HandleFunc("POST /api/submit", s.handleSubmit)
	mux.HandleFunc("POST /api/unpublish", s.handleUnpublish)
	mux.HandleFunc("POST /api/upload", s.handleUpload)
}

func (s *UIServer) registerRoutes() {
	s.RegisterAPIRoutes(s.mux)
	s.mux.HandleFunc("/", s.handleUI)
}

func (s *UIServer) resolveProjectDir(r *http.Request) (string, error) {
	if dir := r.URL.Query().Get("project_dir"); dir != "" {
		return dir, nil
	}
	return "", fmt.Errorf("project_dir query parameter is required")
}

func (s *UIServer) resolveIDE(r *http.Request) string {
	if ide := r.URL.Query().Get("ide"); ide != "" {
		return ide
	}
	return s.ide
}

func (s *UIServer) handleGlobalProjects(w http.ResponseWriter, r *http.Request) {
	mgr, err := NewGlobalLockManager()
	if err != nil {
		writeJSONUI(w, map[string]any{"projects": []any{}, "error": err.Error()})
		return
	}
	active, err := mgr.ListActiveProjects()
	if err != nil {
		writeJSONUI(w, map[string]any{"projects": []any{}, "error": err.Error()})
		return
	}

	type projectInfo struct {
		ID   string `json:"id"`
		Name string `json:"name"`
		Dir  string `json:"dir"`
	}
	var projects []projectInfo
	for _, ap := range active {
		name := filepath.Base(ap.Dir)

		lockPath := filepath.Join(ap.Dir, brand.LockFileName())
		if lf := readJSONFileUI(lockPath); lf != nil {
			if proj, ok := lf["project"].(map[string]any); ok {
				if n, ok := proj["name"].(string); ok && n != "" {
					name = n
				}
			}
		}
		projects = append(projects, projectInfo{ID: ap.ID, Name: name, Dir: ap.Dir})
	}

	p := paths.GetPaths(s.ide, false)

	writeJSONUI(w, map[string]any{
		"projects":            projects,
		"current_project_dir": p.ActiveProjectDir,
		"current_ide":         s.ide,
		"supported_ides":      ide.SupportedIDEs(),
	})
}

func (s *UIServer) handleRegistry(w http.ResponseWriter, r *http.Request) {
	projectDir, err := s.resolveProjectDir(r)
	if err != nil {
		writeJSONUI(w, map[string]any{"error": err.Error()})
		return
	}
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lock := readJSONFileUI(lockPath)
	installed := []map[string]any{}
	if arts, ok := lock["artifacts"].(map[string]any); ok {
		for artType, section := range arts {
			if sec, ok := section.(map[string]any); ok {
				for artID, info := range sec {
					inf, _ := info.(map[string]any)
					origin := getMUI(inf, "origin", "")

					if origin == "managed" || origin == "publish" {
						continue
					}
					installed = append(installed, map[string]any{
						"local_id": artID, "type": artType,
						"remote_id":         getMUI(inf, "remote_id", artID),
						"version":           getMUI(inf, "version", "unknown"),
						"origin":            origin,
						"requested_version": getMUI(inf, "requested_version", ""),
					})
				}
			}
		}
	}
	entries := s.svc.registry.ListEntries("")
	proj, _ := lock["project"].(map[string]any)
	activeProjectID := getMUI(proj, "id", "")

	var projectCluster map[string][]string
	if mgr, err := NewGlobalLockManager(); err == nil {
		if lock, err := mgr.Load(); err == nil {
			if _, currentInst := resolveCurrentProject(projectDir, lock); currentInst != nil {
				projectCluster = currentInst.Cluster
			}
		}
	}

	writeJSONUI(w, map[string]any{
		"entries": entries, "installed": installed, "project_lock": lock,
		"active_project":      filepath.Base(projectDir),
		"active_project_id":   activeProjectID,
		"active_project_name": getMUI(proj, "name", ""),
		"project_path":        projectDir, "ide": s.resolveIDE(r),
		"project_cluster": projectCluster,
	})
}

func (s *UIServer) handleProjectArtifacts(w http.ResponseWriter, r *http.Request) {
	projectDir, err := s.resolveProjectDir(r)
	if err != nil {
		writeJSONUI(w, map[string]any{"error": err.Error()})
		return
	}
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	lock := readJSONFileUI(lockPath)

	var importedArtifacts []map[string]any

	var globalArts map[string]*GlobalArtifact
	if mgr, err := NewGlobalLockManager(); err == nil {
		if glock, err := mgr.Load(); err == nil {
			globalArts = glock.Artifacts
		}
	}

	if arts, ok := lock["artifacts"].(map[string]any); ok {
		for artType, section := range arts {
			if sec, ok := section.(map[string]any); ok {
				for artID, info := range sec {
					inf, _ := info.(map[string]any)

					if artType == "skill" && brand.CoreSkillIDs()[artID] {
						continue
					}
					art := map[string]any{
						"local_id":  artID,
						"type":      artType,
						"remote_id": getMUI(inf, "remote_id", artID),
						"version":   getMUI(inf, "version", ""),
						"alias":     getMUI(inf, "alias", ""),
						"origin":    getMUI(inf, "origin", "hub"),
						"path":      getMUI(inf, "path", ""),
						"published": true,
					}

					if globalArts != nil {
						ver := getMUI(inf, "version", "")
						gKey := artType + "/" + artID + "@" + ver
						if ga, ok := globalArts[gKey]; ok {
							if ga.Name != "" {
								art["registry_name"] = ga.Name
							}
							if ga.Description != "" {
								art["registry_description"] = ga.Description
							}
						}
					}

					remoteID := getMUI(inf, "remote_id", artID)
					regEntry := s.svc.registry.GetEntry(remoteID, ArtifactType(artType))
					if regEntry == nil {
						regEntry = s.svc.registry.GetEntry(artID, ArtifactType(artType))
					}
					if regEntry != nil {
						if art["registry_description"] == nil || art["registry_description"] == "" {
							if regEntry.Description != "" {
								art["registry_description"] = regEntry.Description
							}
						}
						if art["registry_name"] == nil || art["registry_name"] == "" {
							if regEntry.Name != "" {
								art["registry_name"] = regEntry.Name
							}
						}

						if regEntry.Latest != "" {
							currentVersion := getMUI(inf, "version", "")
							requestedVersion := getMUI(inf, "requested_version", "")
							targetVersion := regEntry.Latest
							if requestedVersion != "" {
								if constraint, err := ParseVersionConstraint(requestedVersion); err == nil && !constraint.IsLatest() && len(regEntry.Versions) > 0 {
									if resolved, err := ResolveVersion(regEntry.Versions, constraint); err == nil {
										targetVersion = resolved
									}
								}
							}
							art["registry_version"] = targetVersion
							if currentVersion != "" && currentVersion != targetVersion && currentVersion != "unknown" && currentVersion != "latest" {
								art["has_update"] = true
							}
						}
					}
					importedArtifacts = append(importedArtifacts, art)
				}
			}
		}
	}

	var projectArtifacts []map[string]any

	proj, _ := lock["project"].(map[string]any)
	projectName := getMUI(proj, "name", filepath.Base(projectDir))

	adapter := ide.GetAdapter(s.resolveIDE(r))
	if adapter != nil {
		localArts := adapter.ScanLocal(projectDir)
		for _, la := range localArts {
			art := map[string]any{
				"local_id":  la.ID,
				"type":      la.Type,
				"remote_id": la.ID,
				"origin":    "local",
				"path":      la.Path,
				"published": false,
			}

			if regEntry := s.svc.registry.GetEntry(la.ID, ArtifactType(la.Type)); regEntry != nil {
				art["published"] = true
				art["registry_name"] = regEntry.Name
				art["registry_description"] = regEntry.Description
				art["version"] = regEntry.Latest
				if regEntry.ProjectID != "" {
					art["project_id"] = regEntry.ProjectID
				}
			}
			projectArtifacts = append(projectArtifacts, art)
		}

		mcpPath := adapter.MCPConfig()
		if mcpPath != "" {
			projectArtifacts = append(projectArtifacts, scanMCPArtifacts(mcpPath)...)
		}
	}

	knowledgeDir := filepath.Join(projectDir, brand.DotDir(), "knowledge", "project")
	if info, err := os.Stat(knowledgeDir); err == nil && info.IsDir() {
		entries, _ := os.ReadDir(knowledgeDir)
		if len(entries) > 0 {
			art := map[string]any{
				"local_id":  projectName,
				"type":      "knowledge",
				"remote_id": projectName,
				"origin":    "local",
				"path":      knowledgeDir,
				"published": false,
			}
			if regEntry := s.svc.registry.GetEntry(projectName, TypeKnowledge); regEntry != nil {
				art["published"] = true
				art["registry_name"] = regEntry.Name
				art["registry_description"] = regEntry.Description
				art["version"] = regEntry.Latest
				if regEntry.ProjectID != "" {
					art["project_id"] = regEntry.ProjectID
				}
			}
			projectArtifacts = append(projectArtifacts, art)
		}
	}

	astDir := filepath.Join(projectDir, brand.DotDir(), "ast")
	if info, err := os.Stat(astDir); err == nil && info.IsDir() {
		entries, _ := os.ReadDir(astDir)
		for _, e := range entries {
			if !e.IsDir() || strings.HasPrefix(e.Name(), ".") {
				continue
			}

			if e.Type()&os.ModeSymlink != 0 {
				continue
			}
			artID := e.Name()

			displayID := artID
			if artID == "project" {
				displayID = projectName
			}
			art := map[string]any{
				"local_id":  displayID,
				"type":      "ast",
				"remote_id": displayID,
				"origin":    "local",
				"path":      filepath.Join(astDir, artID),
				"published": false,
			}
			if regEntry := s.svc.registry.GetEntry(displayID, TypeAST); regEntry != nil {
				art["published"] = true
				art["registry_name"] = regEntry.Name
				art["registry_description"] = regEntry.Description
				art["version"] = regEntry.Latest
				if regEntry.ProjectID != "" {
					art["project_id"] = regEntry.ProjectID
				}
			}
			projectArtifacts = append(projectArtifacts, art)
		}
	}

	var projectCluster map[string][]string
	if mgr, err := NewGlobalLockManager(); err == nil {
		if lock, err := mgr.Load(); err == nil {
			if _, currentInst := resolveCurrentProject(projectDir, lock); currentInst != nil {
				projectCluster = currentInst.Cluster
			}
		}
	}

	writeJSONUI(w, map[string]any{
		"project_artifacts":  projectArtifacts,
		"imported_artifacts": importedArtifacts,
		"project_name":       projectName,
		"project_path":       projectDir,
		"project_cluster":    projectCluster,
		"ide":                s.resolveIDE(r),
	})
}

func scanMCPArtifacts(mcpPath string) []map[string]any {
	data, err := os.ReadFile(mcpPath)
	if err != nil {
		return nil
	}
	var conf map[string]any
	if json.Unmarshal(data, &conf) != nil {
		return nil
	}

	servers, _ := conf["mcpServers"].(map[string]any)
	if servers == nil {
		return nil
	}

	managed, _ := conf[brand.ManagedMCPKey()].(map[string]any)

	coreServer := brand.MCPServerName("code-stdio")

	var results []map[string]any
	for name := range servers {
		if name == coreServer {
			continue
		}

		if managed != nil {
			if _, ok := managed[name]; !ok {
				continue
			}
		}
		results = append(results, map[string]any{
			"local_id":  name,
			"type":      "mcp",
			"remote_id": name,
			"origin":    "local",
			"path":      mcpPath,
			"published": false,
		})
	}
	return results
}

func (s *UIServer) handleGitAuthor(w http.ResponseWriter, r *http.Request) {
	author := ""
	if email, err := gitmod.Default().RunGlobalOutput("config", "user.email"); err == nil {
		email = strings.TrimSpace(email)
		if strings.Contains(email, "@") {
			author = strings.Split(email, "@")[0]
		} else if email != "" {
			author = email
		}
	}
	writeJSONUI(w, map[string]string{"author": author})
}

func (s *UIServer) handleProjects(w http.ResponseWriter, r *http.Request) {
	projects := s.svc.registry.ListProjects()
	writeJSONUI(w, map[string]any{"projects": projects})
}

func (s *UIServer) handleInstall(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID         string `json:"id"`
		Alias      string `json:"alias"`
		IDE        string `json:"ide"`
		Type       string `json:"type"`
		ProjectDir string `json:"project_dir"`
		Version    string `json:"version"`
	}
	_ = json.NewDecoder(r.Body).Decode(&body)
	if body.IDE == "" {
		body.IDE = s.ide
	}
	installID := body.ID
	if body.Version != "" && !strings.Contains(body.ID, "@") {
		installID = body.ID + "@" + body.Version
	}
	_, err := s.svc.Install(context.Background(), installID, body.Alias, body.IDE, ArtifactType(body.Type), "", body.ProjectDir)
	if err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": err.Error()})
		return
	}
	writeJSONUI(w, map[string]any{"success": true})
}

func (s *UIServer) handleUninstall(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID         string `json:"id"`
		LocalID    string `json:"local_id"`
		IDE        string `json:"ide"`
		Type       string `json:"type"`
		ProjectDir string `json:"project_dir"`
	}
	_ = json.NewDecoder(r.Body).Decode(&body)
	if body.IDE == "" {
		body.IDE = s.ide
	}
	id := body.LocalID
	if id == "" {
		id = body.ID
	}
	err := s.svc.Uninstall(context.Background(), id, ArtifactType(body.Type), false, body.IDE, body.ProjectDir)
	writeJSONUI(w, map[string]any{"success": err == nil})
}

func (s *UIServer) handleUpdateAll(w http.ResponseWriter, r *http.Request) {
	var body struct {
		IDE        string `json:"ide"`
		ProjectDir string `json:"project_dir"`
	}
	_ = json.NewDecoder(r.Body).Decode(&body)
	if body.IDE == "" {
		body.IDE = s.ide
	}
	results := s.svc.UpdateAll(context.Background(), body.IDE, body.ProjectDir)
	hasErr := len(results) > 0
	writeJSONUI(w, map[string]any{"success": !hasErr, "errors": results})
}

func (s *UIServer) handleUpdateOne(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID         string `json:"id"`
		Type       string `json:"type"`
		IDE        string `json:"ide"`
		ProjectDir string `json:"project_dir"`
	}
	_ = json.NewDecoder(r.Body).Decode(&body)
	if body.IDE == "" {
		body.IDE = s.ide
	}
	err := s.svc.UpdateOne(context.Background(), body.ID, ArtifactType(body.Type), body.IDE, body.ProjectDir)
	if err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": err.Error()})
		return
	}
	writeJSONUI(w, map[string]any{"success": true})
}

func (s *UIServer) handleSubmit(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID           string `json:"id"`
		Type         string `json:"type"`
		Version      string `json:"version"`
		Name         string `json:"name"`
		Description  string `json:"description"`
		Tags         string `json:"tags"`
		Author       string `json:"author"`
		Path         string `json:"path"`
		Global       bool   `json:"global"`
		ProjectDir   string `json:"project_dir"`
		Dependencies []struct {
			ID      string `json:"id"`
			Type    string `json:"type"`
			Version string `json:"version"`
		} `json:"dependencies"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": "invalid request body"})
		return
	}

	if body.ID == "" {
		writeJSONUI(w, map[string]any{"success": false, "error": "artifact ID is required"})
		return
	}
	if body.Path == "" {
		writeJSONUI(w, map[string]any{"success": false, "error": "artifact path is required"})
		return
	}
	if body.Version == "" {
		body.Version = "1.0.0"
	}
	if body.Type == "" {
		body.Type = "rule"
	}

	if _, err := os.Stat(body.Path); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": fmt.Sprintf("source path not found: %s", body.Path)})
		return
	}

	var tagList []string
	if body.Tags != "" {
		for _, t := range strings.Split(body.Tags, ",") {
			if t = strings.TrimSpace(t); t != "" {
				tagList = append(tagList, t)
			}
		}
	}

	var deps []Dependency
	for _, d := range body.Dependencies {
		if d.ID != "" {
			deps = append(deps, Dependency{ID: d.ID, Type: ArtifactType(d.Type), Version: d.Version})
		}
	}

	meta := &Entry{
		ID:           body.ID,
		Name:         body.Name,
		Type:         ArtifactType(body.Type),
		Description:  body.Description,
		Tags:         tagList,
		Dependencies: deps,
	}
	if body.Author != "" {
		meta.Author = &Author{Username: body.Author}
	}

	if !body.Global {
		if body.ProjectDir == "" {
			writeJSONUI(w, map[string]any{"success": false, "error": "project_dir is required"})
			return
		}
		lockPath := filepath.Join(body.ProjectDir, brand.LockFileName())
		if lf, err := LoadLockfile(lockPath); err == nil && lf != nil && lf.Project.ID != "" {
			meta.ProjectID = lf.Project.ID
		}
	}

	ctx := context.Background()
	if err := s.svc.registry.PublishEntry(ctx, body.ID, body.Path, meta, body.Version); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": err.Error()})
		return
	}

	if body.ProjectDir != "" {
		ide := s.resolveIDE(r)
		if err := s.svc.RecordPublish(ctx, body.ID, ArtifactType(body.Type), body.Version, ide, body.ProjectDir); err != nil {
			s.log().Warn("record publish in lockfile", "error", err)
		}
	}

	writeJSONUI(w, map[string]any{"success": true})
}

func (s *UIServer) handleUnpublish(w http.ResponseWriter, r *http.Request) {
	var body struct {
		ID         string `json:"id"`
		Type       string `json:"type"`
		ProjectDir string `json:"project_dir"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": "invalid request body"})
		return
	}

	if body.ID == "" {
		writeJSONUI(w, map[string]any{"success": false, "error": "artifact ID is required"})
		return
	}

	ctx := context.Background()
	if err := s.svc.registry.DeleteEntry(ctx, body.ID, ArtifactType(body.Type)); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": err.Error()})
		return
	}

	if body.ProjectDir == "" {
		writeJSONUI(w, map[string]any{"success": false, "error": "project_dir is required"})
		return
	}
	{
		ide := s.resolveIDE(r)
		_ = s.svc.Uninstall(ctx, body.ID, ArtifactType(body.Type), true, ide, body.ProjectDir)
	}

	writeJSONUI(w, map[string]any{"success": true})
}

func (s *UIServer) handleUpload(w http.ResponseWriter, r *http.Request) {

	if err := r.ParseMultipartForm(50 << 20); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": "failed to parse upload: " + err.Error()})
		return
	}

	artifactID := r.FormValue("id")
	artType := r.FormValue("type")
	version := r.FormValue("version")
	name := r.FormValue("name")
	description := r.FormValue("description")
	tags := r.FormValue("tags")
	author := r.FormValue("author")

	if artifactID == "" {
		writeJSONUI(w, map[string]any{"success": false, "error": "artifact ID is required"})
		return
	}
	if version == "" {
		version = "1.0.0"
	}
	if artType == "" {
		artType = "rule"
	}

	tmpDir, err := os.MkdirTemp("", brand.TempDirPrefix("upload"))
	if err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": "failed to create temp dir"})
		return
	}
	defer func() { _ = os.RemoveAll(tmpDir) }()

	isPower := artType == "power"

	file, header, fileErr := r.FormFile("file")
	if fileErr != nil && !isPower {
		writeJSONUI(w, map[string]any{"success": false, "error": "no file uploaded"})
		return
	}

	if file != nil {
		defer func() { _ = file.Close() }()

		destPath := filepath.Join(tmpDir, header.Filename)
		out, err := os.Create(destPath)
		if err != nil {
			writeJSONUI(w, map[string]any{"success": false, "error": "failed to save file"})
			return
		}
		if _, err := io.Copy(out, file); err != nil {
			_ = out.Close()
			writeJSONUI(w, map[string]any{"success": false, "error": "failed to write file"})
			return
		}
		_ = out.Close()

		if strings.HasSuffix(strings.ToLower(header.Filename), ".zip") {
			extractDir := filepath.Join(tmpDir, "extracted")
			if err := extractZip(destPath, extractDir); err != nil {
				writeJSONUI(w, map[string]any{"success": false, "error": "failed to extract zip: " + err.Error()})
				return
			}
			tmpDir = extractDir
		}
	}

	var tagList []string
	if tags != "" {
		for _, t := range strings.Split(tags, ",") {
			if t = strings.TrimSpace(t); t != "" {
				tagList = append(tagList, t)
			}
		}
	}

	var deps []Dependency
	if depsJSON := r.FormValue("dependencies"); depsJSON != "" {
		var rawDeps []struct {
			ID      string `json:"id"`
			Type    string `json:"type"`
			Version string `json:"version"`
		}
		if json.Unmarshal([]byte(depsJSON), &rawDeps) == nil {
			for _, d := range rawDeps {
				if d.ID != "" {
					deps = append(deps, Dependency{ID: d.ID, Type: ArtifactType(d.Type), Version: d.Version})
				}
			}
		}
	}

	meta := &Entry{
		ID:           artifactID,
		Name:         name,
		Type:         ArtifactType(artType),
		Description:  description,
		Tags:         tagList,
		Dependencies: deps,
	}
	if author != "" {
		meta.Author = &Author{Username: author}
	}

	scope := r.FormValue("scope")
	isGlobal := scope == "global"
	recordDir := r.FormValue("project_dir")

	if !isGlobal {
		if recordDir == "" {
			writeJSONUI(w, map[string]any{"success": false, "error": "project_dir is required for project-scoped uploads"})
			return
		}
		lockPath := filepath.Join(recordDir, brand.LockFileName())
		if lf, err := LoadLockfile(lockPath); err == nil && lf != nil && lf.Project.ID != "" {
			meta.ProjectID = lf.Project.ID
		}
	}

	ctx := context.Background()
	if err := s.svc.registry.PublishEntry(ctx, artifactID, tmpDir, meta, version); err != nil {
		writeJSONUI(w, map[string]any{"success": false, "error": err.Error()})
		return
	}

	if recordDir != "" {
		ide := s.resolveIDE(r)
		if err := s.svc.RecordPublish(ctx, artifactID, ArtifactType(artType), version, ide, recordDir); err != nil {
			s.log().Warn("record upload-publish in lockfile", "error", err)
		}
	}

	writeJSONUI(w, map[string]any{"success": true})
}

func (s *UIServer) handleUI(w http.ResponseWriter, r *http.Request) {
	if graphitui.ServeStatic(w, r) {
		return
	}
	data, err := fs.ReadFile(graphitui.DistFS, "dist/index.html")
	if err != nil {
		http.Error(w, "UI not found: "+err.Error(), 500)
		return
	}
	apiBase := fmt.Sprintf("http://localhost:%d", s.port)
	injection := fmt.Sprintf(`<script>
  window.__API_BASE__ = %q;
  window.__APP_MODE__ = "hub";
</script>`, apiBase)
	data = bytes.Replace(data, []byte("</head>"), []byte(injection+"</head>"), 1)
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write(data)
}

func CorsWrap(h http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type")
		if r.Method == http.MethodOptions {
			w.WriteHeader(204)
			return
		}
		h.ServeHTTP(w, r)
	})
}

func corsWrap(h http.Handler) http.Handler { return CorsWrap(h) }

func writeJSONUI(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func readJSONFileUI(path string) map[string]any {
	data, err := os.ReadFile(path)
	if err != nil {
		return map[string]any{}
	}
	var m map[string]any
	if json.Unmarshal(data, &m) != nil {
		return map[string]any{}
	}
	return m
}

func getMUI(m map[string]any, key, def string) string {
	if m == nil {
		return def
	}
	if v, ok := m[key].(string); ok && v != "" {
		return v
	}
	return def
}

func extractZip(zipPath, destDir string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer func() { _ = r.Close() }()

	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}

	for _, f := range r.File {

		target := filepath.Join(destDir, f.Name)
		if !strings.HasPrefix(filepath.Clean(target), filepath.Clean(destDir)+string(os.PathSeparator)) {
			return fmt.Errorf("zip slip detected: %s", f.Name)
		}

		if f.FileInfo().IsDir() {
			if err := os.MkdirAll(target, f.Mode()); err != nil {
				return err
			}
			continue
		}

		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}

		rc, err := f.Open()
		if err != nil {
			return err
		}

		out, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, f.Mode())
		if err != nil {
			_ = rc.Close()
			return err
		}
		if _, err := io.Copy(out, rc); err != nil {
			_ = out.Close()
			_ = rc.Close()
			return err
		}
		_ = out.Close()
		_ = rc.Close()
	}
	return nil
}

func (s *UIServer) handleUnlink(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ID         string `json:"id"`
		Type       string `json:"type"`
		IDE        string `json:"ide"`
		ProjectDir string `json:"project_dir"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	ide := s.ide
	if req.IDE != "" {
		ide = req.IDE
	}

	if err := s.svc.Unlink(context.Background(), req.ID, ide, ArtifactType(req.Type), req.ProjectDir); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusOK)
}
