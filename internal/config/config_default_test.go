package config

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestIsModuleDisabled(t *testing.T) {
	if IsModuleDisabled("ast", nil, nil) {
		t.Error("expected AST module to be enabled by default")
	}

	cfgTrue := ConfigMap{
		"modules": map[string]any{
			"ast": "true",
		},
	}
	if IsModuleDisabled("ast", nil, cfgTrue) {
		t.Error("expected AST module to be enabled when explicitly configured to true")
	}

	cfgFalse := ConfigMap{
		"modules": map[string]any{
			"ast": "false",
		},
	}
	if !IsModuleDisabled("ast", nil, cfgFalse) {
		t.Error("expected AST module to be disabled when explicitly configured to false")
	}
}

func TestDaemonSyncModuleSwitch(t *testing.T) {
	if IsModuleDisabled("sync", nil, nil) {
		t.Fatal("daemon filesystem sync should be enabled by default")
	}

	for value, wantDisabled := range map[string]bool{"false": true, "true": false} {
		cfg := ConfigMap{"modules": map[string]any{"sync": value}}
		if got := IsModuleDisabled("sync", nil, cfg); got != wantDisabled {
			t.Errorf("modules.sync=%s: disabled=%v, want %v", value, got, wantDisabled)
		}
	}
}

func TestConfigCRUD(t *testing.T) {
	cfg := make(ConfigMap)

	SetConfigValue(cfg, "foo", "bar")
	val, ok := GetConfigValue(cfg, "foo")
	if !ok || val != "bar" {
		t.Errorf("expected foo=bar, got %q (ok=%t)", val, ok)
	}

	SetConfigValue(cfg, "nested.key", "value")
	val, ok = GetConfigValue(cfg, "nested.key")
	if !ok || val != "value" {
		t.Errorf("expected nested.key=value, got %q (ok=%t)", val, ok)
	}

	cfg["nested_bad"] = "not_a_map"
	SetConfigValue(cfg, "nested_bad.key", "new_val")
	val, ok = GetConfigValue(cfg, "nested_bad.key")
	if !ok || val != "new_val" {
		t.Errorf("expected nested_bad.key=new_val, got %q (ok=%t)", val, ok)
	}

	cfg["non_string"] = 123
	val, ok = GetConfigValue(cfg, "non_string")
	if ok || val != "" {
		t.Errorf("expected non_string to fail, got %q (ok=%t)", val, ok)
	}

	cfg["nested_non_string"] = map[string]any{"key": 123}
	val, ok = GetConfigValue(cfg, "nested_non_string.key")
	if ok || val != "" {
		t.Errorf("expected nested_non_string.key to fail, got %q (ok=%t)", val, ok)
	}

	entries := ListConfigEntries(cfg)
	foundFoo := false
	for _, entry := range entries {
		if entry[0] == "foo" && entry[1] == "bar" {
			foundFoo = true
		}
	}
	if !foundFoo {
		t.Errorf("expected foo=bar in listed entries, entries: %v", entries)
	}

	UnsetConfigValue(cfg, "foo")
	_, ok = GetConfigValue(cfg, "foo")
	if ok {
		t.Error("expected foo to be unset")
	}

	UnsetConfigValue(cfg, "nested.key")
	_, ok = GetConfigValue(cfg, "nested.key")
	if ok {
		t.Error("expected nested.key to be unset")
	}
}

func TestResolveConfig(t *testing.T) {
	origCompiledDefaults := CompiledDefaults
	defer func() { CompiledDefaults = origCompiledDefaults }()
	CompiledDefaults = "default.key=default_val,other.key=other_val"

	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	val := ResolveConfig("default.key", nil, nil)
	if val != "default_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "default_val")
	}

	projectCfg := ConfigMap{
		"default": map[string]any{
			"key": "project_val",
		},
	}
	val = ResolveConfig("default.key", nil, projectCfg)
	if val != "project_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "project_val")
	}

	origEnv := os.Getenv("GRAPHIT_DEFAULT_KEY")
	defer func() { _ = os.Setenv("GRAPHIT_DEFAULT_KEY", origEnv) }()
	_ = os.Setenv("GRAPHIT_DEFAULT_KEY", "env_val")

	val = ResolveConfig("default.key", nil, projectCfg)
	if val != "env_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "env_val")
	}

	inlineCfg := ConfigMap{
		"default": map[string]any{
			"key": "inline_val",
		},
	}
	val = ResolveConfig("default.key", inlineCfg, projectCfg)
	if val != "inline_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "inline_val")
	}
}

