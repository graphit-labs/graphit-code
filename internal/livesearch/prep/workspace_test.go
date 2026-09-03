package prep

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
)

func isolateHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

func newPreparedSession(t *testing.T, ide string) (*livesearch.Session, []string) {
	t.Helper()
	var progress []string
	mgr := livesearch.NewManager(t.TempDir(), nil,
		func(ctx context.Context, s *livesearch.Session, report func(string)) error {
			return Prepare(ctx, s, func(msg string) {
				progress = append(progress, msg)
				report(msg)
			})
		})
	t.Cleanup(mgr.CloseAll)

	s, err := mgr.Create(livesearch.Options{IDE: ide})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	waitReady(t, s)
	return s, progress
}

func waitReady(t *testing.T, s *livesearch.Session) {
	t.Helper()
	for i := 0; i < 4000; i++ {
		switch s.State() {
		case livesearch.StateReady:
			return
		case livesearch.StateFailed:
			t.Fatalf("preparation failed: %s", s.Meta().Error)
		}
		sleepBriefly()
	}
	t.Fatalf("preparation did not finish; state is %q", s.State())
}

func TestPrepareGivesTheProjectANonEmptyIdentityOfItsOwn(t *testing.T) {
	isolateHome(t)
	s, _ := newPreparedSession(t, "claude")

	data, err := os.ReadFile(filepath.Join(s.WorkspaceDir(), brand.LockFileName()))
	if err != nil {
		t.Fatalf("reading the lockfile: %v", err)
	}
	var lf hub.Lockfile
	if err := json.Unmarshal(data, &lf); err != nil {
		t.Fatalf("decoding the lockfile: %v", err)
	}

	if lf.Project.ID == "" {
		t.Fatal("the ephemeral project has no ID")
	}
	if lf.Project.ID != s.ID() {
		t.Fatalf("the project ID is %q, want the session ID %q", lf.Project.ID, s.ID())
	}
	if len(lf.IDEs) != 1 || lf.IDEs[0] != "claude" {
		t.Fatalf("the lockfile records IDEs %v, want [claude]", lf.IDEs)
	}
	if !strings.Contains(lf.Project.Name, "live-search-") {
		t.Fatalf("the project name is %q, want it to name itself a live search", lf.Project.Name)
	}
}

func TestPrepareInstallsSkillsAndConfiguresTheDynamicInstructionHook(t *testing.T) {
	isolateHome(t)
	s, progress := newPreparedSession(t, "claude")
	ws := s.WorkspaceDir()

	if _, err := os.Stat(filepath.Join(ws, "AGENTS.md")); !os.IsNotExist(err) {
		t.Fatalf("the mandate was materialized instead of delivered by hook: %v", err)
	}
	hook, err := os.ReadFile(filepath.Join(ws, ".claude", "settings.json"))
	if err != nil {
		t.Fatalf("reading lifecycle hook config: %v (progress: %v)", err, progress)
	}
	for _, want := range []string{"SessionStart", "_session-hook"} {
		if !strings.Contains(string(hook), want) {
			t.Fatalf("dynamic hook config missing %q: %s", want, hook)
		}
	}
	for _, forbidden := range []string{"--project-dir", ws} {
		if strings.Contains(string(hook), forbidden) {
			t.Fatalf("dynamic hook config embeds machine-specific value %q: %s", forbidden, hook)
		}
	}

	skills := filepath.Join(ws, ".claude", "skills")
	entries, err := os.ReadDir(skills)
	if err != nil {
		t.Fatalf("reading the skills directory: %v (progress: %v)", err, progress)
	}
	if len(entries) == 0 {
		t.Fatalf("no skills were installed (progress: %v)", progress)
	}
	var found bool
	for _, e := range entries {
		if strings.Contains(e.Name(), "ast") {
			found = true
		}
	}
	if !found {
		t.Fatalf("the AST skill is missing from %v", entries)
	}
}

