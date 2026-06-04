package ide

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

// ---------------------------------------------------------------------------
// GetAdapter
// ---------------------------------------------------------------------------

func TestGetAdapter(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		ide     string
		wantNil bool
	}{
		{"antigravity", "antigravity", false},
		{"cursor", "cursor", false},
		{"claude", "claude", false},
		{"claude-code alias", "claude-code", false},
		{"kiro", "kiro", false},
		{"codex", "codex", false},
		{"opencode", "opencode", false},
		{"gemini", "gemini", false},
		{"gemini-code alias", "gemini-code", false},
		{"case insensitive", "CLAUDE", false},
		{"unknown IDE", "vscode", true},
		{"empty string", "", true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			got := GetAdapter(tc.ide)
			if (got == nil) != tc.wantNil {
				t.Errorf("GetAdapter(%q) nil=%v, want nil=%v", tc.ide, got == nil, tc.wantNil)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// SupportedIDEs
// ---------------------------------------------------------------------------

func TestSupportedIDEs(t *testing.T) {
	t.Parallel()
	ides := SupportedIDEs()
	if len(ides) == 0 {
		t.Fatal("expected non-empty list")
	}
	expected := map[string]bool{
		"antigravity": true,
		"cursor":      true,
		"claude":      true,
		"kiro":        true,
		"codex":       true,
		"opencode":    true,
		"gemini":      true,
	}
	for _, ide := range ides {
		if !expected[ide] {
			t.Errorf("unexpected IDE in list: %q", ide)
		}
		delete(expected, ide)
	}
	for missing := range expected {
		t.Errorf("missing expected IDE: %q", missing)
	}
}

// ---------------------------------------------------------------------------
// GlobalRulesFile
// ---------------------------------------------------------------------------

func TestGlobalRulesFile(t *testing.T) {
	t.Parallel()
	for _, ide := range append(SupportedIDEs(), "unknown") {
		t.Run(ide, func(t *testing.T) {
			t.Parallel()
			got := GlobalRulesFile(ide)
			if got != "AGENTS.md" {
				t.Errorf("GlobalRulesFile(%q) = %q, want AGENTS.md", ide, got)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// GetFileMode
// ---------------------------------------------------------------------------

func TestGetFileMode(t *testing.T) {
	t.Parallel()
	tests := []struct {
		ide      string
		artType  string
		wantMode string
	}{
		{"gemini", "rule", "file"},
		{"gemini", "skill", "folder"},
		{"claude", "rule", "file"},
		{"cursor", "rule", "file"},
		{"cursor", "skill", "folder"},
		{"codex", "command", "file"},
		{"opencode", "agent", "file"},
		{"unknown-ide", "rule", "file"},
		{"gemini", "nonexistent-type", "file"},
	}
	for _, tc := range tests {
		t.Run(tc.ide+"/"+tc.artType, func(t *testing.T) {
			t.Parallel()
			got := GetFileMode(tc.ide, tc.artType)
			if got != tc.wantMode {
				t.Errorf("GetFileMode(%q, %q) = %q, want %q", tc.ide, tc.artType, got, tc.wantMode)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// ArtifactTypePath
// ---------------------------------------------------------------------------

func TestArtifactTypePath(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	tests := []struct {
		name     string
		ide      string
		artType  string
		artName  string
		wantSub  string
		wantErr  bool
	}{
		{"gemini rule", "gemini", "rule", "my-rule", filepath.Join(".gemini", "rules", "my-rule.md"), false},
		{"gemini skill", "gemini", "skill", "my-skill", filepath.Join(".gemini", "skills", "my-skill"), false},
		{"claude command", "claude", "command", "cmd1", filepath.Join(".claude", "commands", "cmd1.md"), false},
		{"cursor rule ext", "cursor", "rule", "r1", filepath.Join(".cursor", "rules", "r1.mdc"), false},
		{"workflow maps to commands", "gemini", "workflow", "wf1", filepath.Join(".gemini", "commands", "wf1.md"), false},
		{"unknown IDE", "no-such-ide", "rule", "x", "", true},
		{"unknown artType", "gemini", "mcp", "x", "", true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			got, err := ArtifactTypePath(dir, tc.ide, tc.artType, tc.artName)
			if tc.wantErr {
				if err == nil {
					t.Error("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			want := filepath.Join(dir, tc.wantSub)
			if got != want {
				t.Errorf("got %q, want %q", got, want)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// expandHome
// ---------------------------------------------------------------------------

func TestExpandHome(t *testing.T) {
	t.Parallel()

	t.Run("with tilde prefix", func(t *testing.T) {
		t.Parallel()
		got, err := expandHome("~/some/path")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		home, _ := os.UserHomeDir()
		want := filepath.Join(home, "some/path")
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("without tilde", func(t *testing.T) {
		t.Parallel()
		got, err := expandHome("/absolute/path")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if got != "/absolute/path" {
			t.Errorf("got %q, want /absolute/path", got)
		}
	})

	t.Run("relative path", func(t *testing.T) {
		t.Parallel()
		got, err := expandHome("relative/path")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if got != "relative/path" {
			t.Errorf("got %q, want relative/path", got)
		}
	})
}

// ---------------------------------------------------------------------------
// projectIDFrom
// ---------------------------------------------------------------------------

func TestProjectIDFrom(t *testing.T) {
	t.Parallel()

	t.Run("nil map", func(t *testing.T) {
		t.Parallel()
		if got := projectIDFrom(nil); got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})

	t.Run("empty map", func(t *testing.T) {
		t.Parallel()
		if got := projectIDFrom(map[string]map[string]string{}); got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})

	t.Run("with project_id", func(t *testing.T) {
		t.Parallel()
		m := map[string]map[string]string{
			"art1": {"type": "rule", "project_id": "proj-123"},
		}
		if got := projectIDFrom(m); got != "proj-123" {
			t.Errorf("got %q, want proj-123", got)
		}
	})

	t.Run("no project_id key", func(t *testing.T) {
		t.Parallel()
		m := map[string]map[string]string{
			"art1": {"type": "rule"},
		}
		if got := projectIDFrom(m); got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})
}

// ---------------------------------------------------------------------------
// findMCPJSON
// ---------------------------------------------------------------------------

func TestFindMCPJSON(t *testing.T) {
	t.Parallel()

	t.Run("mcp.json exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "mcp.json"), []byte("{}"), 0o644)
		got := findMCPJSON(dir)
		if got == "" {
			t.Error("expected non-empty path")
		}
		if filepath.Base(got) != "mcp.json" {
			t.Errorf("expected mcp.json, got %q", filepath.Base(got))
		}
	})

	t.Run("MCP.json exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "MCP.json"), []byte("{}"), 0o644)
		got := findMCPJSON(dir)
		if got == "" {
			t.Error("expected non-empty path")
		}
	})

	t.Run("neither exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		got := findMCPJSON(dir)
		if got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})
}

// ---------------------------------------------------------------------------
// Constructor tests
// ---------------------------------------------------------------------------

func TestNewAntigravityAdapter(t *testing.T) {
	t.Parallel()
	a := NewAntigravityAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".agents" {
		t.Errorf("RootDirName = %q, want .agents", a.cfg.RootDirName)
	}
}

func TestNewCursorAdapter(t *testing.T) {
	t.Parallel()
	a := NewCursorAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".cursor" {
		t.Errorf("RootDirName = %q, want .cursor", a.cfg.RootDirName)
	}
	if a.cfg.FileTypes["rule"].Ext != "mdc" {
		t.Errorf("rule ext = %q, want mdc", a.cfg.FileTypes["rule"].Ext)
	}
}

func TestNewClaudeAdapter(t *testing.T) {
	t.Parallel()
	a := NewClaudeAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".claude" {
		t.Errorf("RootDirName = %q, want .claude", a.cfg.RootDirName)
	}
}

func TestNewKiroAdapter(t *testing.T) {
	t.Parallel()
	a := NewKiroAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".kiro" {
		t.Errorf("RootDirName = %q, want .kiro", a.cfg.RootDirName)
	}
	if a.cfg.RulesDir != "steering" {
		t.Errorf("RulesDir = %q, want steering", a.cfg.RulesDir)
	}
}

func TestNewCodexAdapter(t *testing.T) {
	t.Parallel()
	a := NewCodexAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".codex" {
		t.Errorf("RootDirName = %q, want .codex", a.cfg.RootDirName)
	}
	if !a.cfg.MCPCustomSync {
		t.Error("expected MCPCustomSync=true")
	}
}

func TestNewOpenCodeAdapter(t *testing.T) {
	t.Parallel()
	a := NewOpenCodeAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".opencode" {
		t.Errorf("RootDirName = %q, want .opencode", a.cfg.RootDirName)
	}
}

func TestNewGeminiAdapter(t *testing.T) {
	t.Parallel()
	a := NewGeminiAdapter()
	if a == nil {
		t.Fatal("expected non-nil")
	}
	if a.cfg.RootDirName != ".gemini" {
		t.Errorf("RootDirName = %q, want .gemini", a.cfg.RootDirName)
	}
	if len(a.cfg.MCPExtraPaths) == 0 {
		t.Error("expected MCPExtraPaths")
	}
}

// ---------------------------------------------------------------------------
// NewFolderBasedAdapter defaults
// ---------------------------------------------------------------------------

func TestNewFolderBasedAdapter_Defaults(t *testing.T) {
	t.Parallel()

	t.Run("nil FileTypes gets defaults", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{})
		if a.cfg.FileTypes == nil {
			t.Fatal("expected non-nil FileTypes")
		}
		if a.cfg.FileTypes["rule"].Mode != "file" {
			t.Error("expected default rule mode=file")
		}
	})

	t.Run("empty dirs get defaults", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{})
		if a.cfg.RulesDir != "rules" {
			t.Errorf("RulesDir = %q, want rules", a.cfg.RulesDir)
		}
		if a.cfg.CommandsDir != "commands" {
			t.Errorf("CommandsDir = %q, want commands", a.cfg.CommandsDir)
		}
		if a.cfg.SkillsDir != "skills" {
			t.Errorf("SkillsDir = %q, want skills", a.cfg.SkillsDir)
		}
	})

	t.Run("custom FileTypes preserved", func(t *testing.T) {
		t.Parallel()
		custom := map[string]FileMode{"rule": {Mode: "file", Ext: "txt"}}
		a := NewFolderBasedAdapter(FolderConfig{FileTypes: custom})
		if a.cfg.FileTypes["rule"].Ext != "txt" {
			t.Errorf("expected custom ext=txt, got %q", a.cfg.FileTypes["rule"].Ext)
		}
	})
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.MCPConfig
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_MCPConfig(t *testing.T) {
	t.Parallel()

	t.Run("empty path", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{})
		if got := a.MCPConfig(); got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})

	t.Run("tilde path", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{MCPFilePath: "~/mcp.json"})
		got := a.MCPConfig()
		if strings.HasPrefix(got, "~") {
			t.Errorf("expected expanded path, got %q", got)
		}
		if !strings.HasSuffix(got, "mcp.json") {
			t.Errorf("expected mcp.json suffix, got %q", got)
		}
	})
}

// ---------------------------------------------------------------------------
// copyFile
// ---------------------------------------------------------------------------

func TestCopyFile(t *testing.T) {
	t.Parallel()

	t.Run("basic copy", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		src := filepath.Join(dir, "src.txt")
		dst := filepath.Join(dir, "dst.txt")
		content := "hello world"
		_ = os.WriteFile(src, []byte(content), 0o644)

		if err := copyFile(src, dst); err != nil {
			t.Fatal(err)
		}
		data, _ := os.ReadFile(dst)
		if string(data) != content {
			t.Errorf("got %q, want %q", string(data), content)
		}
	})

	t.Run("creates parent dirs", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		src := filepath.Join(dir, "src.txt")
		dst := filepath.Join(dir, "sub", "deep", "dst.txt")
		_ = os.WriteFile(src, []byte("data"), 0o644)

		if err := copyFile(src, dst); err != nil {
			t.Fatal(err)
		}
		if _, err := os.Stat(dst); err != nil {
			t.Errorf("destination not created: %v", err)
		}
	})

	t.Run("source not found", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := copyFile(filepath.Join(dir, "nonexistent"), filepath.Join(dir, "dst"))
		if err == nil {
			t.Error("expected error for missing source")
		}
	})
}

// ---------------------------------------------------------------------------
// copyDirAll
// ---------------------------------------------------------------------------

func TestCopyDirAll(t *testing.T) {
	t.Parallel()

	t.Run("recursive copy", func(t *testing.T) {
		t.Parallel()
		srcDir := t.TempDir()
		dstDir := filepath.Join(t.TempDir(), "out")

		_ = os.MkdirAll(filepath.Join(srcDir, "sub"), 0o755)
		_ = os.WriteFile(filepath.Join(srcDir, "a.txt"), []byte("a"), 0o644)
		_ = os.WriteFile(filepath.Join(srcDir, "sub", "b.txt"), []byte("b"), 0o644)

		if err := copyDirAll(srcDir, dstDir); err != nil {
			t.Fatal(err)
		}

		data, err := os.ReadFile(filepath.Join(dstDir, "a.txt"))
		if err != nil || string(data) != "a" {
			t.Error("a.txt content mismatch")
		}
		data, err = os.ReadFile(filepath.Join(dstDir, "sub", "b.txt"))
		if err != nil || string(data) != "b" {
			t.Error("sub/b.txt content mismatch")
		}
	})

	t.Run("empty dir", func(t *testing.T) {
		t.Parallel()
		srcDir := t.TempDir()
		dstDir := filepath.Join(t.TempDir(), "out")

		if err := copyDirAll(srcDir, dstDir); err != nil {
			t.Fatal(err)
		}
		info, err := os.Stat(dstDir)
		if err != nil || !info.IsDir() {
			t.Error("expected dest dir to exist")
		}
	})
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.ScanLocal
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_ScanLocal(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
	})

	baseDir := filepath.Join(dir, ".test-ide")
	_ = os.MkdirAll(filepath.Join(baseDir, "rules"), 0o755)
	_ = os.MkdirAll(filepath.Join(baseDir, "commands"), 0o755)
	_ = os.MkdirAll(filepath.Join(baseDir, "skills"), 0o755)
	_ = os.MkdirAll(filepath.Join(baseDir, "agents"), 0o755)

	// Rules are files
	_ = os.WriteFile(filepath.Join(baseDir, "rules", "r1.md"), []byte("rule"), 0o644)
	_ = os.WriteFile(filepath.Join(baseDir, "rules", "r2.md"), []byte("rule"), 0o644)

	// Commands are files
	_ = os.WriteFile(filepath.Join(baseDir, "commands", "c1.md"), []byte("cmd"), 0o644)

	// Skills are folders
	_ = os.MkdirAll(filepath.Join(baseDir, "skills", "my-skill"), 0o755)
	_ = os.WriteFile(filepath.Join(baseDir, "skills", "my-skill", "SKILL.md"), []byte("skill"), 0o644)

	// Core skills should be skipped
	for coreID := range brand.CoreSkillIDs() {
		_ = os.MkdirAll(filepath.Join(baseDir, "skills", coreID), 0o755)
	}

	// Agents are files
	_ = os.WriteFile(filepath.Join(baseDir, "agents", "a1.md"), []byte("agent"), 0o644)

	results := a.ScanLocal(dir)

	typeCount := map[string]int{}
	for _, r := range results {
		typeCount[r.Type]++
	}

	if typeCount["rule"] != 2 {
		t.Errorf("expected 2 rules, got %d", typeCount["rule"])
	}
	if typeCount["command"] != 1 {
		t.Errorf("expected 1 command, got %d", typeCount["command"])
	}
	if typeCount["skill"] != 1 {
		t.Errorf("expected 1 skill (core skills skipped), got %d", typeCount["skill"])
	}
	if typeCount["agent"] != 1 {
		t.Errorf("expected 1 agent, got %d", typeCount["agent"])
	}
}

func TestFolderBasedAdapter_ScanLocal_EmptyDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{RootDirName: ".ide"})
	results := a.ScanLocal(dir)
	if len(results) != 0 {
		t.Errorf("expected 0 results, got %d", len(results))
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.Sync
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_Sync(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
	})

	// Prepare source artifacts
	srcRule := filepath.Join(dir, "src-artifacts", "rule1")
	_ = os.MkdirAll(srcRule, 0o755)
	_ = os.WriteFile(filepath.Join(srcRule, "RULE.md"), []byte("# rule content"), 0o644)

	srcSkill := filepath.Join(dir, "src-artifacts", "skill1")
	_ = os.MkdirAll(srcSkill, 0o755)
	_ = os.WriteFile(filepath.Join(srcSkill, "SKILL.md"), []byte("# skill"), 0o644)

	srcCmd := filepath.Join(dir, "src-artifacts", "cmd1")
	_ = os.MkdirAll(srcCmd, 0o755)
	_ = os.WriteFile(filepath.Join(srcCmd, "COMMAND.md"), []byte("# cmd"), 0o644)

	installed := map[string]map[string]string{
		"rule1":  {"type": "rule", "path": srcRule},
		"skill1": {"type": "skill", "path": srcSkill},
		"cmd1":   {"type": "command", "path": srcCmd},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}

	if err := a.Sync(installed, pp, "proj-123"); err != nil {
		t.Fatal(err)
	}

	// Verify rule was synced
	ruleFile := filepath.Join(dir, ".test-ide", "rules", "rule1.md")
	if _, err := os.Stat(ruleFile); err != nil {
		t.Errorf("rule file not created: %v", err)
	}

	// Verify skill dir was synced
	skillDir := filepath.Join(dir, ".test-ide", "skills", "skill1")
	if _, err := os.Stat(skillDir); err != nil {
		t.Errorf("skill dir not created: %v", err)
	}

	// Verify command was synced
	cmdFile := filepath.Join(dir, ".test-ide", "commands", "cmd1.md")
	if _, err := os.Stat(cmdFile); err != nil {
		t.Errorf("command file not created: %v", err)
	}
}

func TestFolderBasedAdapter_Sync_AgentsFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		AgentsFile:  "COMPILED_AGENTS.md",
	})

	srcAgent := filepath.Join(dir, "src-agents", "agent1")
	_ = os.MkdirAll(srcAgent, 0o755)
	_ = os.WriteFile(filepath.Join(srcAgent, "AGENT.md"), []byte("agent content"), 0o644)

	installed := map[string]map[string]string{
		"agent1": {"type": "agent", "path": srcAgent},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-123"); err != nil {
		t.Fatal(err)
	}

	agentsFile := filepath.Join(dir, "COMPILED_AGENTS.md")
	data, err := os.ReadFile(agentsFile)
	if err != nil {
		t.Fatalf("agents file not created: %v", err)
	}
	if !strings.Contains(string(data), "AGENT1") {
		t.Error("expected compiled agent content")
	}
}

func TestFolderBasedAdapter_Sync_Workflow(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		CommandsDir: "commands",
	})

	srcWf := filepath.Join(dir, "src-wf", "wf1")
	_ = os.MkdirAll(srcWf, 0o755)
	_ = os.WriteFile(filepath.Join(srcWf, "readme.md"), []byte("workflow"), 0o644)

	installed := map[string]map[string]string{
		"wf1": {"type": "workflow", "path": srcWf},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-123"); err != nil {
		t.Fatal(err)
	}

	wfFile := filepath.Join(dir, ".test-ide", "commands", "wf1.md")
	if _, err := os.Stat(wfFile); err != nil {
		t.Errorf("workflow file not synced: %v", err)
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.Remove
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_Remove(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		RulesDir:    "rules",
		SkillsDir:   "skills",
	})

	baseDir := filepath.Join(dir, ".test-ide")
	_ = os.MkdirAll(filepath.Join(baseDir, "rules"), 0o755)
	_ = os.MkdirAll(filepath.Join(baseDir, "skills", "s1"), 0o755)
	_ = os.WriteFile(filepath.Join(baseDir, "rules", "r1.md"), []byte("rule"), 0o644)
	_ = os.WriteFile(filepath.Join(baseDir, "skills", "s1", "SKILL.md"), []byte("skill"), 0o644)

	installed := map[string]map[string]string{
		"r1": {"type": "rule"},
		"s1": {"type": "skill"},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Remove(pp, installed); err != nil {
		t.Fatal(err)
	}

	if _, err := os.Stat(filepath.Join(baseDir, "rules", "r1.md")); !os.IsNotExist(err) {
		t.Error("expected rule file to be removed")
	}
	if _, err := os.Stat(filepath.Join(baseDir, "skills", "s1")); !os.IsNotExist(err) {
		t.Error("expected skill dir to be removed")
	}
}

func TestFolderBasedAdapter_Remove_NilInstalled(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{RootDirName: ".test-ide"})

	baseDir := filepath.Join(dir, ".test-ide")
	_ = os.MkdirAll(baseDir, 0o755)
	_ = os.WriteFile(filepath.Join(baseDir, "stuff.txt"), []byte("data"), 0o644)

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Remove(pp, nil); err != nil {
		t.Fatal(err)
	}

	if _, err := os.Stat(baseDir); !os.IsNotExist(err) {
		t.Error("expected base dir to be removed when installed is nil")
	}
}

func TestFolderBasedAdapter_Remove_AgentsFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		AgentsFile:  "AGENTS_COMPILED.md",
	})

	agentsFile := filepath.Join(dir, "AGENTS_COMPILED.md")
	_ = os.WriteFile(agentsFile, []byte("compiled"), 0o644)

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Remove(pp, nil)

	if _, err := os.Stat(agentsFile); !os.IsNotExist(err) {
		t.Error("expected agents file to be removed")
	}
}