func TestResolveAgentAndCLI(t *testing.T) {
	origBrand := brand.Brand
	brand.Brand = "graphit"
	defer func() { brand.Brand = origBrand }()

	origHome := os.Getenv("HOME")
	tempDir, err := os.MkdirTemp("", "config-agent-cli-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()
	_ = os.Setenv("HOME", tempDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	origEnvAgent := os.Getenv("GRAPHIT_AGENT")
	origEnvCLI := os.Getenv("GRAPHIT_CLI")
	defer func() {
		_ = os.Setenv("GRAPHIT_AGENT", origEnvAgent)
		_ = os.Setenv("GRAPHIT_CLI", origEnvCLI)
	}()
	_ = os.Unsetenv("GRAPHIT_AGENT")
	_ = os.Unsetenv("GRAPHIT_CLI")

	origCompiledDefaults := CompiledDefaults
	defer func() { CompiledDefaults = origCompiledDefaults }()
	CompiledDefaults = ""
	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	agent := ResolveAgent("flag_agent", nil, nil)
	if agent != "flag_agent" {
		t.Errorf("expected flag_agent, got %q", agent)
	}

	agent = ResolveAgent("", nil, nil)
	if agent != FallbackAgent {
		t.Errorf("expected default fallback to be %q, got %q", FallbackAgent, agent)
	}

	if CLIForAgent("antigravity") != "agy" {
		t.Errorf("expected agy for antigravity, got %q", CLIForAgent("antigravity"))
	}
	if CLIForAgent("gemini") != "gemini" {
		t.Errorf("expected gemini, got %q", CLIForAgent("gemini"))
	}
	if CLIForAgent("claude") != "claude" {
		t.Errorf("expected claude, got %q", CLIForAgent("claude"))
	}
	if CLIForAgent("cursor") != "cursor-agent" {
		t.Errorf("expected cursor-agent, got %q", CLIForAgent("cursor"))
	}
	if CLIForAgent("codex") != "codex" {
		t.Errorf("expected codex, got %q", CLIForAgent("codex"))
	}
	if CLIForAgent("opencode") != "opencode" {
		t.Errorf("expected opencode, got %q", CLIForAgent("opencode"))
	}
	if CLIForAgent("kiro") != "kiro-cli" {
		t.Errorf("expected kiro-cli, got %q", CLIForAgent("kiro"))
	}
	for _, agent := range []string{"qwen", "kimi", "deepcode"} {
		if CLIForAgent(agent) != agent {
			t.Errorf("expected %s, got %q", agent, CLIForAgent(agent))
		}
	}
	if CLIForAgent("deepseek") != "" {
		t.Errorf("deepseek is a provider name, not an agent alias; got %q", CLIForAgent("deepseek"))
	}
	if CLIForAgent("unknown") != "" {
		t.Errorf("expected empty for unknown, got %q", CLIForAgent("unknown"))
	}

	cli := ResolveCLI("flag_cli", nil, nil, "")
	if cli != "flag_cli" {
		t.Errorf("expected flag_cli, got %q", cli)
	}

	cli = ResolveCLI("", nil, nil, "gemini")
	if cli != "gemini" {
		t.Errorf("expected gemini CLI, got %q", cli)
	}

	cli = ResolveCLI("", nil, nil, "")
	if cli != FallbackCLI {
		t.Errorf("expected default CLI to be %q, got %q", FallbackCLI, cli)
	}

	if DefaultAgent() != FallbackAgent {
		t.Errorf("expected DefaultAgent to be %q, got %q", FallbackAgent, DefaultAgent())
	}
	if DefaultCLI() != FallbackCLI {
		t.Errorf("expected DefaultCLI to be %q, got %q", FallbackCLI, DefaultCLI())
	}
}

func TestResolveProjectAgent(t *testing.T) {
	agent := ResolveProjectAgent("flag_agent", nil, nil, nil)
	if agent != "flag_agent" {
		t.Errorf("ResolveProjectAgent flag = %q; want %q", agent, "flag_agent")
	}

	inlineCfg := ConfigMap{"agent": "inline_agent"}
	agent = ResolveProjectAgent("", inlineCfg, nil, nil)
	if agent != "inline_agent" {
		t.Errorf("ResolveProjectAgent inline = %q; want %q", agent, "inline_agent")
	}

	projectCfg := ConfigMap{"agent": "project_agent"}
	agent = ResolveProjectAgent("", nil, projectCfg, nil)
	if agent != "project_agent" {
		t.Errorf("ResolveProjectAgent project = %q; want %q", agent, "project_agent")
	}

	origEnv := os.Getenv("GRAPHIT_AGENT")
	defer func() { _ = os.Setenv("GRAPHIT_AGENT", origEnv) }()
	_ = os.Setenv("GRAPHIT_AGENT", "ambient_agent")

	agent = ResolveProjectAgent("", nil, nil, []string{"other_agent", "ambient_agent"})
	if agent != "ambient_agent" {
		t.Errorf("ResolveProjectAgent lockfile match = %q; want %q", agent, "ambient_agent")
	}

	agent = ResolveProjectAgent("", nil, nil, []string{"first_agent", "other_agent"})
	if agent != "first_agent" {
		t.Errorf("ResolveProjectAgent lockfile first = %q; want %q", agent, "first_agent")
	}
}

func TestRepoURLsAndDirs(t *testing.T) {
	inline := ConfigMap{
		"ast": map[string]any{
			"index_source": "false",
		},
		"knowledge": map[string]any{
			"docs_dir": "custom_docs",
		},
	}

	if ResolveIndexSource(inline, nil) != false {
		t.Errorf("expected index_source to be false")
	}
	if ResolveDocsDir(inline, nil) != "custom_docs" {
		t.Errorf("expected custom_docs")
	}
	if got := ResolveDocsDir(nil, nil); got != DefaultDocsDir {
		t.Errorf("default docs dir = %q; want %q", got, DefaultDocsDir)
	}
}

// The root README is in the wiki whatever knowledge.docs_dir says, unless the
// project asks for it not to be.
func TestResolveKnowledgeIncludeReadme(t *testing.T) {
	if !ResolveKnowledgeIncludeReadme(nil, nil) {
		t.Error("the root README is not indexed by default")
	}
	off := ConfigMap{"knowledge": map[string]any{"include_readme": "false"}}
	if ResolveKnowledgeIncludeReadme(off, nil) {
		t.Error("knowledge.include_readme=false did not switch the README off")
	}
	on := ConfigMap{"knowledge": map[string]any{"include_readme": "true"}}
	if !ResolveKnowledgeIncludeReadme(on, nil) {
		t.Error("knowledge.include_readme=true switched the README off")
	}
}

func TestResolveHubIcebugReverseEdges(t *testing.T) {
	if !ResolveHubIcebugReverseEdges(nil, nil) {
		t.Error("reverse edges are not enabled by default")
	}

	off := ConfigMap{"hub": map[string]any{"icebug.reverse_edges": "false"}}
	if ResolveHubIcebugReverseEdges(off, nil) {
		t.Error("inline hub.icebug.reverse_edges=false did not disable reverse edges")
	}
	if ResolveHubIcebugReverseEdges(nil, off) {
		t.Error("project hub.icebug.reverse_edges=false did not disable reverse edges")
	}

	on := ConfigMap{"hub": map[string]any{"icebug.reverse_edges": "true"}}
	if !ResolveHubIcebugReverseEdges(on, off) {
		t.Error("higher-priority inline true did not override project false")
	}
}

// The docs tree is the wiki's, not the code graph's — unless the project says so.
func TestResolveAstIndexDocs(t *testing.T) {
	if ResolveAstIndexDocs(nil, nil) {
		t.Error("the AST pipeline indexes the docs tree by default")
	}
	on := ConfigMap{"ast": map[string]any{"index_docs": "true"}}
	if !ResolveAstIndexDocs(on, nil) {
		t.Error("ast.index_docs=true did not opt the docs tree back in")
	}
	off := ConfigMap{"ast": map[string]any{"index_docs": "false"}}
	if ResolveAstIndexDocs(off, nil) {
		t.Error("ast.index_docs=false enabled docs indexing")
	}
}

// LoadProjectConfig exists so internal/ast can read project config without
// importing hub, which imports ast. It has to be as forgiving as the lockfile
// loader it stands in for: no file and no valid JSON both mean "nothing set".
func TestLoadProjectConfig(t *testing.T) {
	dir := t.TempDir()

	if cfg := LoadProjectConfig(dir); cfg != nil {
		t.Errorf("no lockfile returned %v; want nil", cfg)
	}

	lock := filepath.Join(dir, brand.LockFileName())
	if err := os.WriteFile(lock, []byte("{not json"), 0o644); err != nil {
		t.Fatal(err)
	}
	if cfg := LoadProjectConfig(dir); cfg != nil {
		t.Errorf("malformed lockfile returned %v; want nil", cfg)
	}

	body := `{"project":{"id":"x"},"config":{"knowledge":{"docs_dir":"documentacao"}}}`
	if err := os.WriteFile(lock, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := ResolveDocsDir(nil, LoadProjectConfig(dir)); got != "documentacao" {
		t.Errorf("docs dir from lockfile = %q; want %q", got, "documentacao")
	}
}

func TestGlobalConfigOperations(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tempDir, err := os.MkdirTemp("", "config-test-home")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()
	_ = os.Setenv("HOME", tempDir)

	appDir, err := AppDir()
	if err != nil {
		t.Fatalf("AppDir() error: %v", err)
	}
	if !strings.HasPrefix(appDir, tempDir) {
		t.Errorf("AppDir() = %q; expected prefix %q", appDir, tempDir)
	}

	hubPath, err := HubRepoDirPath()
	if err != nil {
		t.Fatalf("HubRepoDirPath() error: %v", err)
	}
	if !strings.Contains(hubPath, "hub") {
		t.Errorf("expected path to contain 'hub', got %q", hubPath)
	}

	err = SetGlobalConfigValue("test.key", "value123")
	if err != nil {
		t.Fatalf("failed to set global config value: %v", err)
	}

	val, ok, err := GetGlobalConfigValue("test.key")
	if err != nil {
		t.Fatalf("failed to get global config value: %v", err)
	}
	if !ok || val != "value123" {
		t.Errorf("expected test.key=value123, got %q (ok=%t)", val, ok)
	}

	err = UnsetGlobalConfigValue("test.key")
	if err != nil {
		t.Fatalf("failed to unset global config value: %v", err)
	}

	val, ok, err = GetGlobalConfigValue("test.key")
	if err != nil {
		t.Fatalf("failed to get global config value: %v", err)
	}
	if ok || val != "" {
		t.Errorf("expected test.key to be unset, got %q (ok=%t)", val, ok)
	}

	path, _ := globalConfigPath()
	err = os.WriteFile(path, []byte("{invalid-json"), 0644)
	if err != nil {
		t.Fatalf("failed to write invalid json: %v", err)
	}

	_, err = LoadGlobalConfig()
	if err == nil {
		t.Error("expected error loading invalid json")
	}

	err = os.WriteFile(path, []byte("null"), 0644)
	if err != nil {
		t.Fatalf("failed to write null json: %v", err)
	}
	nullCfg, err := LoadGlobalConfig()
	if err != nil {
		t.Fatalf("expected no error loading null config: %v", err)
	}
	if len(nullCfg) != 0 {
		t.Errorf("expected empty config for null json, got: %v", nullCfg)
	}
}

func TestResolveUrls(t *testing.T) {
	seedConfigAuth(t, "env-hub-bucket", "")

	if HubBucket() != "env-hub-bucket" {
		t.Errorf("expected env-hub-bucket, got %q", HubBucket())
	}
}

// The prefix is joined into every key, so a value the user typed with slashes must not
// produce a doubled separator.
func TestHubPrefixIsNormalised(t *testing.T) {
	seedConfigAuth(t, "bucket", "/team-a/hub/")
	if got := HubPrefix(); got != "team-a/hub" {
		t.Errorf("HubPrefix() = %q; want %q", got, "team-a/hub")
	}
}

func seedConfigAuth(t *testing.T, bucket, prefix string) {
	t.Helper()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{AllowAWSCredentialChain: true}, S3: auth.S3Config{Bucket: bucket, Prefix: prefix}}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "test", Provider: "test", Username: "test"}); err != nil {
		t.Fatal(err)
	}
}

