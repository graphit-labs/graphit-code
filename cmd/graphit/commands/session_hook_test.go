package commands

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

func TestSessionHookCommandRendersFormatPayload(t *testing.T) {
	t.Parallel()

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "additional-context"})
	cmd.SetIn(strings.NewReader("{}"))
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), `"additional_context"`) || !strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("unexpected hook output: %s", output.String())
	}
}

func TestSessionHookLoadsEnabledMandatesAndInstalledHubRulesDynamically(t *testing.T) {
	projectDir := t.TempDir()
	ruleDir := filepath.Join(t.TempDir(), "rule")
	if err := os.MkdirAll(ruleDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ruleDir, "RULE.md"), []byte("DYNAMIC HUB RULE"), 0o644); err != nil {
		t.Fatal(err)
	}
	lf := &hub.Lockfile{
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{
			hub.TypeRule: {"dynamic-rule": {LinkSource: ruleDir}},
		},
		Config: map[string]any{"modules": map[string]any{"ast": "false"}},
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "additional-context", "--project-dir", projectDir})
	cmd.SetIn(strings.NewReader("{}"))
	var output bytes.Buffer
	cmd.SetOut(&output)
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	got := output.String()
	for _, want := range []string{"GRAPHIT_SYSTEM_MANDATE", "graphit-memory", "DYNAMIC HUB RULE"} {
		if !strings.Contains(got, want) {
			t.Fatalf("dynamic hook context missing %q: %s", want, got)
		}
	}
	if strings.Contains(got, "graphit-ast`") {
		t.Fatalf("disabled AST mandate was injected: %s", got)
	}
	if _, err := os.Stat(filepath.Join(projectDir, "AGENTS.md")); !os.IsNotExist(err) {
		t.Fatalf("hook materialized AGENTS.md: %v", err)
	}
}

func TestSessionHookOmitsMemoryBootstrapWhenModuleIsDisabled(t *testing.T) {
	projectDir := t.TempDir()
	lf := &hub.Lockfile{
		Artifacts: map[hub.ArtifactType]map[string]*hub.LockfileArtifactMeta{},
		Config:    map[string]any{"modules": map[string]any{"memory": "false"}},
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), lf); err != nil {
		t.Fatal(err)
	}

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "additional-context", "--project-dir", projectDir})
	cmd.SetIn(strings.NewReader("{}"))
	var output bytes.Buffer
	cmd.SetOut(&output)
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	got := output.String()
	if strings.Contains(got, "graphit_memory_") || strings.Contains(got, "Graphit session bootstrap") || strings.Contains(got, "graphit-memory`") {
		t.Fatalf("disabled memory module leaked into hook context: %s", got)
	}
}

func TestSessionHookLoadsProjectMandateOverrideFromNativeCWD(t *testing.T) {
	projectDir := t.TempDir()
	workingDir := filepath.Join(projectDir, "packages", "api")
	if err := os.MkdirAll(workingDir, 0o755); err != nil {
		t.Fatal(err)
	}
	rulesDir := filepath.Join(projectDir, brand.DotDir(), "rules")
	if err := os.MkdirAll(rulesDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rulesDir, "ast.md"), []byte("PINNED AST MANDATE"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), &hub.Lockfile{}); err != nil {
		t.Fatal(err)
	}

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "additional-context"})
	cmd.SetIn(strings.NewReader(`{"cwd":` + strconv.Quote(workingDir) + `}`))
	var output bytes.Buffer
	cmd.SetOut(&output)
	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "PINNED AST MANDATE") {
		t.Fatalf("hook ignored the mandate override resolved from native cwd: %s", output.String())
	}
}