// ---------------------------------------------------------------------------
// reconcileMCPFile
// ---------------------------------------------------------------------------

func TestReconcileMCPFile(t *testing.T) {
	t.Parallel()

	t.Run("empty target path", func(t *testing.T) {
		t.Parallel()
		if err := reconcileMCPFile("", "proj", map[string]any{}); err != nil {
			t.Errorf("expected nil error for empty path, got: %v", err)
		}
	})

	t.Run("new file with servers", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		target := filepath.Join(dir, "mcp.json")

		servers := map[string]any{
			"server1": map[string]any{"command": "echo"},
		}
		if err := reconcileMCPFile(target, "proj-1", servers); err != nil {
			t.Fatal(err)
		}

		data, _ := os.ReadFile(target)
		var parsed map[string]any
		_ = json.Unmarshal(data, &parsed)

		mcpServers, _ := parsed["mcpServers"].(map[string]any)
		if mcpServers == nil || mcpServers["server1"] == nil {
			t.Error("expected server1 in mcpServers")
		}

		managedKey := brand.ManagedMCPKey()
		managed, _ := parsed[managedKey].(map[string]any)
		if managed == nil || managed["server1"] == nil {
			t.Error("expected server1 in managed keys")
		}
	})

	t.Run("merge with existing file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		target := filepath.Join(dir, "mcp.json")

		existing := map[string]any{
			"mcpServers": map[string]any{
				"user-server": map[string]any{"command": "user-cmd"},
			},
		}
		data, _ := json.Marshal(existing)
		_ = os.WriteFile(target, data, 0o644)

		servers := map[string]any{
			"new-server": map[string]any{"command": "new-cmd"},
		}
		if err := reconcileMCPFile(target, "proj-1", servers); err != nil {
			t.Fatal(err)
		}

		data, _ = os.ReadFile(target)
		var parsed map[string]any
		_ = json.Unmarshal(data, &parsed)

		mcpServers, _ := parsed["mcpServers"].(map[string]any)
		if mcpServers["user-server"] == nil {
			t.Error("expected user-server to be preserved")
		}
		if mcpServers["new-server"] == nil {
			t.Error("expected new-server to be added")
		}
	})

	t.Run("remove claims", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		target := filepath.Join(dir, "mcp.json")

		// First, add a server
		servers := map[string]any{
			"my-server": map[string]any{"command": "cmd"},
		}
		_ = reconcileMCPFile(target, "proj-1", servers)

		// Then remove all claims for proj-1
		_ = reconcileMCPFile(target, "proj-1", map[string]any{})

		data, _ := os.ReadFile(target)
		var parsed map[string]any
		_ = json.Unmarshal(data, &parsed)

		mcpServers, _ := parsed["mcpServers"].(map[string]any)
		if mcpServers["my-server"] != nil {
			t.Error("expected my-server to be removed when no claims remain")
		}
	})

	t.Run("multi-project claims preserve server", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		target := filepath.Join(dir, "mcp.json")

		servers := map[string]any{
			"shared-server": map[string]any{"command": "cmd"},
		}
		_ = reconcileMCPFile(target, "proj-1", servers)
		_ = reconcileMCPFile(target, "proj-2", servers)

		// Remove proj-1 claims
		_ = reconcileMCPFile(target, "proj-1", map[string]any{})

		data, _ := os.ReadFile(target)
		var parsed map[string]any
		_ = json.Unmarshal(data, &parsed)

		mcpServers, _ := parsed["mcpServers"].(map[string]any)
		if mcpServers["shared-server"] == nil {
			t.Error("expected shared-server to remain (proj-2 still claims it)")
		}
	})
}

