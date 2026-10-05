//go:build lancedb

package mcpstdio

import (
	"context"
	"encoding/json"
	"github.com/graphit-labs/graphit-code/internal/gitenv"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func gitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	cmd.Env = append(gitenv.WithoutInvocationScope(os.Environ()), "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@example.com",
		"GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@example.com",
		"GIT_CONFIG_GLOBAL="+os.DevNull, "GIT_CONFIG_SYSTEM="+os.DevNull)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

func writeProjectFile(t *testing.T, dir, rel, content string) {
	t.Helper()
	path := filepath.Join(dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// toolText calls an MCP tool and returns its text content, failing on any tool error.
// Tools that offer the compact format are asked for JSON.
func toolText(t *testing.T, c *mcp.ClientSession, module, action string, args map[string]any) string {
	t.Helper()
	if module+"_"+action != "memory_insert" {
		args["ai_optimized"] = false
	}
	result, err := c.CallTool(context.Background(), &mcp.CallToolParams{Name: brand.MCPToolName(module, action), Arguments: args})
	if err != nil {
		t.Fatalf("%s_%s: %v", module, action, err)
	}
	var text strings.Builder
	for _, content := range result.Content {
		if tc, ok := content.(*mcp.TextContent); ok {
			text.WriteString(tc.Text)
		}
	}
	if result.IsError {
		t.Fatalf("%s_%s failed: %s", module, action, text.String())
	}
	return text.String()
}

// Two worktrees of one project, on main and on feature/a, each read and write their own
// AST graph, documentation wiki, project memory, tasks and sessions through the MCP
// server. The feature version starts from what main held when it first wrote, the two
// diverge afterwards, user memory stays shared, and project_version reaches the other
// version from either checkout.
func TestWorktreesOnDifferentBranchesKeepEveryModuleApart(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git is not installed")
	}
	isolateHome(t)
	t.Setenv("LADYBUGDB_PATH", "")
	t.Setenv(brand.EnvVar("PROJECT_VERSION"), "")
	t.Setenv(brand.EnvVar("GIT_BASE_BRANCH"), "")

	goQueries, err := os.ReadFile(filepath.Join("..", "ast", "queries", "go.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	mainDir := t.TempDir()
	writeProjectFile(t, mainDir, brand.LockFileName(), `{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`)
	writeProjectFile(t, mainDir, brand.DotDir()+"/ast/queries/go.yaml", string(goQueries))
	writeProjectFile(t, mainDir, "base.go", "package demo\n\nfunc InheritedBaseline() {}\n")
	writeProjectFile(t, mainDir, "docs/baseline.md", "# Baseline guide\n\nThe quarzbaseline contract.\n")
	gitIn(t, mainDir, "init", "-q", "-b", "main")
	gitIn(t, mainDir, "add", ".")
	gitIn(t, mainDir, "commit", "-q", "-m", "init")
	featureDir := filepath.Join(t.TempDir(), "feature")
	gitIn(t, mainDir, "worktree", "add", "-q", "-b", "feature/a", featureDir)

	c := testMCPClient(t)
	project := func(dir string, fields map[string]any) map[string]any {
		args := map[string]any{"project_dir": dir}
		for k, v := range fields {
			args[k] = v
		}
		return args
	}
	sessionAndTask := func(dir, actor, title string) {
		t.Helper()
		session := toolText(t, c, "task", "session_create", project(dir, map[string]any{
			"agent_id": actor, "title": title + " session", "idempotency_key": title + "-session",
			"description": "Integrated project version check for " + title + ".", "strategy": "Write, then compare versions.",
		}))
		var created struct {
			ID string `json:"id"`
		}
		if err := json.Unmarshal([]byte(session), &created); err != nil || created.ID == "" {
			t.Fatalf("session_create returned %q: %v", session, err)
		}
		id := created.ID
		toolText(t, c, "task", "session_claim", project(dir, map[string]any{"agent_id": actor, "id": id}))
		toolText(t, c, "task", "create", project(dir, map[string]any{
			"agent_id": actor, "title": title, "description": "Task written in one project version.",
			"acceptance_criteria": []string{"Visible only where expected."}, "tests": []string{"Listed per version."},
			"idempotency_key": title,
		}))
	}
	remember := func(dir, title string) {
		t.Helper()
		toolText(t, c, "memory", "insert", project(dir, map[string]any{"title": title, "content": title + " is a fact of this version.", "type": "fact"}))
	}

	// Main writes before the feature version exists in any store.
	toolText(t, c, "ast", "index", project(mainDir, nil))
	toolText(t, c, "knowledge", "index", project(mainDir, nil))
	remember(mainDir, "inheritedmemory")
	sessionAndTask(mainDir, "main-agent", "inheritedtask")
	toolText(t, c, "memory", "insert", map[string]any{"title": "shareduserpreference", "content": "A preference of the engineer.", "scope": "user"})

	// The feature worktree changes code and docs and writes its own records.
	writeProjectFile(t, featureDir, "feature.go", "package demo\n\nfunc OnlyOnFeature() {}\n")
	writeProjectFile(t, featureDir, "docs/feature.md", "# Feature guide\n\nThe zyxfeature contract.\n")
	toolText(t, c, "ast", "index", project(featureDir, nil))
	toolText(t, c, "knowledge", "index", project(featureDir, nil))
	remember(featureDir, "featurememory")
	sessionAndTask(featureDir, "feature-agent", "featuretask")

	// Main keeps writing after the fork.
	writeProjectFile(t, mainDir, "later.go", "package demo\n\nfunc OnlyOnMainLater() {}\n")
	writeProjectFile(t, mainDir, "docs/later.md", "# Later guide\n\nThe wqmainlater contract.\n")
	toolText(t, c, "ast", "index", project(mainDir, nil))
	toolText(t, c, "knowledge", "index", project(mainDir, nil))
	remember(mainDir, "mainlatermemory")
	sessionAndTask(mainDir, "main-later-agent", "mainlatertask")

	views := map[string]func(dir string, version string) string{
		"ast": func(dir, version string) string {
			return toolText(t, c, "ast", "query", project(dir, map[string]any{
				"project_version": version, "query": "MATCH (f:Function) RETURN f.name LIMIT 50",
			}))
		},
		"knowledge": func(dir, version string) string {
			return toolText(t, c, "wiki", "browse", project(dir, map[string]any{"project_version": version}))
		},
		"memory": func(dir, version string) string {
			return toolText(t, c, "memory", "list", project(dir, map[string]any{"project_version": version}))
		},
		"task": func(dir, version string) string {
			return toolText(t, c, "task", "list", project(dir, map[string]any{"project_version": version}))
		},
		"session": func(dir, version string) string {
			return toolText(t, c, "task", "session_list", project(dir, map[string]any{"project_version": version}))
		},
	}
	expect := map[string]struct{ inherited, featureOnly, mainOnly string }{
		"ast":       {"InheritedBaseline", "OnlyOnFeature", "OnlyOnMainLater"},
		"knowledge": {"Baseline guide", "Feature guide", "Later guide"},
		"memory":    {"inheritedmemory", "featurememory", "mainlatermemory"},
		"task":      {"inheritedtask", "featuretask", "mainlatertask"},
		"session":   {"inheritedtask session", "featuretask session", "mainlatertask session"},
	}
	check := func(label, module, got string, wantFeature bool) {
		t.Helper()
		e := expect[module]
		if !strings.Contains(got, e.inherited) {
			t.Errorf("%s %s lost the inherited %q:\n%s", label, module, e.inherited, got)
		}
		if strings.Contains(got, e.featureOnly) != wantFeature {
			t.Errorf("%s %s shows %q = %v, want %v:\n%s", label, module, e.featureOnly, !wantFeature, wantFeature, got)
		}
		if strings.Contains(got, e.mainOnly) == wantFeature {
			t.Errorf("%s %s shows %q = %v, want %v:\n%s", label, module, e.mainOnly, wantFeature, !wantFeature, got)
		}
	}
	for module, view := range views {
		check("main checkout", module, view(mainDir, ""), false)
		check("feature checkout", module, view(featureDir, ""), true)
		check("main checkout asking for feature/a", module, view(mainDir, "feature/a"), true)
		check("feature checkout asking for main", module, view(featureDir, "main"), false)
	}

	for _, dir := range []string{mainDir, featureDir} {
		if got := toolText(t, c, "memory", "list", map[string]any{"project_dir": dir, "scope": "user"}); !strings.Contains(got, "shareduserpreference") {
			t.Errorf("user memory is not shared by the checkout at %s:\n%s", dir, got)
		}
	}
}