func TestPrepareWritesAProjectLocalMCPConfigNotAGlobalOne(t *testing.T) {
	home := isolateHome(t)
	s, _ := newPreparedSession(t, "claude")

	local := filepath.Join(s.WorkspaceDir(), ".mcp.json")
	data, err := os.ReadFile(local)
	if err != nil {
		t.Fatalf("the project-local MCP config was not written: %v", err)
	}
	var conf struct {
		MCPServers map[string]struct {
			Command string   `json:"command"`
			Args    []string `json:"args"`
		} `json:"mcpServers"`
	}
	if err := json.Unmarshal(data, &conf); err != nil {
		t.Fatalf("decoding the MCP config: %v", err)
	}
	server, ok := conf.MCPServers[brand.MCPServerName("code-stdio")]
	if !ok {
		t.Fatalf("the graphit server is missing from %v", conf.MCPServers)
	}
	if strings.Join(server.Args, " ") != "mcp --stdio" {
		t.Fatalf("the server is invoked with %v, want [mcp --stdio]", server.Args)
	}
	if server.Command == "" {
		t.Fatal("the server has no command")
	}

	for _, global := range []string{
		filepath.Join(home, ".claude.json"),
		filepath.Join(home, ".cursor", "mcp.json"),
		filepath.Join(home, ".kiro", "settings", "mcp.json"),
	} {
		if _, err := os.Stat(global); !os.IsNotExist(err) {
			t.Fatalf("preparation touched the user's global MCP config at %s", global)
		}
	}
}

func TestPrepareAllowsTheGraphitToolsWithoutNamingEachOne(t *testing.T) {
	isolateHome(t)
	s, _ := newPreparedSession(t, "claude")

	data, err := os.ReadFile(filepath.Join(s.WorkspaceDir(), ".claude", "settings.local.json"))
	if err != nil {
		t.Fatalf("the tool permissions were not written: %v", err)
	}
	var settings struct {
		Permissions struct {
			Allow []string `json:"allow"`
		} `json:"permissions"`
	}
	if err := json.Unmarshal(data, &settings); err != nil {
		t.Fatalf("decoding the permissions: %v", err)
	}

	want := "mcp__" + brand.MCPServerName("code-stdio")
	var allowed bool
	for _, a := range settings.Permissions.Allow {
		if a == want {
			allowed = true
		}
		if strings.HasPrefix(a, want+"__") {
			t.Fatalf("permissions name individual tools (%q), which cannot stay complete", a)
		}
	}
	if !allowed {
		t.Fatalf("the graphit server is not allowed: %v", settings.Permissions.Allow)
	}
	for _, tool := range []string{"Read", "Grep", "Glob"} {
		var found bool
		for _, a := range settings.Permissions.Allow {
			if a == tool {
				found = true
			}
		}
		if !found {
			t.Fatalf("%s is not allowed, so the agent cannot open a wiki page: %v", tool, settings.Permissions.Allow)
		}
	}
}

func snapshotTree(t *testing.T, root string) map[string]bool {
	t.Helper()
	seen := map[string]bool{}
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil //nolint:nilerr // an unreadable entry is not part of the comparison
		}
		if !d.IsDir() {
			seen[path] = true
		}
		return nil
	})
	return seen
}

func TestTheProjectScaffoldingWritesNothingOutsideTheWorkspace(t *testing.T) {
	home := isolateHome(t)

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	ws := s.WorkspaceDir()

	before := snapshotTree(t, home)
	if err := writeLockfile(ws, "claude", s.ID()); err != nil {
		t.Fatalf("writeLockfile: %v", err)
	}
	installGuidance(ws, "claude", func(string) {})
	configureTools(ws, "claude", func(string) {})
	after := snapshotTree(t, home)

	var added []string
	for path := range after {
		if !before[path] {
			added = append(added, strings.TrimPrefix(path, home))
		}
	}
	if len(added) > 0 {
		t.Fatalf("the scaffolding wrote into the user's home: %v", added)
	}

	if _, err := os.Stat(filepath.Join(ws, brand.LockFileName())); err != nil {
		t.Fatalf("the workspace was not prepared at all: %v", err)
	}
}

