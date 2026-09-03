package hub

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func newTestUIServer(t *testing.T) *UIServer {
	t.Helper()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}
	s := &UIServer{svc: svc, port: 9999, mux: http.NewServeMux(), ide: "claude"}
	return s
}

func TestWriteJSONUI(t *testing.T) {
	t.Parallel()
	w := httptest.NewRecorder()
	writeJSONUI(w, map[string]string{"key": "value"})

	if w.Header().Get("Content-Type") != "application/json" {
		t.Errorf("expected application/json content type, got %q", w.Header().Get("Content-Type"))
	}
	if !strings.Contains(w.Body.String(), "value") {
		t.Errorf("expected 'value' in body, got %q", w.Body.String())
	}
}

func TestReadJSONFileUI(t *testing.T) {
	t.Parallel()

	t.Run("valid file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		p := filepath.Join(dir, "test.json")
		_ = os.WriteFile(p, []byte(`{"key":"value"}`), 0o644)
		result := readJSONFileUI(p)
		if result["key"] != "value" {
			t.Errorf("expected 'value', got %v", result["key"])
		}
	})

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		result := readJSONFileUI("/nonexistent/file.json")
		if len(result) != 0 {
			t.Errorf("expected empty map, got %v", result)
		}
	})

	t.Run("invalid json", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		p := filepath.Join(dir, "bad.json")
		_ = os.WriteFile(p, []byte("not json"), 0o644)
		result := readJSONFileUI(p)
		if len(result) != 0 {
			t.Errorf("expected empty map, got %v", result)
		}
	})
}

func TestGetMUI(t *testing.T) {
	t.Parallel()

	t.Run("nil map", func(t *testing.T) {
		t.Parallel()
		got := getMUI(nil, "key", "default")
		if got != "default" {
			t.Errorf("expected 'default', got %q", got)
		}
	})

	t.Run("key exists", func(t *testing.T) {
		t.Parallel()
		m := map[string]any{"key": "value"}
		got := getMUI(m, "key", "default")
		if got != "value" {
			t.Errorf("expected 'value', got %q", got)
		}
	})

	t.Run("key missing", func(t *testing.T) {
		t.Parallel()
		m := map[string]any{"other": "value"}
		got := getMUI(m, "key", "default")
		if got != "default" {
			t.Errorf("expected 'default', got %q", got)
		}
	})

	t.Run("key is empty string", func(t *testing.T) {
		t.Parallel()
		m := map[string]any{"key": ""}
		got := getMUI(m, "key", "default")
		if got != "default" {
			t.Errorf("expected 'default' for empty value, got %q", got)
		}
	})

	t.Run("key is non-string", func(t *testing.T) {
		t.Parallel()
		m := map[string]any{"key": 42}
		got := getMUI(m, "key", "default")
		if got != "default" {
			t.Errorf("expected 'default' for non-string, got %q", got)
		}
	})
}

func TestExtractZip(t *testing.T) {
	t.Parallel()

	t.Run("valid zip", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		zipPath := filepath.Join(dir, "test.zip")

		var buf bytes.Buffer
		w := zip.NewWriter(&buf)
		f, _ := w.Create("hello.txt")
		_, _ = f.Write([]byte("hello world"))
		subf, _ := w.Create("subdir/nested.txt")
		_, _ = subf.Write([]byte("nested content"))
		_ = w.Close()
		_ = os.WriteFile(zipPath, buf.Bytes(), 0o644)

		extractDir := filepath.Join(dir, "extracted")
		err := extractZip(zipPath, extractDir)
		if err != nil {
			t.Fatalf("extractZip failed: %v", err)
		}

		data, err := os.ReadFile(filepath.Join(extractDir, "hello.txt"))
		if err != nil {
			t.Fatalf("read extracted file: %v", err)
		}
		if string(data) != "hello world" {
			t.Errorf("expected 'hello world', got %q", data)
		}

		data2, err := os.ReadFile(filepath.Join(extractDir, "subdir", "nested.txt"))
		if err != nil {
			t.Fatalf("read nested extracted file: %v", err)
		}
		if string(data2) != "nested content" {
			t.Errorf("expected 'nested content', got %q", data2)
		}
	})

	t.Run("invalid zip", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		zipPath := filepath.Join(dir, "bad.zip")
		_ = os.WriteFile(zipPath, []byte("not a zip"), 0o644)
		err := extractZip(zipPath, filepath.Join(dir, "extracted"))
		if err == nil {
			t.Error("expected error for invalid zip")
		}
	})

	t.Run("zip with directory entries", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		zipPath := filepath.Join(dir, "test.zip")

		var buf bytes.Buffer
		w := zip.NewWriter(&buf)
		dh := &zip.FileHeader{Name: "dir/"}
		dh.SetMode(0o755 | os.ModeDir)
		_, _ = w.CreateHeader(dh)
		f, _ := w.Create("dir/file.txt")
		_, _ = f.Write([]byte("data"))
		_ = w.Close()
		_ = os.WriteFile(zipPath, buf.Bytes(), 0o644)

		extractDir := filepath.Join(dir, "extracted")
		err := extractZip(zipPath, extractDir)
		if err != nil {
			t.Fatalf("extractZip failed: %v", err)
		}
	})
}