// ---------------------------------------------------------------------------
// InjectManagedBlock / RemoveManagedBlock
// ---------------------------------------------------------------------------

func TestInjectManagedBlock(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	t.Run("inject into new file", func(t *testing.T) {
		err := InjectManagedBlock(dir, "gemini", "TEST_MODULE", "test content")
		if err != nil {
			t.Fatal(err)
		}

		data, _ := os.ReadFile(filepath.Join(dir, "AGENTS.md"))
		content := string(data)
		if !strings.Contains(content, "test content") {
			t.Error("expected injected content")
		}
		marker := blockMarkerForName("TEST_MODULE")
		if !strings.Contains(content, marker) {
			t.Errorf("expected marker %q in content", marker)
		}
	})

	t.Run("update existing block", func(t *testing.T) {
		err := InjectManagedBlock(dir, "gemini", "TEST_MODULE", "updated content")
		if err != nil {
			t.Fatal(err)
		}

		data, _ := os.ReadFile(filepath.Join(dir, "AGENTS.md"))
		content := string(data)
		if !strings.Contains(content, "updated content") {
			t.Error("expected updated content")
		}
		if strings.Contains(content, "test content") {
			t.Error("old content should have been replaced")
		}
	})
}

func TestRemoveManagedBlock(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	_ = InjectManagedBlock(dir, "gemini", "REMOVE_ME", "to be removed")

	err := RemoveManagedBlock(dir, "gemini", "REMOVE_ME")
	if err != nil {
		t.Fatal(err)
	}

	data, _ := os.ReadFile(filepath.Join(dir, "AGENTS.md"))
	if strings.Contains(string(data), "to be removed") {
		t.Error("expected block content to be removed")
	}
}