func TestTaskPrefixUsesNormalConfigPrecedenceAndNormalisation(t *testing.T) {
	t.Setenv("GRAPHIT_TASK_PREFIX", "/global-team/tasks/")
	if got := ResolveTaskPrefix(nil, nil); got != "global-team/tasks" {
		t.Fatalf("environment task prefix = %q", got)
	}
	t.Setenv("GRAPHIT_TASK_PREFIX", "")
	project := ConfigMap{"task": map[string]any{"prefix": "project/tasks"}}
	if got := ResolveTaskPrefix(nil, project); got != "project/tasks" {
		t.Fatalf("project task prefix = %q", got)
	}
	inline := ConfigMap{"task": map[string]any{"prefix": "inline/tasks"}}
	if got := ResolveTaskPrefix(inline, project); got != "inline/tasks" {
		t.Fatalf("inline task prefix = %q", got)
	}
}

func TestTaskStorageDurationsUseSafeDefaultsAndConfigPrecedence(t *testing.T) {
	t.Setenv("GRAPHIT_TASK_OPERATION_TIMEOUT", "")
	t.Setenv("GRAPHIT_TASK_VERSION_RETENTION", "")
	if got := ResolveTaskOperationTimeout(nil, nil); got != 30*time.Second {
		t.Fatalf("default task operation timeout = %s", got)
	}
	if got := ResolveTaskVersionRetention(nil, nil); got != 15*time.Minute {
		t.Fatalf("default task version retention = %s", got)
	}

	project := ConfigMap{"task": map[string]any{"operation_timeout": "45s", "version_retention": "1h"}}
	if got := ResolveTaskOperationTimeout(nil, project); got != 45*time.Second {
		t.Fatalf("project task operation timeout = %s", got)
	}
	if got := ResolveTaskVersionRetention(nil, project); got != time.Hour {
		t.Fatalf("project task version retention = %s", got)
	}

	inline := ConfigMap{"task": map[string]any{"operation_timeout": "10s", "version_retention": "30m"}}
	if got := ResolveTaskOperationTimeout(inline, project); got != 10*time.Second {
		t.Fatalf("inline task operation timeout = %s", got)
	}
	if got := ResolveTaskVersionRetention(inline, project); got != 30*time.Minute {
		t.Fatalf("inline task version retention = %s", got)
	}
}