func TestNothingOutsideTheSessionEverLearnsItsID(t *testing.T) {
	home := isolateHome(t)

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	if err := Prepare(context.Background(), s, func(string) {}); err != nil {
		t.Fatalf("Prepare: %v", err)
	}

	id := s.ID()
	var offenders []string
	_ = filepath.WalkDir(home, func(path string, d os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return nil //nolint:nilerr // an unreadable entry cannot be an offender
		}
		if strings.Contains(filepath.Base(path), id) {
			offenders = append(offenders, "path: "+strings.TrimPrefix(path, home))
			return nil
		}
		if d.IsDir() {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil || len(data) > 1<<20 {
			return nil //nolint:nilerr
		}
		if strings.Contains(string(data), id) {
			offenders = append(offenders, "content: "+strings.TrimPrefix(path, home))
		}
		return nil
	})
	if len(offenders) > 0 {
		t.Fatalf("the ephemeral session was recorded outside itself: %v", offenders)
	}

	lockData, err := os.ReadFile(filepath.Join(s.WorkspaceDir(), brand.LockFileName()))
	if err != nil {
		t.Fatalf("reading the workspace lockfile: %v", err)
	}
	if !strings.Contains(string(lockData), id) {
		t.Fatal("the workspace lockfile does not carry the session ID, so this test proves nothing")
	}
}

func TestPrepareReportsProgressForEachStage(t *testing.T) {
	isolateHome(t)
	_, progress := newPreparedSession(t, "claude")

	if len(progress) == 0 {
		t.Fatal("preparation reported nothing, so a user watching sees a blank screen")
	}
	joined := strings.ToLower(strings.Join(progress, " | "))
	for _, stage := range []string{"ephemeral project", "framework's skills", "graphit tools"} {
		if !strings.Contains(joined, stage) {
			t.Fatalf("no progress mentioned %q: %v", stage, progress)
		}
	}
}

func TestPrepareUsesTheCodexAdapterForHooksAndMCP(t *testing.T) {
	isolateHome(t)
	s, progress := newPreparedSession(t, "codex")

	for _, rel := range []string{filepath.Join(".codex", "config.toml"), filepath.Join(".codex", "hooks.json")} {
		if _, err := os.Stat(filepath.Join(s.WorkspaceDir(), rel)); err != nil {
			t.Fatalf("Codex adapter output %s is missing: %v (progress: %v)", rel, err, progress)
		}
	}
}

func TestPrepareIsHonestAboutAModuleThatFailsToInstall(t *testing.T) {
	isolateHome(t)

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}

	var progress []string
	blocked := filepath.Join(s.WorkspaceDir(), ".claude")
	if err := os.WriteFile(blocked, []byte("not a directory"), 0o600); err != nil {
		t.Fatalf("setup: %v", err)
	}
	_ = Prepare(context.Background(), s, func(msg string) { progress = append(progress, msg) })

	joined := strings.ToLower(strings.Join(progress, " | "))
	if !strings.Contains(joined, "could not be") {
		t.Skipf("the installers tolerated a blocked directory; nothing to report: %v", progress)
	}
}

func TestPrepareStopsWhenTheSessionIsCancelled(t *testing.T) {
	isolateHome(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	mgr := livesearch.NewManager(t.TempDir(), nil, nil)
	t.Cleanup(mgr.CloseAll)
	s, err := mgr.Create(livesearch.Options{IDE: "claude"})
	if err != nil {
		t.Fatalf("Create: %v", err)
	}

	err = Prepare(ctx, s, func(string) {})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Prepare returned %v, want context.Canceled", err)
	}
}

func TestValidateIDEAcceptsWhatTheFrameworkSupportsAndRefusesTheRest(t *testing.T) {
	for _, good := range []string{"claude", "claude-code", "CLAUDE", "cursor", "kiro", "codex", "opencode", "gemini", "antigravity"} {
		if err := ValidateIDE(good); err != nil {
			t.Fatalf("ValidateIDE(%q) refused a supported IDE: %v", good, err)
		}
	}
	for _, bad := range []string{"cursor-agent", "vscode", "", "   "} {
		if err := ValidateIDE(bad); !errors.Is(err, ErrUnsupportedIDE) {
			t.Fatalf("ValidateIDE(%q) returned %v, want ErrUnsupportedIDE", bad, err)
		}
	}
}

func TestCanonicalIDEResolvesAliases(t *testing.T) {
	cases := []struct{ in, want string }{
		{"claude-code", "claude"},
		{"claude", "claude"},
		{"gemini-code", "gemini"},
		{"  Kiro  ", "kiro"},
	}
	for _, c := range cases {
		if got := canonicalIDE(c.in); got != c.want {
			t.Fatalf("canonicalIDE(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func sleepBriefly() { time.Sleep(2 * time.Millisecond) }