// ---------------------------------------------------------------------------
// InstallManagedSkill / RemoveManagedSkill
// ---------------------------------------------------------------------------

func TestInstallManagedSkill(t *testing.T) {
	t.Parallel()

	t.Run("install for known IDE", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := InstallManagedSkill(dir, "gemini", "test-skill", "# My Skill")
		if err != nil {
			t.Fatal(err)
		}

		skillFile := filepath.Join(dir, ".gemini", "skills", "test-skill", "SKILL.md")
		data, err := os.ReadFile(skillFile)
		if err != nil {
			t.Fatalf("skill file not created: %v", err)
		}
		if string(data) != "# My Skill" {
			t.Errorf("content = %q, want '# My Skill'", string(data))
		}
	})

	t.Run("install for each adapter type", func(t *testing.T) {
		t.Parallel()
		for _, ide := range []string{"claude", "codex", "opencode", "antigravity", "cursor"} {
			t.Run(ide, func(t *testing.T) {
				t.Parallel()
				dir := t.TempDir()
				err := InstallManagedSkill(dir, ide, "s1", "content")
				if err != nil {
					t.Fatalf("install for %s failed: %v", ide, err)
				}
			})
		}
	})

	t.Run("unknown IDE", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := InstallManagedSkill(dir, "no-such-ide", "s1", "content")
		if err == nil {
			t.Error("expected error for unknown IDE")
		}
	})
}