// Configured() is the single test for "the Hub has a remote", so a config carrying only a
// region must not read as configured.
func TestS3ConfigIsOnlyConfiguredWithABucket(t *testing.T) {
	if (S3Config{Region: "us-east-1"}).Configured() {
		t.Error("a config with only a region reported itself as configured")
	}
	if !(S3Config{Bucket: "b"}).Configured() {
		t.Error("a config with a bucket reported itself as unconfigured")
	}
}

func TestAppDirHomeError(t *testing.T) {
	origHome := os.Getenv("HOME")
	origUserProfile := os.Getenv("USERPROFILE")
	defer func() {
		_ = os.Setenv("HOME", origHome)
		_ = os.Setenv("USERPROFILE", origUserProfile)
	}()

	_ = os.Unsetenv("HOME")
	_ = os.Unsetenv("USERPROFILE")

	_, err := AppDir()
	if err == nil {
		t.Error("expected error when HOME and USERPROFILE are unset")
	}

	_, err = globalConfigPath()
	if err == nil {
		t.Error("expected error in globalConfigPath when HOME is unset")
	}

	_, err = LoadGlobalConfig()
	if err == nil {
		t.Error("expected error in LoadGlobalConfig when HOME is unset")
	}

	err = SaveGlobalConfig(make(ConfigMap))
	if err == nil {
		t.Error("expected error in SaveGlobalConfig when HOME is unset")
	}

	_, _, err = GetGlobalConfigValue("key")
	if err == nil {
		t.Error("expected error in GetGlobalConfigValue when HOME is unset")
	}

	err = SetGlobalConfigValue("key", "val")
	if err == nil {
		t.Error("expected error in SetGlobalConfigValue when HOME is unset")
	}

	err = UnsetGlobalConfigValue("key")
	if err == nil {
		t.Error("expected error in UnsetGlobalConfigValue when HOME is unset")
	}

	_, err = HubRepoDirPath()
	if err == nil {
		t.Error("expected error in HubRepoDirPath when HOME is unset")
	}

}

