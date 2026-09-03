package config

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestIsModuleDisabled(t *testing.T) {
	// Without any config, AST should be enabled by default (not in OptInModules).
	if IsModuleDisabled("ast", nil, nil) {
		t.Error("expected AST module to be enabled by default")
	}

	// If explicitly set to true, it should be enabled (IsModuleDisabled = false).
	cfgTrue := ConfigMap{
		"modules": map[string]any{
			"ast": "true",
		},
	}
	if IsModuleDisabled("ast", nil, cfgTrue) {
		t.Error("expected AST module to be enabled when explicitly configured to true")
	}

	// If explicitly set to false, it should be disabled (IsModuleDisabled = true).
	cfgFalse := ConfigMap{
		"modules": map[string]any{
			"ast": "false",
		},
	}
	if !IsModuleDisabled("ast", nil, cfgFalse) {
		t.Error("expected AST module to be disabled when explicitly configured to false")
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

	// Test SetConfigValue (nested overwrite non-map)
	cfg["nested_bad"] = "not_a_map"
	SetConfigValue(cfg, "nested_bad.key", "new_val")
	val, ok = GetConfigValue(cfg, "nested_bad.key")
	if !ok || val != "new_val" {
		t.Errorf("expected nested_bad.key=new_val, got %q (ok=%t)", val, ok)
	}

	// Test GetConfigValue when key is not a string
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

	// Reset default cache for testing
	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	// Test 1: Fallback to defaults
	val := ResolveConfig("default.key", nil, nil)
	if val != "default_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "default_val")
	}

	// Test 2: Project config overrides defaults
	projectCfg := ConfigMap{
		"default": map[string]any{
			"key": "project_val",
		},
	}
	val = ResolveConfig("default.key", nil, projectCfg)
	if val != "project_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "project_val")
	}

	// Test 3: Env overrides project and defaults
	origEnv := os.Getenv("GRAPHIT_DEFAULT_KEY")
	defer func() { _ = os.Setenv("GRAPHIT_DEFAULT_KEY", origEnv) }()
	_ = os.Setenv("GRAPHIT_DEFAULT_KEY", "env_val")

	val = ResolveConfig("default.key", nil, projectCfg)
	if val != "env_val" {
		t.Errorf("ResolveConfig(default.key) = %q; want %q", val, "env_val")
	}

	// Test 4: Inline config overrides Env, project, and defaults
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