func TestRemoveManagedSkill(t *testing.T) {
	t.Parallel()

	t.Run("remove existing skill", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = InstallManagedSkill(dir, "gemini", "to-remove", "content")

		err := RemoveManagedSkill(dir, "gemini", "to-remove")
		if err != nil {
			t.Fatal(err)
		}

		skillDir := filepath.Join(dir, ".gemini", "skills", "to-remove")
		if _, err := os.Stat(skillDir); !os.IsNotExist(err) {
			t.Error("expected skill dir to be removed")
		}
	})

	t.Run("remove for each adapter type", func(t *testing.T) {
		t.Parallel()
		for _, ide := range []string{"claude", "codex", "opencode", "antigravity"} {
			t.Run(ide, func(t *testing.T) {
				t.Parallel()
				dir := t.TempDir()
				_ = InstallManagedSkill(dir, ide, "s1", "content")
				err := RemoveManagedSkill(dir, ide, "s1")
				if err != nil {
					t.Fatalf("remove for %s failed: %v", ide, err)
				}
			})
		}
	})

	t.Run("unknown IDE", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := RemoveManagedSkill(dir, "no-such-ide", "s1")
		if err == nil {
			t.Error("expected error for unknown IDE")
		}
	})

	t.Run("nonexistent skill is noop", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := RemoveManagedSkill(dir, "gemini", "does-not-exist")
		if err != nil {
			t.Errorf("expected nil error, got: %v", err)
		}
	})
}

// ---------------------------------------------------------------------------
// blockMarkerForName
// ---------------------------------------------------------------------------

func TestBlockMarkerForName(t *testing.T) {
	t.Parallel()
	got := blockMarkerForName("memory")
	expected := strings.ToUpper(brand.Brand) + " MEMORY BLOCK"
	if got != expected {
		t.Errorf("got %q, want %q", got, expected)
	}
}

// ---------------------------------------------------------------------------
// FolderConfig.allMCPPaths
// ---------------------------------------------------------------------------