func TestAppDirMkdirError(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tempDir, err := os.MkdirTemp("", "config-mkdir-err")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()
	_ = os.Setenv("HOME", tempDir)

	conflictPath := filepath.Join(tempDir, ".graphit")
	err = os.WriteFile(conflictPath, []byte("regular file"), 0644)
	if err != nil {
		t.Fatalf("failed to create conflicting file: %v", err)
	}

	_, err = AppDir()
	if err == nil {
		t.Error("expected MkdirAll error when brand directory path exists as a regular file")
	}
}

func TestSaveGlobalConfigError(t *testing.T) {
	cfg := ConfigMap{
		"invalid": make(chan int),
	}
	err := SaveGlobalConfig(cfg)
	if err == nil {
		t.Error("expected json marshal error when saving config with channel")
	}
}

func TestUncoveredBranches(t *testing.T) {
	tempDir, err := os.MkdirTemp("", "config-uncovered-home")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()

	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()
	_ = os.Setenv("HOME", tempDir)

	cfg := ConfigMap{
		"empty_sec": map[string]any{},
		"valid_sec": map[string]any{"k": "v"},
	}
	err = SaveGlobalConfig(cfg)
	if err != nil {
		t.Fatalf("unexpected save error: %v", err)
	}
	if _, ok := cfg["empty_sec"]; ok {
		t.Error("expected empty_sec to be deleted")
	}

	cfg2 := ConfigMap{
		"mysec": map[string]any{"other_key": "val"},
	}
	SetConfigValue(cfg2, "mysec.key", "val")
	val, ok := GetConfigValue(cfg2, "mysec.key")
	if !ok || val != "val" {
		t.Errorf("expected mysec.key=val, got %q (ok=%t)", val, ok)
	}

	inlineCfg := ConfigMap{"agent": "myide", "cli": "mycli"}
	resAgent := ResolveAgent("", inlineCfg, nil)
	if resAgent != "myide" {
		t.Errorf("expected myide, got %q", resAgent)
	}
	resCLI := ResolveCLI("", inlineCfg, nil, "")
	if resCLI != "mycli" {
		t.Errorf("expected mycli, got %q", resCLI)
	}

	origEnv := os.Getenv("GRAPHIT_AGENT")
	defer func() { _ = os.Setenv("GRAPHIT_AGENT", origEnv) }()
	_ = os.Setenv("GRAPHIT_AGENT", "ambient_agent")
	agent := ResolveProjectAgent("", nil, nil, nil)
	if agent != "ambient_agent" {
		t.Errorf("expected ambient_agent, got %q", agent)
	}

	_ = os.Unsetenv("GRAPHIT_AGENT")

	err = SetGlobalConfigValue("agent", "global_agent")
	if err != nil {
		t.Fatalf("failed to set global agent: %v", err)
	}
	agent = ResolveProjectAgent("", nil, nil, nil)
	if agent != "global_agent" {
		t.Errorf("expected global_agent, got %q", agent)
	}

	err = UnsetGlobalConfigValue("agent")
	if err != nil {
		t.Fatalf("failed to unset global agent: %v", err)
	}

	origCompiledDefaults := CompiledDefaults
	defer func() { CompiledDefaults = origCompiledDefaults }()
	CompiledDefaults = "agent=default_agent"
	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	agent = ResolveProjectAgent("", nil, nil, nil)
	if agent != "default_agent" {
		t.Errorf("expected default_agent, got %q", agent)
	}

	CompiledDefaults = ""
	parsedDefaults = nil
	defaultsOnce = sync.Once{}
	agent = ResolveProjectAgent("", nil, nil, nil)
	if agent != FallbackAgent {
		t.Errorf("expected fallback to %q, got %q", FallbackAgent, agent)
	}

	err = SetGlobalConfigValue("some.global.key", "resolved_global_val")
	if err != nil {
		t.Fatalf("failed to set global key: %v", err)
	}
	valGlobal := ResolveConfig("some.global.key", nil, nil)
	if valGlobal != "resolved_global_val" {
		t.Errorf("expected resolved_global_val, got %q", valGlobal)
	}

	if !ResolveIndexSource(nil, nil) {
		t.Error("expected ResolveIndexSource to default to true")
	}

}