func TestIsAllowedOrigin(t *testing.T) {
	t.Parallel()
	tests := []struct {
		origin string
		want   bool
	}{
		{"", true},
		{"http://localhost:8080", true},
		{"http://127.0.0.1:8080", true},
		{"http://[::1]:8080", true},
		{"http://localhost", true},
		{"http://127.0.0.1", true},
		{"http://[::1]", true},
		{"http://example.com", false},
		{"https://localhost:8080", false},
		{"http://evil.com:8080", false},
	}
	for _, tc := range tests {
		t.Run(tc.origin, func(t *testing.T) {
			t.Parallel()
			got := isAllowedOrigin(tc.origin)
			if got != tc.want {
				t.Errorf("isAllowedOrigin(%q) = %v, want %v", tc.origin, got, tc.want)
			}
		})
	}
}

func TestCorsWrap(t *testing.T) {
	t.Parallel()

	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(200)
		_, _ = w.Write([]byte("ok"))
	})

	t.Run("normal request with valid origin", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test", nil)
		req.Header.Set("Origin", "http://localhost:3000")
		w := httptest.NewRecorder()
		CorsWrap(handler).ServeHTTP(w, req)
		if w.Header().Get("Access-Control-Allow-Origin") != "http://localhost:3000" {
			t.Error("expected CORS origin header")
		}
		if w.Code != 200 {
			t.Errorf("expected 200, got %d", w.Code)
		}
	})

	t.Run("OPTIONS preflight", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("OPTIONS", "/test", nil)
		req.Header.Set("Origin", "http://localhost:3000")
		w := httptest.NewRecorder()
		CorsWrap(handler).ServeHTTP(w, req)
		if w.Code != 204 {
			t.Errorf("expected 204, got %d", w.Code)
		}
	})

	t.Run("disallowed origin", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test", nil)
		req.Header.Set("Origin", "http://evil.com")
		w := httptest.NewRecorder()
		CorsWrap(handler).ServeHTTP(w, req)
		if w.Header().Get("Access-Control-Allow-Origin") != "" {
			t.Error("expected no CORS origin header for disallowed origin")
		}
	})

	t.Run("no origin header", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test", nil)
		w := httptest.NewRecorder()
		CorsWrap(handler).ServeHTTP(w, req)
		if w.Header().Get("Access-Control-Allow-Origin") != "" {
			t.Error("expected CORS origin header for same-origin")
		}
	})
}

func TestCorsWrapWithAllowedOriginsReplacesTheLocalhostDefault(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})

	for _, tc := range []struct {
		origin string
		want   string
	}{
		{"https://ui.example.test", "https://ui.example.test"},
		{"http://localhost:3000", ""},
		{"https://evil.example.test", ""},
	} {
		req := httptest.NewRequest(http.MethodGet, "/test", nil)
		req.Header.Set("Origin", tc.origin)
		w := httptest.NewRecorder()
		CorsWrapWithAllowedOrigins(handler, []string{"https://ui.example.test"}).ServeHTTP(w, req)
		if got := w.Header().Get("Access-Control-Allow-Origin"); got != tc.want {
			t.Errorf("origin %q reflected as %q; want %q", tc.origin, got, tc.want)
		}
	}
}