func TestFolderConfig_AllMCPPaths(t *testing.T) {
	t.Parallel()

	t.Run("MCPCustomSync returns nil", func(t *testing.T) {
		t.Parallel()
		cfg := FolderConfig{MCPCustomSync: true, MCPFilePath: "~/.config/mcp.json"}
		paths := cfg.allMCPPaths()
		if paths != nil {
			t.Errorf("expected nil, got %v", paths)
		}
	})

	t.Run("normal config", func(t *testing.T) {
		t.Parallel()
		cfg := FolderConfig{
			MCPFilePath:   "~/.config/mcp.json",
			MCPExtraPaths: []string{"~/.extra/mcp.json"},
		}
		paths := cfg.allMCPPaths()
		if len(paths) != 2 {
			t.Errorf("expected 2 paths, got %d", len(paths))
		}
	})

	t.Run("empty MCPFilePath", func(t *testing.T) {
		t.Parallel()
		cfg := FolderConfig{MCPExtraPaths: []string{"extra"}}
		paths := cfg.allMCPPaths()
		if len(paths) != 1 {
			t.Errorf("expected 1 path, got %d", len(paths))
		}
	})
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter internal methods
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_GetTypeDir(t *testing.T) {
	t.Parallel()
	a := NewFolderBasedAdapter(FolderConfig{
		RulesDir:    "rules",
		CommandsDir: "commands",
		SkillsDir:   "skills",
		AgentsDir:   "agents",
	})

	tests := []struct {
		artType string
		want    string
	}{
		{"rule", "rules"},
		{"command", "commands"},
		{"skill", "skills"},
		{"agent", "agents"},
		{"workflow", "commands"},
		{"mcp", ""},
		{"unknown", ""},
	}
	for _, tc := range tests {
		t.Run(tc.artType, func(t *testing.T) {
			t.Parallel()
			got := a.getTypeDir(tc.artType)
			if got != tc.want {
				t.Errorf("getTypeDir(%q) = %q, want %q", tc.artType, got, tc.want)
			}
		})
	}
}

func TestFolderBasedAdapter_GetFileMode(t *testing.T) {
	t.Parallel()
	a := NewFolderBasedAdapter(FolderConfig{})

	tests := []struct {
		artType  string
		wantMode string
		wantExt  string
	}{
		{"rule", "file", "md"},
		{"command", "file", "md"},
		{"skill", "folder", ""},
		{"workflow", "file", "md"},
		{"unknown", "file", "md"},
	}
	for _, tc := range tests {
		t.Run(tc.artType, func(t *testing.T) {
			t.Parallel()
			fm := a.getFileMode(tc.artType)
			if fm.Mode != tc.wantMode {
				t.Errorf("mode=%q, want %q", fm.Mode, tc.wantMode)
			}
			if fm.Ext != tc.wantExt {
				t.Errorf("ext=%q, want %q", fm.Ext, tc.wantExt)
			}
		})
	}
}

func TestFolderBasedAdapter_BaseDir(t *testing.T) {
	t.Parallel()

	t.Run("with root dir", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{RootDirName: ".ide"})
		got := a.baseDir("/project")
		want := filepath.Join("/project", ".ide")
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("empty root dir", func(t *testing.T) {
		t.Parallel()
		a := NewFolderBasedAdapter(FolderConfig{})
		got := a.baseDir("/project")
		if got != "/project" {
			t.Errorf("got %q, want /project", got)
		}
	})
}

// ---------------------------------------------------------------------------
// ClaudeAdapter Sync/Remove
// ---------------------------------------------------------------------------

func TestClaudeAdapter_Sync(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewClaudeAdapter()

	srcRule := filepath.Join(dir, "src", "rule1")
	_ = os.MkdirAll(srcRule, 0o755)
	_ = os.WriteFile(filepath.Join(srcRule, "RULE.md"), []byte("claude rule"), 0o644)

	installed := map[string]map[string]string{
		"rule1": {"type": "rule", "path": srcRule},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// Verify CLAUDE.md was created with managed block
	claudeMD := filepath.Join(dir, "CLAUDE.md")
	data, err := os.ReadFile(claudeMD)
	if err != nil {
		t.Fatalf("CLAUDE.md not created: %v", err)
	}
	if !strings.Contains(string(data), "@AGENTS.md") {
		t.Error("expected @AGENTS.md reference in CLAUDE.md")
	}
}

func TestClaudeAdapter_Remove(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewClaudeAdapter()

	// Set up CLAUDE.md with managed block
	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	// Now remove
	if err := a.Remove(pp, map[string]map[string]string{}); err != nil {
		t.Fatal(err)
	}

	claudeMD := filepath.Join(dir, "CLAUDE.md")
	if _, err := os.Stat(claudeMD); err == nil {
		data, _ := os.ReadFile(claudeMD)
		content := string(data)
		if strings.Contains(content, "@AGENTS.md") {
			t.Error("expected managed block to be removed from CLAUDE.md")
		}
	}
}

// ---------------------------------------------------------------------------
// CodexAdapter Sync/Remove
// ---------------------------------------------------------------------------

func TestCodexAdapter_Sync(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewCodexAdapter()

	// Override MCPFilePath to a temp location
	mcpTarget := filepath.Join(dir, "codex-config.toml")
	a.cfg.MCPFilePath = mcpTarget

	srcRule := filepath.Join(dir, "src", "rule1")
	_ = os.MkdirAll(srcRule, 0o755)
	_ = os.WriteFile(filepath.Join(srcRule, "RULE.md"), []byte("codex rule"), 0o644)

	installed := map[string]map[string]string{
		"rule1": {"type": "rule", "path": srcRule},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// Verify TOML config was created
	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		t.Fatalf("codex config not created: %v", err)
	}
	content := string(data)
	coreKey := brand.MCPServerName("code-stdio")
	if !strings.Contains(content, coreKey) {
		t.Errorf("expected %q in TOML config", coreKey)
	}
}

func TestCodexAdapter_Remove(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewCodexAdapter()

	mcpTarget := filepath.Join(dir, "codex-config.toml")
	a.cfg.MCPFilePath = mcpTarget

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	if err := a.Remove(pp, map[string]map[string]string{}); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		t.Fatalf("config file should still exist: %v", err)
	}
	coreKey := brand.MCPServerName("code-stdio")
	if strings.Contains(string(data), coreKey) {
		t.Error("expected core server to be removed from TOML")
	}
}

// ---------------------------------------------------------------------------
// OpenCodeAdapter Sync/Remove
// ---------------------------------------------------------------------------

func TestOpenCodeAdapter_Sync(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewOpenCodeAdapter()

	mcpTarget := filepath.Join(dir, "opencode.json")
	a.cfg.MCPFilePath = mcpTarget

	installed := map[string]map[string]string{}
	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(mcpTarget)
	if err != nil {
		t.Fatalf("opencode config not created: %v", err)
	}

	var parsed map[string]any
	_ = json.Unmarshal(data, &parsed)

	// OpenCode writes both "mcp" and "mcpServers"
	if parsed["mcp"] == nil {
		t.Error("expected 'mcp' key in config")
	}
	if parsed["mcpServers"] == nil {
		t.Error("expected 'mcpServers' key in config")
	}
}

func TestOpenCodeAdapter_Remove(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewOpenCodeAdapter()

	mcpTarget := filepath.Join(dir, "opencode.json")
	a.cfg.MCPFilePath = mcpTarget

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	if err := a.Remove(pp, map[string]map[string]string{}); err != nil {
		t.Fatal(err)
	}

	data, _ := os.ReadFile(mcpTarget)
	var parsed map[string]any
	_ = json.Unmarshal(data, &parsed)

	if parsed["mcp"] != nil {
		t.Error("expected 'mcp' key to be removed")
	}
	if parsed["mcpServers"] != nil {
		t.Error("expected 'mcpServers' key to be removed")
	}
}

func TestOpenCodeAdapter_Remove_FileNotExist(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewOpenCodeAdapter()
	a.cfg.MCPFilePath = filepath.Join(dir, "nonexistent.json")

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	err := a.Remove(pp, map[string]map[string]string{})
	if err != nil {
		t.Errorf("expected nil error for nonexistent file, got: %v", err)
	}
}

func TestOpenCodeAdapter_Remove_PreservesOtherKeys(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewOpenCodeAdapter()

	mcpTarget := filepath.Join(dir, "opencode.json")
	a.cfg.MCPFilePath = mcpTarget

	// Write a config with extra user keys in "mcp" and "mcpServers"
	existing := map[string]any{
		"mcp": map[string]any{
			"user-mcp": map[string]any{"type": "local"},
		},
		"mcpServers": map[string]any{
			"user-server": map[string]any{"command": "echo"},
		},
	}
	data, _ := json.Marshal(existing)
	_ = os.WriteFile(mcpTarget, data, 0o644)

	// Sync to add the core server
	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	// Remove should only remove the core server, preserving user entries
	_ = a.Remove(pp, map[string]map[string]string{})

	data, _ = os.ReadFile(mcpTarget)
	var parsed map[string]any
	_ = json.Unmarshal(data, &parsed)

	mcp, _ := parsed["mcp"].(map[string]any)
	if mcp == nil || mcp["user-mcp"] == nil {
		t.Error("expected user-mcp to be preserved")
	}

	servers, _ := parsed["mcpServers"].(map[string]any)
	if servers == nil || servers["user-server"] == nil {
		t.Error("expected user-server to be preserved")
	}
}

// ---------------------------------------------------------------------------
// GeminiAdapter Sync/Remove
// ---------------------------------------------------------------------------

func TestGeminiAdapter_Sync(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewGeminiAdapter()

	// Create rules that syncGeminiMD will discover
	rulesDir := filepath.Join(dir, ".gemini", "rules")
	_ = os.MkdirAll(rulesDir, 0o755)
	_ = os.WriteFile(filepath.Join(rulesDir, "test-rule.md"), []byte("rule"), 0o644)

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.FolderBasedAdapter.Sync(map[string]map[string]string{}, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// Now run the full Gemini sync (which includes syncGeminiMD)
	if err := a.Sync(map[string]map[string]string{}, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// AGENTS.md should contain reference to the rule
	agentsMD := filepath.Join(dir, "AGENTS.md")
	data, err := os.ReadFile(agentsMD)
	if err != nil {
		t.Fatalf("AGENTS.md not created: %v", err)
	}
	if !strings.Contains(string(data), "@.gemini/rules/test-rule.md") {
		t.Error("expected @.gemini/rules/test-rule.md in AGENTS.md")
	}
}

func TestGeminiAdapter_Sync_NoRules(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewGeminiAdapter()

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(map[string]map[string]string{}, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// With no rules, AGENTS.md should not contain a managed block
	// (syncGeminiMD returns nil when ReadDir fails)
}

func TestGeminiAdapter_Remove(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewGeminiAdapter()

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}

	// Create rules so sync will inject a managed block
	rulesDir := filepath.Join(dir, ".gemini", "rules")
	_ = os.MkdirAll(rulesDir, 0o755)
	_ = os.WriteFile(filepath.Join(rulesDir, "r1.md"), []byte("rule"), 0o644)
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	// Now remove
	if err := a.Remove(pp, map[string]map[string]string{}); err != nil {
		t.Fatal(err)
	}

	agentsMD := filepath.Join(dir, "AGENTS.md")
	if _, err := os.Stat(agentsMD); err == nil {
		data, _ := os.ReadFile(agentsMD)
		if strings.Contains(string(data), "@.gemini/rules/r1.md") {
			t.Error("expected managed block to be removed from AGENTS.md")
		}
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.Sync with agent into rules dir (no AgentsDir, no AgentsFile)
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_Sync_AgentToRulesDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		RulesDir:    "rules",
	})

	srcAgent := filepath.Join(dir, "src-agent", "agent1")
	_ = os.MkdirAll(srcAgent, 0o755)
	_ = os.WriteFile(filepath.Join(srcAgent, "AGENT.md"), []byte("agent stuff"), 0o644)

	installed := map[string]map[string]string{
		"agent1": {"type": "agent", "path": srcAgent},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// Agent should be copied as agent1_agent.md into rules dir
	agentFile := filepath.Join(dir, ".test-ide", "rules", "agent1_agent.md")
	if _, err := os.Stat(agentFile); err != nil {
		t.Errorf("agent file not created in rules dir: %v", err)
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.Sync with MCP artifact
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_Sync_MCPArtifact(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mcpTarget := filepath.Join(dir, "mcp-config.json")

	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		MCPFilePath: mcpTarget,
	})

	// Create an MCP artifact with a mcp.json
	srcMCP := filepath.Join(dir, "src-mcp", "mcp1")
	_ = os.MkdirAll(srcMCP, 0o755)
	mcpConf := map[string]any{
		"custom-server": map[string]any{"command": "echo"},
	}
	data, _ := json.Marshal(mcpConf)
	_ = os.WriteFile(filepath.Join(srcMCP, "mcp.json"), data, 0o644)

	installed := map[string]map[string]string{
		"mcp1": {"type": "mcp", "path": srcMCP},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}

	// Verify MCP config was written
	result, err := os.ReadFile(mcpTarget)
	if err != nil {
		t.Fatalf("mcp config not created: %v", err)
	}

	var parsed map[string]any
	_ = json.Unmarshal(result, &parsed)

	mcpServers, _ := parsed["mcpServers"].(map[string]any)
	if mcpServers == nil || mcpServers["custom-server"] == nil {
		t.Error("expected custom-server in mcpServers")
	}
}

// ---------------------------------------------------------------------------
// getGraphitExecutable
// ---------------------------------------------------------------------------

func TestGetGraphitExecutable(t *testing.T) {
	t.Parallel()
	// Just verify it returns a non-empty string
	exe := getGraphitExecutable()
	if exe == "" {
		t.Error("expected non-empty executable path")
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.findCanonicalSource
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_FindCanonicalSource(t *testing.T) {
	t.Parallel()
	a := NewFolderBasedAdapter(FolderConfig{})

	t.Run("canonical file exists", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "RULE.md"), []byte("rule"), 0o644)
		got := a.findCanonicalSource("rule", dir)
		if filepath.Base(got) != "RULE.md" {
			t.Errorf("expected RULE.md, got %q", got)
		}
	})

	t.Run("fallback to first file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "custom.md"), []byte("rule"), 0o644)
		got := a.findCanonicalSource("rule", dir)
		if got == "" {
			t.Error("expected fallback to first file")
		}
	})

	t.Run("empty dir", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		got := a.findCanonicalSource("rule", dir)
		if got != "" {
			t.Errorf("expected empty for empty dir, got %q", got)
		}
	})

	t.Run("only directories", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.MkdirAll(filepath.Join(dir, "subdir"), 0o755)
		got := a.findCanonicalSource("rule", dir)
		if got != "" {
			t.Errorf("expected empty when only dirs, got %q", got)
		}
	})

	t.Run("nonexistent dir", func(t *testing.T) {
		t.Parallel()
		got := a.findCanonicalSource("rule", "/nonexistent/dir")
		if got != "" {
			t.Errorf("expected empty, got %q", got)
		}
	})
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.copyArtifact
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_CopyArtifact(t *testing.T) {
	t.Parallel()
	a := NewFolderBasedAdapter(FolderConfig{})

	t.Run("file mode", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		srcDir := filepath.Join(dir, "src")
		_ = os.MkdirAll(srcDir, 0o755)
		_ = os.WriteFile(filepath.Join(srcDir, "RULE.md"), []byte("content"), 0o644)

		dstDir := filepath.Join(dir, "dst")
		_ = os.MkdirAll(dstDir, 0o755)

		err := a.copyArtifact("rule", srcDir, dstDir, "my-rule")
		if err != nil {
			t.Fatal(err)
		}

		data, _ := os.ReadFile(filepath.Join(dstDir, "my-rule.md"))
		if string(data) != "content" {
			t.Errorf("content = %q, want 'content'", string(data))
		}
	})

	t.Run("folder mode", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		srcDir := filepath.Join(dir, "src")
		_ = os.MkdirAll(srcDir, 0o755)
		_ = os.WriteFile(filepath.Join(srcDir, "SKILL.md"), []byte("skill"), 0o644)

		dstDir := filepath.Join(dir, "dst")
		_ = os.MkdirAll(dstDir, 0o755)

		err := a.copyArtifact("skill", srcDir, dstDir, "my-skill")
		if err != nil {
			t.Fatal(err)
		}

		if _, err := os.Stat(filepath.Join(dstDir, "my-skill", "SKILL.md")); err != nil {
			t.Error("skill dir/file not copied")
		}
	})

	t.Run("source not found", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := a.copyArtifact("rule", filepath.Join(dir, "nonexistent"), dir, "x")
		if err == nil {
			t.Error("expected error for nonexistent source")
		}
	})

	t.Run("no canonical source", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		srcDir := filepath.Join(dir, "src")
		_ = os.MkdirAll(srcDir, 0o755)
		// only a subdirectory, no files
		_ = os.MkdirAll(filepath.Join(srcDir, "subdir"), 0o755)

		err := a.copyArtifact("rule", srcDir, dir, "x")
		if err == nil {
			t.Error("expected error when no canonical source found")
		}
	})
}