func TestIsSetupDone(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tempDir := t.TempDir()
	_ = os.Setenv("HOME", tempDir)

	if IsSetupDone() {
		t.Error("expected IsSetupDone() to be false before config exists")
	}

	err := SetGlobalConfigValue("setup.done", "true")
	if err != nil {
		t.Fatalf("failed to set global config: %v", err)
	}

	if !IsSetupDone() {
		t.Error("expected IsSetupDone() to be true after config exists")
	}
}

func TestIsSetupDoneHomeError(t *testing.T) {
	origHome := os.Getenv("HOME")
	origUserProfile := os.Getenv("USERPROFILE")
	defer func() {
		_ = os.Setenv("HOME", origHome)
		_ = os.Setenv("USERPROFILE", origUserProfile)
	}()

	_ = os.Unsetenv("HOME")
	_ = os.Unsetenv("USERPROFILE")

	if IsSetupDone() {
		t.Error("expected IsSetupDone() to be false when HOME is unset")
	}
}

func TestLoadGlobalConfigReadError(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tempDir := t.TempDir()
	_ = os.Setenv("HOME", tempDir)

	appDir := filepath.Join(tempDir, ".graphit")
	err := os.MkdirAll(appDir, 0o700)
	if err != nil {
		t.Fatalf("failed to create app dir: %v", err)
	}

	configPath := filepath.Join(appDir, "config.json")
	err = os.MkdirAll(configPath, 0o755)
	if err != nil {
		t.Fatalf("failed to create config dir: %v", err)
	}

	_, err = LoadGlobalConfig()
	if err == nil {
		t.Error("expected read error when config.json is a directory")
	}
	if !strings.Contains(err.Error(), "reading global config") {
		t.Errorf("expected 'reading global config' error, got: %v", err)
	}
}