func TestCorsWrapWithExplicitWildcardAllowsAnyOrigin(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	req := httptest.NewRequest(http.MethodGet, "/test", nil)
	req.Header.Set("Origin", "https://ui.example.test")
	w := httptest.NewRecorder()
	CorsWrapWithAllowedOrigins(handler, []string{"*"}).ServeHTTP(w, req)
	if got := w.Header().Get("Access-Control-Allow-Origin"); got != "https://ui.example.test" {
		t.Fatalf("wildcard reflected %q", got)
	}
}

func TestUIServer_resolveProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	t.Run("with project_dir query param", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test?project_dir=/tmp/myproject", nil)
		dir, err := s.resolveProjectDir(req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if dir != "/tmp/myproject" {
			t.Errorf("expected '/tmp/myproject', got %q", dir)
		}
	})

	t.Run("without project_dir query param", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test", nil)
		_, err := s.resolveProjectDir(req)
		if err == nil {
			t.Error("expected error without project_dir")
		}
	})
}

func TestUIServer_resolveIDE(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	t.Run("with ide query param", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test?ide=vscode", nil)
		ide := s.resolveIDE(req)
		if ide != "vscode" {
			t.Errorf("expected 'vscode', got %q", ide)
		}
	})

	t.Run("without ide query param", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("GET", "/test", nil)
		ide := s.resolveIDE(req)
		if ide != "claude" {
			t.Errorf("expected 'claude' (server default), got %q", ide)
		}
	})
}

func TestUIServer_Port(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	if s.Port() != 9999 {
		t.Errorf("expected port 9999, got %d", s.Port())
	}
}

func TestUIServer_handleProjects(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	s.svc.registry.projects["p1"] = &Project{RemoteID: "p1", Name: "Proj"}

	req := httptest.NewRequest("GET", "/api/projects", nil)
	w := httptest.NewRecorder()
	s.handleProjects(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	projects, ok := resp["projects"].([]any)
	if !ok {
		t.Fatal("expected projects array in response")
	}
	if len(projects) != 1 {
		t.Errorf("expected 1 project, got %d", len(projects))
	}
}

func TestUIServer_handleRegistry_NoProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("GET", "/api/registry", nil)
	w := httptest.NewRecorder()
	s.handleRegistry(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["error"]; !ok {
		t.Error("expected error in response when project_dir is missing")
	}
}

func TestUIServer_handleRegistry_WithProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	req := httptest.NewRequest("GET", "/api/registry?project_dir="+dir, nil)
	w := httptest.NewRecorder()
	s.handleRegistry(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["entries"]; !ok {
		t.Error("expected entries in response")
	}
	if _, ok := resp["installed"]; !ok {
		t.Error("expected installed in response")
	}
}

func TestUIServer_handleProjectArtifacts_NoProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("GET", "/api/project-artifacts", nil)
	w := httptest.NewRecorder()
	s.handleProjectArtifacts(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["error"]; !ok {
		t.Error("expected error in response when project_dir is missing")
	}
}

func TestUIServer_handleProjectArtifacts_WithProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	req := httptest.NewRequest("GET", "/api/project-artifacts?project_dir="+dir, nil)
	w := httptest.NewRecorder()
	s.handleProjectArtifacts(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["project_name"]; !ok {
		t.Error("expected project_name in response")
	}
}

func TestUIServer_handleInstall(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/install", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleInstall(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] == true {
		t.Error("expected install to fail (no registry entry)")
	}
}

func TestUIServer_handleInstall_WithVersion(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","version":"1.0.0","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/install", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleInstall(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] == true {
		t.Error("expected install to fail (no registry entry)")
	}
}

func TestUIServer_handleUninstall(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/uninstall", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUninstall(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON content type")
	}
}

func TestUIServer_handleUninstall_WithLocalID(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"local_id":"test-local","type":"rule","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/uninstall", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUninstall(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON content type")
	}
}

func TestUIServer_handleUpdateAll(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := `{"ide":"claude","project_dir":"` + dir + `"}`
	req := httptest.NewRequest("POST", "/api/update_all", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUpdateAll(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON content type")
	}
}

func TestUIServer_handleUpdateOne(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/update_one", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUpdateOne(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] == true {
		t.Error("expected update to fail")
	}
}

func TestUIServer_handleSubmit_InvalidBody(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader("not json"))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for invalid body")
	}
}