// ---------------------------------------------------------------------------
// CodexAdapter.removeCodexMCP edge cases
// ---------------------------------------------------------------------------

func TestCodexAdapter_RemoveCodexMCP_FileNotExist(t *testing.T) {
	t.Parallel()
	a := NewCodexAdapter()
	err := a.removeCodexMCP(filepath.Join(t.TempDir(), "nonexistent.toml"))
	if err != nil {
		t.Errorf("expected nil error for nonexistent file, got: %v", err)
	}
}

func TestCodexAdapter_RemoveCodexMCP_InvalidTOML(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	target := filepath.Join(dir, "config.toml")
	_ = os.WriteFile(target, []byte("not valid toml [[["), 0o644)

	a := NewCodexAdapter()
	err := a.removeCodexMCP(target)
	if err != nil {
		t.Errorf("expected nil error for invalid TOML, got: %v", err)
	}
}

func TestCodexAdapter_RemoveCodexMCP_EmptyServersDeleted(t *testing.T) {
	t.Parallel()
	a := NewCodexAdapter()
	dir := t.TempDir()
	target := filepath.Join(dir, "config.toml")

	a.cfg.MCPFilePath = target
	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	_ = a.Sync(map[string]map[string]string{}, pp, "proj-1")

	// Remove → mcp_servers section should be removed entirely
	_ = a.removeCodexMCP(target)
	data, _ := os.ReadFile(target)
	if strings.Contains(string(data), "mcp_servers") {
		t.Error("expected mcp_servers section to be deleted when empty")
	}
}