func TestIsModuleDisabledOptIn(t *testing.T) {
	if !IsModuleDisabled("dream", nil, nil) {
		t.Error("expected 'dream' opt-in module to be disabled by default")
	}

	cfgTrue := ConfigMap{
		"modules": map[string]any{
			"dream": "true",
		},
	}
	if IsModuleDisabled("dream", nil, cfgTrue) {
		t.Error("expected 'dream' module to be enabled when explicitly set to true")
	}

	cfgFalse := ConfigMap{
		"modules": map[string]any{
			"dream": "false",
		},
	}
	if !IsModuleDisabled("dream", nil, cfgFalse) {
		t.Error("expected 'dream' module to be disabled when explicitly set to false")
	}
}

func TestIsOptInModule(t *testing.T) {
	if !isOptInModule("dream") {
		t.Error("expected 'dream' to be an opt-in module")
	}
	if !isOptInModule("DREAM") {
		t.Error("expected case-insensitive match for 'DREAM' as opt-in module")
	}

	if isOptInModule("ast") {
		t.Error("expected 'ast' to NOT be an opt-in module")
	}
}

func TestResolveProjectActivityWindow_Default(t *testing.T) {
	if got := ResolveProjectActivityWindow(nil, nil); got != defaultProjectActivityWindow {
		t.Errorf("ResolveProjectActivityWindow() = %v; want default %v", got, defaultProjectActivityWindow)
	}
}

func TestResolveProjectActivityWindow_ProjectOverride(t *testing.T) {
	projectCfg := ConfigMap{
		"daemon": map[string]any{
			"activity_window": "15m",
		},
	}
	if got := ResolveProjectActivityWindow(nil, projectCfg); got != 15*time.Minute {
		t.Errorf("ResolveProjectActivityWindow() = %v; want 15m", got)
	}
}

func TestResolveProjectActivityWindow_EnvOverride(t *testing.T) {
	envKey := "GRAPHIT_DAEMON_ACTIVITY_WINDOW"
	origEnv := os.Getenv(envKey)
	defer func() { _ = os.Setenv(envKey, origEnv) }()
	_ = os.Setenv(envKey, "1h")

	if got := ResolveProjectActivityWindow(nil, nil); got != time.Hour {
		t.Errorf("ResolveProjectActivityWindow() = %v; want 1h", got)
	}
}