func TestResolveSessionHookProjectDirFromNativeHostInputsWithoutGit(t *testing.T) {
	projectDir := t.TempDir()
	workingDir := filepath.Join(projectDir, "packages", "api")
	if err := os.MkdirAll(workingDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), &hub.Lockfile{}); err != nil {
		t.Fatal(err)
	}

	tests := map[string]string{
		"cwd":               `{"cwd":` + strconv.Quote(workingDir) + `}`,
		"cursor roots":      `{"workspace_roots":[` + strconv.Quote(workingDir) + `]}`,
		"antigravity paths": `{"workspacePaths":[` + strconv.Quote(workingDir) + `]}`,
	}
	for name, input := range tests {
		t.Run(name, func(t *testing.T) {
			if got := resolveSessionHookProjectDir("", []byte(input)); got != projectDir {
				t.Fatalf("resolved project = %q, want %q", got, projectDir)
			}
		})
	}

	unrelatedGitRoot := t.TempDir()
	if err := os.MkdirAll(filepath.Join(unrelatedGitRoot, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	multiRootInput := `{"workspace_roots":[` + strconv.Quote(unrelatedGitRoot) + `,` + strconv.Quote(workingDir) + `]}`
	if got := resolveSessionHookProjectDir("", []byte(multiRootInput)); got != projectDir {
		t.Fatalf("multi-root project = %q, want Graphit root %q", got, projectDir)
	}

	otherProject := t.TempDir()
	if err := hub.SaveLockfile(filepath.Join(otherProject, brand.LockFileName()), &hub.Lockfile{}); err != nil {
		t.Fatal(err)
	}
	if got := resolveSessionHookProjectDir(otherProject, []byte(`{"cwd":`+strconv.Quote(workingDir)+`}`)); got != otherProject {
		t.Fatalf("explicit diagnostic project = %q, want %q", got, otherProject)
	}
}

func TestResolveSessionHookProjectDirFromProcessCWD(t *testing.T) {
	projectDir := t.TempDir()
	workingDir := filepath.Join(projectDir, "packages", "api")
	if err := os.MkdirAll(workingDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := hub.SaveLockfile(filepath.Join(projectDir, brand.LockFileName()), &hub.Lockfile{}); err != nil {
		t.Fatal(err)
	}
	t.Chdir(workingDir)

	if got := resolveSessionHookProjectDir("", nil); got != projectDir {
		t.Fatalf("resolved project = %q, want %q", got, projectDir)
	}
}

func TestResolveSessionHookProjectDirRequiresGraphitLockfile(t *testing.T) {
	gitOnlyRoot := t.TempDir()
	workingDir := filepath.Join(gitOnlyRoot, "packages", "api")
	if err := os.MkdirAll(filepath.Join(gitOnlyRoot, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(workingDir, 0o755); err != nil {
		t.Fatal(err)
	}
	t.Chdir(workingDir)

	input := []byte(`{"cwd":` + strconv.Quote(workingDir) + `}`)
	if got := resolveSessionHookProjectDir("", input); got != "" {
		t.Fatalf("resolved project = %q without %s; .git must not define a Graphit project", got, brand.LockFileName())
	}
	if got := resolveSessionHookProjectDir(gitOnlyRoot, nil); got != "" {
		t.Fatalf("explicit start resolved project = %q without %s", got, brand.LockFileName())
	}
}

func TestLoadMandatoryHookContextReadsBothAuthoritativeScopes(t *testing.T) {
	t.Parallel()

	context, loaded := loadMandatoryHookContextWith("/project", func(projectDir, scope string) ([]memory.MandatoryEntry, error) {
		if projectDir != "/project" {
			t.Fatalf("project dir = %q", projectDir)
		}
		return []memory.MandatoryEntry{{Title: scope + " policy", Content: "content for " + scope}}, nil
	})
	if !loaded || !strings.Contains(context, "### project memory: project policy") || !strings.Contains(context, "### user memory: user policy") {
		t.Fatalf("mandatory scopes were not rendered: loaded=%v context=%q", loaded, context)
	}
}

func TestLoadMandatoryHookContextFallsBackWhenAStoreCannotOpen(t *testing.T) {
	t.Parallel()

	_, loaded := loadMandatoryHookContextWith("/project", func(string, string) ([]memory.MandatoryEntry, error) {
		return nil, errors.New("store unavailable")
	})
	if loaded {
		t.Fatal("store failure must preserve the MCP fallback")
	}
}

func TestSessionHookCommandFirstInvocationDoesNotWaitForCharacterDeviceInput(t *testing.T) {
	input, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = input.Close() })

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "first-invocation"})
	cmd.SetIn(input)
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), `"injectSteps"`) || !strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("unexpected first-invocation output: %s", output.String())
	}
}

func TestSessionHookCommandFirstInvocationReadsPipedPayload(t *testing.T) {
	t.Parallel()

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "first-invocation"})
	cmd.SetIn(strings.NewReader(`{"invocationNum":1}`))
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "GRAPHIT_SYSTEM_MANDATE") || strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("non-first invocation did not contain only resident instructions: %s", output.String())
	}
}

func TestHookInputNeedsMandatoryOnlyOnFirstInvocation(t *testing.T) {
	t.Parallel()

	if !hookInputNeedsMandatory("first-invocation", []byte(`{"invocationNum":0}`)) {
		t.Fatal("first invocation must load mandatory memory")
	}
	if hookInputNeedsMandatory("first-invocation", []byte(`{"invocationNum":1}`)) {
		t.Fatal("later invocation must not reopen mandatory memory")
	}
	if !hookInputNeedsMandatory("cursor-subagent-task", nil) {
		t.Fatal("Cursor subagent task injection must carry authoritative mandatory memory")
	}
}