// ---------------------------------------------------------------------------
// FolderBasedAdapter.Sync skips artifacts with missing type dir
// ---------------------------------------------------------------------------

func TestFolderBasedAdapter_Sync_SkipsMissingTypeDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	// SkillsDir empty means skills are skipped
	a := NewFolderBasedAdapter(FolderConfig{
		RootDirName: ".test-ide",
		RulesDir:    "rules",
		SkillsDir:   "",
	})

	srcSkill := filepath.Join(dir, "src", "s1")
	_ = os.MkdirAll(srcSkill, 0o755)
	_ = os.WriteFile(filepath.Join(srcSkill, "SKILL.md"), []byte("skill"), 0o644)

	installed := map[string]map[string]string{
		"s1": {"type": "skill", "path": srcSkill},
	}

	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	// Should not error
	if err := a.Sync(installed, pp, "proj-1"); err != nil {
		t.Fatal(err)
	}
}

// ---------------------------------------------------------------------------
// Adapter interface compliance
// ---------------------------------------------------------------------------

func TestAdapterInterfaceCompliance(t *testing.T) {
	t.Parallel()
	var _ Adapter = NewAntigravityAdapter()
	var _ Adapter = NewCursorAdapter()
	var _ Adapter = NewClaudeAdapter()
	var _ Adapter = NewKiroAdapter()
	var _ Adapter = NewCodexAdapter()
	var _ Adapter = NewOpenCodeAdapter()
	var _ Adapter = NewGeminiAdapter()
}