func TestResolveProjectActivityWindow_ZeroDisables(t *testing.T) {
	envKey := "GRAPHIT_DAEMON_ACTIVITY_WINDOW"
	origEnv := os.Getenv(envKey)
	defer func() { _ = os.Setenv(envKey, origEnv) }()
	_ = os.Setenv(envKey, "0")

	if got := ResolveProjectActivityWindow(nil, nil); got != 0 {
		t.Errorf("ResolveProjectActivityWindow() = %v; want 0 (disabled)", got)
	}
}

func TestResolveProjectActivityWindow_InvalidFallsBackToDefault(t *testing.T) {
	envKey := "GRAPHIT_DAEMON_ACTIVITY_WINDOW"
	origEnv := os.Getenv(envKey)
	defer func() { _ = os.Setenv(envKey, origEnv) }()
	_ = os.Setenv(envKey, "not-a-duration")

	if got := ResolveProjectActivityWindow(nil, nil); got != defaultProjectActivityWindow {
		t.Errorf("ResolveProjectActivityWindow() = %v; want default %v on invalid input", got, defaultProjectActivityWindow)
	}
}

// Retention is a POLICY because two kinds of store want different answers: the knowledge wiki is
// rebuilt from docs/ and keeps a margin for in-flight readers, while a store holding the only copy
// of its data wants a window long enough to be a recovery path.
func TestResolveWikiVersionRetention(t *testing.T) {
	if got := ResolveWikiVersionRetention(nil, nil); got != defaultWikiVersionRetention {
		t.Errorf("unset = %s, want the %s default", got, defaultWikiVersionRetention)
	}

	long := ConfigMap{"wiki": map[string]any{"version_retention": "72h"}}
	if got := ResolveWikiVersionRetention(long, nil); got != 72*time.Hour {
		t.Errorf("72h = %s, want 72h", got)
	}

	for _, bad := range []string{"1ms", "999ms", "0", "-5m", "not-a-duration"} {
		cfg := ConfigMap{"wiki": map[string]any{"version_retention": bad}}
		if got := ResolveWikiVersionRetention(cfg, nil); got != defaultWikiVersionRetention {
			t.Errorf("%q = %s, want the %s default", bad, got, defaultWikiVersionRetention)
		}
	}

	atFloor := ConfigMap{"wiki": map[string]any{"version_retention": "1s"}}
	if got := ResolveWikiVersionRetention(atFloor, nil); got != time.Second {
		t.Errorf("1s = %s, want 1s", got)
	}
}

func TestResolveMemoryVersionRetentionIsIndependentOfTheWikis(t *testing.T) {
	if got := ResolveMemoryVersionRetention(nil, nil); got != defaultMemoryVersionRetention {
		t.Errorf("unset = %s, want the %s default", got, defaultMemoryVersionRetention)
	}
	if defaultMemoryVersionRetention <= defaultWikiVersionRetention {
		t.Errorf("the memory store's default retention (%s) must comfortably exceed the wiki's (%s) — "+
			"it is a recovery window, not a reader margin",
			defaultMemoryVersionRetention, defaultWikiVersionRetention)
	}

	onlyWiki := ConfigMap{"wiki": map[string]any{"version_retention": "1h"}}
	if got := ResolveMemoryVersionRetention(onlyWiki, nil); got != defaultMemoryVersionRetention {
		t.Errorf("the wiki's key moved the memory store's retention to %s", got)
	}
	onlyMemory := ConfigMap{"memory": map[string]any{"version_retention": "2160h"}}
	if got := ResolveMemoryVersionRetention(onlyMemory, nil); got != 2160*time.Hour {
		t.Errorf("memory.version_retention = %s, want 2160h", got)
	}
	if got := ResolveWikiVersionRetention(onlyMemory, nil); got != defaultWikiVersionRetention {
		t.Errorf("the memory key moved the wiki's retention to %s", got)
	}

	for _, bad := range []string{"1ms", "0", "-5m", "nonsense"} {
		cfg := ConfigMap{"memory": map[string]any{"version_retention": bad}}
		if got := ResolveMemoryVersionRetention(cfg, nil); got != defaultMemoryVersionRetention {
			t.Errorf("%q = %s, want the default", bad, got)
		}
	}
}