func TestUIServer_handleSubmit_MissingID(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"path":"/tmp"}`
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for missing ID")
	}
}

func TestUIServer_handleSubmit_MissingPath(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test"}`
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for missing path")
	}
}

func TestUIServer_handleSubmit_PathNotFound(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","path":"/nonexistent/path/xyz"}`
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for nonexistent path")
	}
}

func TestUIServer_handleSubmit_WithTags(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := `{"id":"test","path":"` + dir + `","tags":"go, testing, hub","version":"","type":"","author":"testuser","global":true}`
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleSubmit_ProjectScoped(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := `{"id":"test","path":"` + dir + `","global":false,"project_dir":""}`
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for project-scoped with empty project_dir")
	}
}

func TestUIServer_handleUnpublish_InvalidBody(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("POST", "/api/unpublish", strings.NewReader("bad json"))
	w := httptest.NewRecorder()
	s.handleUnpublish(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure")
	}
}

func TestUIServer_handleUnpublish_MissingID(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"type":"rule"}`
	req := httptest.NewRequest("POST", "/api/unpublish", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUnpublish(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for missing ID")
	}
}

func TestUIServer_handleGitAuthor(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("GET", "/api/git-author", nil)
	w := httptest.NewRecorder()
	s.handleGitAuthor(w, req)

	var resp map[string]string
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["author"]; !ok {
		t.Error("expected author key in response")
	}
}

func TestUIServer_handleGlobalProjects(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("GET", "/api/global-projects", nil)
	w := httptest.NewRecorder()
	s.handleGlobalProjects(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if _, ok := resp["projects"]; !ok {
		t.Error("expected 'projects' key in response")
	}
}

func TestUIServer_handleUnlink(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	t.Run("invalid body", func(t *testing.T) {
		t.Parallel()
		req := httptest.NewRequest("POST", "/api/unlink", strings.NewReader("bad"))
		w := httptest.NewRecorder()
		s.handleUnlink(w, req)
		if w.Code != http.StatusBadRequest {
			t.Errorf("expected 400, got %d", w.Code)
		}
	})

	t.Run("missing type", func(t *testing.T) {
		t.Parallel()
		body := `{"id":"test","project_dir":"/tmp/test"}`
		req := httptest.NewRequest("POST", "/api/unlink", strings.NewReader(body))
		w := httptest.NewRecorder()
		s.handleUnlink(w, req)
		if w.Code != http.StatusInternalServerError {
			t.Errorf("expected 500, got %d", w.Code)
		}
	})
}

func TestScanMCPArtifacts(t *testing.T) {
	t.Parallel()

	t.Run("nonexistent file", func(t *testing.T) {
		t.Parallel()
		result := scanMCPArtifacts("/nonexistent/file.json")
		if result != nil {
			t.Error("expected nil for nonexistent file")
		}
	})

	t.Run("invalid json", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		p := filepath.Join(dir, "mcp.json")
		_ = os.WriteFile(p, []byte("not json"), 0o644)
		result := scanMCPArtifacts(p)
		if result != nil {
			t.Error("expected nil for invalid json")
		}
	})

	t.Run("no mcpServers key", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		p := filepath.Join(dir, "mcp.json")
		_ = os.WriteFile(p, []byte(`{"other":"key"}`), 0o644)
		result := scanMCPArtifacts(p)
		if result != nil {
			t.Error("expected nil when no mcpServers key")
		}
	})
}

func TestNewUIServerOnPort(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}
	s, err := NewUIServerOnPort(svc, "claude", 8888)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if s.Port() != 8888 {
		t.Errorf("expected port 8888, got %d", s.Port())
	}
}

func TestUIServer_RegisterAPIRoutes(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	mux := http.NewServeMux()
	s.RegisterAPIRoutes(mux)

	req := httptest.NewRequest("GET", "/api/projects", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)
	if w.Code == 404 {
		t.Error("expected /api/projects route to be registered")
	}
}

func TestUIServer_Start(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		errCh <- s.Start(ctx)
	}()

	cancel()

	err := <-errCh
	if err != nil {
		t.Logf("Start returned: %v", err)
	}
}

func TestUIServer_handleUpload_NoParsableForm(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	req := httptest.NewRequest("POST", "/api/upload", strings.NewReader("not multipart"))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for unparseable form")
	}
}