func TestResolveIDEAndCLI(t *testing.T) {
	// Isolate Brand
	origBrand := brand.Brand
	brand.Brand = "graphit"
	defer func() { brand.Brand = origBrand }()

	// Isolate HOME
	origHome := os.Getenv("HOME")
	tempDir, err := os.MkdirTemp("", "config-ide-cli-test")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer func() { _ = os.RemoveAll(tempDir) }()
	_ = os.Setenv("HOME", tempDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Save env and restore
	origEnvIDE := os.Getenv("GRAPHIT_IDE")
	origEnvCLI := os.Getenv("GRAPHIT_CLI")
	defer func() {
		_ = os.Setenv("GRAPHIT_IDE", origEnvIDE)
		_ = os.Setenv("GRAPHIT_CLI", origEnvCLI)
	}()
	_ = os.Unsetenv("GRAPHIT_IDE")
	_ = os.Unsetenv("GRAPHIT_CLI")

	// Reset CompiledDefaults and cache for testing to ensure clean slate
	origCompiledDefaults := CompiledDefaults
	defer func() { CompiledDefaults = origCompiledDefaults }()
	CompiledDefaults = ""
	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	ide := ResolveIDE("flag_ide", nil, nil)
	if ide != "flag_ide" {
		t.Errorf("expected flag_ide, got %q", ide)
	}

	ide = ResolveIDE("", nil, nil)
	if ide != FallbackIDE {
		t.Errorf("expected default fallback to be %q, got %q", FallbackIDE, ide)
	}

	if CLIForIDE("antigravity") != "agy" {
		t.Errorf("expected agy for antigravity, got %q", CLIForIDE("antigravity"))
	}
	if CLIForIDE("gemini") != "gemini" {
		t.Errorf("expected gemini, got %q", CLIForIDE("gemini"))
	}
	if CLIForIDE("claude") != "claude" {
		t.Errorf("expected claude, got %q", CLIForIDE("claude"))
	}
	if CLIForIDE("cursor") != "cursor-agent" {
		t.Errorf("expected cursor-agent, got %q", CLIForIDE("cursor"))
	}
	if CLIForIDE("codex") != "codex" {
		t.Errorf("expected codex, got %q", CLIForIDE("codex"))
	}
	if CLIForIDE("opencode") != "opencode" {
		t.Errorf("expected opencode, got %q", CLIForIDE("opencode"))
	}
	if CLIForIDE("kiro") != "kiro-cli" {
		t.Errorf("expected kiro-cli, got %q", CLIForIDE("kiro"))
	}
	if CLIForIDE("unknown") != "" {
		t.Errorf("expected empty for unknown, got %q", CLIForIDE("unknown"))
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

	if DefaultIDE() != FallbackIDE {
		t.Errorf("expected DefaultIDE to be %q, got %q", FallbackIDE, DefaultIDE())
	}
	if DefaultCLI() != FallbackCLI {
		t.Errorf("expected DefaultCLI to be %q, got %q", FallbackCLI, DefaultCLI())
	}
}

func TestResolveProjectIDE(t *testing.T) {
	// Test priority 1: flagValue
	ide := ResolveProjectIDE("flag_ide", nil, nil, nil)
	if ide != "flag_ide" {
		t.Errorf("ResolveProjectIDE flag = %q; want %q", ide, "flag_ide")
	}

	inlineCfg := ConfigMap{"ide": "inline_ide"}
	ide = ResolveProjectIDE("", inlineCfg, nil, nil)
	if ide != "inline_ide" {
		t.Errorf("ResolveProjectIDE inline = %q; want %q", ide, "inline_ide")
	}

	projectCfg := ConfigMap{"ide": "project_ide"}
	ide = ResolveProjectIDE("", nil, projectCfg, nil)
	if ide != "project_ide" {
		t.Errorf("ResolveProjectIDE project = %q; want %q", ide, "project_ide")
	}

	// Test priority 4: lockfileIDEs matching ambient resolved
	origEnv := os.Getenv("GRAPHIT_IDE")
	defer func() { _ = os.Setenv("GRAPHIT_IDE", origEnv) }()
	_ = os.Setenv("GRAPHIT_IDE", "ambient_ide")

	ide = ResolveProjectIDE("", nil, nil, []string{"other_ide", "ambient_ide"})
	if ide != "ambient_ide" {
		t.Errorf("ResolveProjectIDE lockfile match = %q; want %q", ide, "ambient_ide")
	}

	// Test priority 5: lockfileIDEs first element when no ambient match
	ide = ResolveProjectIDE("", nil, nil, []string{"first_ide", "other_ide"})
	if ide != "first_ide" {
		t.Errorf("ResolveProjectIDE lockfile first = %q; want %q", ide, "first_ide")
	}
}

func TestRepoURLsAndDirs(t *testing.T) {
	// Test HubBucket, HubRepoDirPath, ResolveIndexSource, ResolveDocsDir
	inline := ConfigMap{
		"hub": map[string]any{
			"bucket": "hub-bucket",
			"region": "us-east-1",
		},
		"ast": map[string]any{
			"index_source": "false",
		},
		"knowledge": map[string]any{
			"docs_dir": "custom_docs",
		},
	}

	if ResolveHubBucket(inline, nil) != "hub-bucket" {
		t.Errorf("expected hub-bucket")
	}
	if ResolveHubRegion(inline, nil) != "us-east-1" {
		t.Errorf("expected us-east-1")
	}
	if ResolveIndexSource(inline, nil) != false {
		t.Errorf("expected index_source to be false")
	}
	if ResolveDocsDir(inline, nil) != "custom_docs" {
		t.Errorf("expected custom_docs")
	}
	// "docs", not "." — the whole project was the old default and it made the wiki
	// index every indexable file in the repository.
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
	// Set HOME to a temporary directory so we don't mutate local user files
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

	// Validate paths

	hubPath, err := HubRepoDirPath()
	if err != nil {
		t.Fatalf("HubRepoDirPath() error: %v", err)
	}
	if !strings.Contains(hubPath, "hub") {
		t.Errorf("expected path to contain 'hub', got %q", hubPath)
	}

	// Test CRUD via Save & Load
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

	// Test loading invalid JSON
	path, _ := globalConfigPath()
	err = os.WriteFile(path, []byte("{invalid-json"), 0644)
	if err != nil {
		t.Fatalf("failed to write invalid json: %v", err)
	}

	_, err = LoadGlobalConfig()
	if err == nil {
		t.Error("expected error loading invalid json")
	}

	// Test loading null global config
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
	// Save envs
	origBucketEnv := os.Getenv("GRAPHIT_HUB_BUCKET")
	defer func() {
		_ = os.Setenv("GRAPHIT_HUB_BUCKET", origBucketEnv)
	}()

	_ = os.Setenv("GRAPHIT_HUB_BUCKET", "env-hub-bucket")

	if HubBucket() != "env-hub-bucket" {
		t.Errorf("expected env-hub-bucket, got %q", HubBucket())
	}
}

// The prefix is joined into every key, so a value the user typed with slashes must not
// produce a doubled separator.
func TestHubPrefixIsNormalised(t *testing.T) {
	orig := os.Getenv("GRAPHIT_HUB_PREFIX")
	defer func() { _ = os.Setenv("GRAPHIT_HUB_PREFIX", orig) }()

	_ = os.Setenv("GRAPHIT_HUB_PREFIX", "/team-a/hub/")
	if got := HubPrefix(); got != "team-a/hub" {
		t.Errorf("HubPrefix() = %q; want %q", got, "team-a/hub")
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

	// Create a regular file where the brand directory should be created
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
	// Set invalid JSON value (a channel or function cannot be marshaled)
	cfg := ConfigMap{
		"invalid": make(chan int),
	}
	err := SaveGlobalConfig(cfg)
	if err == nil {
		t.Error("expected json marshal error when saving config with channel")
	}
}

func TestUncoveredBranches(t *testing.T) {
	// 1. SaveGlobalConfig empty section deletion
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

	// 2. SetConfigValue nested map append
	cfg2 := ConfigMap{
		"mysec": map[string]any{"other_key": "val"},
	}
	SetConfigValue(cfg2, "mysec.key", "val")
	val, ok := GetConfigValue(cfg2, "mysec.key")
	if !ok || val != "val" {
		t.Errorf("expected mysec.key=val, got %q (ok=%t)", val, ok)
	}

	// 3. ResolveIDE and ResolveCLI when ResolveConfig returns non-empty value
	inlineCfg := ConfigMap{"ide": "myide", "cli": "mycli"}
	resIDE := ResolveIDE("", inlineCfg, nil)
	if resIDE != "myide" {
		t.Errorf("expected myide, got %q", resIDE)
	}
	resCLI := ResolveCLI("", inlineCfg, nil, "")
	if resCLI != "mycli" {
		t.Errorf("expected mycli, got %q", resCLI)
	}

	// 4. ResolveProjectIDE when lockfileIDEs is empty
	origEnv := os.Getenv("GRAPHIT_IDE")
	defer func() { _ = os.Setenv("GRAPHIT_IDE", origEnv) }()
	_ = os.Setenv("GRAPHIT_IDE", "ambient_ide")
	ide := ResolveProjectIDE("", nil, nil, nil)
	if ide != "ambient_ide" {
		t.Errorf("expected ambient_ide, got %q", ide)
	}

	// 5. resolveAmbientIDE branch coverage: global config has it, and defaults has it
	_ = os.Unsetenv("GRAPHIT_IDE")

	err = SetGlobalConfigValue("ide", "global_ide")
	if err != nil {
		t.Fatalf("failed to set global ide: %v", err)
	}
	ide = ResolveProjectIDE("", nil, nil, nil)
	if ide != "global_ide" {
		t.Errorf("expected global_ide, got %q", ide)
	}

	// 5b. Defaults has it
	err = UnsetGlobalConfigValue("ide")
	if err != nil {
		t.Fatalf("failed to unset global ide: %v", err)
	}

	origCompiledDefaults := CompiledDefaults
	defer func() { CompiledDefaults = origCompiledDefaults }()
	CompiledDefaults = "ide=default_ide"
	parsedDefaults = nil
	defaultsOnce = sync.Once{}

	ide = ResolveProjectIDE("", nil, nil, nil)
	if ide != "default_ide" {
		t.Errorf("expected default_ide, got %q", ide)
	}

	// 5c. resolveAmbientIDE falls back to FallbackIDE
	CompiledDefaults = ""
	parsedDefaults = nil
	defaultsOnce = sync.Once{}
	ide = ResolveProjectIDE("", nil, nil, nil)
	if ide != FallbackIDE {
		t.Errorf("expected fallback to %q, got %q", FallbackIDE, ide)
	}

	err = SetGlobalConfigValue("some.global.key", "resolved_global_val")
	if err != nil {
		t.Fatalf("failed to set global key: %v", err)
	}
	valGlobal := ResolveConfig("some.global.key", nil, nil)
	if valGlobal != "resolved_global_val" {
		t.Errorf("expected resolved_global_val, got %q", valGlobal)
	}

	// 6. ResolveIndexSource defaults to true when empty
	if !ResolveIndexSource(nil, nil) {
		t.Error("expected ResolveIndexSource to default to true")
	}

}

func TestIsSetupDone(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	tempDir := t.TempDir()
	_ = os.Setenv("HOME", tempDir)

	// Before creating config file, IsSetupDone should return false
	if IsSetupDone() {
		t.Error("expected IsSetupDone() to be false before config exists")
	}

	err := SetGlobalConfigValue("setup.done", "true")
	if err != nil {
		t.Fatalf("failed to set global config: %v", err)
	}

	// After creating config file, IsSetupDone should return true
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

	// Should return false when HOME is not set (globalConfigPath errors)
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

	// Create config as a directory (not a file) to cause a non-NotExist read error
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
	// "dream" is in OptInModules, so by default it should be disabled
	if !IsModuleDisabled("dream", nil, nil) {
		t.Error("expected 'dream' opt-in module to be disabled by default")
	}

	// Explicitly enabling it
	cfgTrue := ConfigMap{
		"modules": map[string]any{
			"dream": "true",
		},
	}
	if IsModuleDisabled("dream", nil, cfgTrue) {
		t.Error("expected 'dream' module to be enabled when explicitly set to true")
	}

	// Explicitly disabling it
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
	// "dream" should be opt-in
	if !isOptInModule("dream") {
		t.Error("expected 'dream' to be an opt-in module")
	}
	if !isOptInModule("DREAM") {
		t.Error("expected case-insensitive match for 'DREAM' as opt-in module")
	}

	// "ast" should not be opt-in
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

	// A SUB-SECOND WINDOW IS REFUSED, and this is measured rather than defensive: the engine prunes
	// nothing at all below one second — it reports zero old versions while they plainly exist — so
	// honouring "1ms" would silently disable pruning while reading as the most aggressive setting
	// available.
	for _, bad := range []string{"1ms", "999ms", "0", "-5m", "not-a-duration"} {
		cfg := ConfigMap{"wiki": map[string]any{"version_retention": bad}}
		if got := ResolveWikiVersionRetention(cfg, nil); got != defaultWikiVersionRetention {
			t.Errorf("%q = %s, want the %s default", bad, got, defaultWikiVersionRetention)
		}
	}

	// Exactly one second is the floor and is honoured.
	atFloor := ConfigMap{"wiki": map[string]any{"version_retention": "1s"}}
	if got := ResolveWikiVersionRetention(atFloor, nil); got != time.Second {
		t.Errorf("1s = %s, want 1s", got)
	}
}

// 🔒 THE MEMORY STORE'S RETENTION IS A DIFFERENT NUMBER FROM THE WIKI'S, and this pins that they
// cannot be collapsed back into one key.
//
// The wiki's fifteen minutes is a margin for in-flight readers of a derived index — a pruned version
// costs a rebuild. The memory store is the only copy of what it holds, so its history is the recovery
// path D2 accepted, and the retention is the length of the safety net. Sharing one key honours D2's
// letter and breaks it in fact: a bad pass noticed the next morning finds nothing to roll back to.
func TestResolveMemoryVersionRetentionIsIndependentOfTheWikis(t *testing.T) {
	if got := ResolveMemoryVersionRetention(nil, nil); got != defaultMemoryVersionRetention {
		t.Errorf("unset = %s, want the %s default", got, defaultMemoryVersionRetention)
	}
	if defaultMemoryVersionRetention <= defaultWikiVersionRetention {
		t.Errorf("the memory store's default retention (%s) must comfortably exceed the wiki's (%s) — "+
			"it is a recovery window, not a reader margin",
			defaultMemoryVersionRetention, defaultWikiVersionRetention)
	}

	// Setting one must not move the other.
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

	// The same measured one-second floor: below it the engine prunes nothing at all, so honouring a
	// smaller value would silently disable pruning.
	for _, bad := range []string{"1ms", "0", "-5m", "nonsense"} {
		cfg := ConfigMap{"memory": map[string]any{"version_retention": bad}}
		if got := ResolveMemoryVersionRetention(cfg, nil); got != defaultMemoryVersionRetention {
			t.Errorf("%q = %s, want the default", bad, got)
		}
	}
}
