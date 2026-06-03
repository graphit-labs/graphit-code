package config

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestIsModuleDisabled(t *testing.T) {
	// Without any config, improvements should be enabled by default (not in OptInModules)
	if IsModuleDisabled("improvements", nil, nil) {
		t.Error("expected improvements module to be enabled by default")
	}

	// If explicitly set to true, it should be enabled (IsModuleDisabled = false)
	cfgTrue := ConfigMap{
		"modules": map[string]any{
			"improvements": "true",
		},
	}
	if IsModuleDisabled("improvements", nil, cfgTrue) {
		t.Error("expected improvements module to be enabled when explicitly configured to true")
	}

	// If explicitly set to false, it should be disabled (IsModuleDisabled = true)
	cfgFalse := ConfigMap{
		"modules": map[string]any{
			"improvements": "false",
		},
	}
	if !IsModuleDisabled("improvements", nil, cfgFalse) {
		t.Error("expected improvements module to be disabled when explicitly configured to false")
	}

	// For comparison, another default module like "ast" should be enabled by default (IsModuleDisabled = false)
	if IsModuleDisabled("ast", nil, nil) {
		t.Error("expected ast module to be enabled by default")
	}
}

func TestConfigCRUD(t *testing.T) {
	cfg := make(ConfigMap)

	// Test SetConfigValue (non-nested)
	SetConfigValue(cfg, "foo", "bar")
	val, ok := GetConfigValue(cfg, "foo")
	if !ok || val != "bar" {
		t.Errorf("expected foo=bar, got %q (ok=%t)", val, ok)
	}

	// Test SetConfigValue (nested)
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

	// Test ListConfigEntries
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

	// Test UnsetConfigValue (non-nested)
	UnsetConfigValue(cfg, "foo")
	_, ok = GetConfigValue(cfg, "foo")
	if ok {
		t.Error("expected foo to be unset")
	}

	// Test UnsetConfigValue (nested)
	UnsetConfigValue(cfg, "nested.key")
	_, ok = GetConfigValue(cfg, "nested.key")
	if ok {
		t.Error("expected nested.key to be unset")
	}
}

func TestResolveConfig(t *testing.T) {
	// Set defaults
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

	// ResolveIDE tests
	ide := ResolveIDE("flag_ide", nil, nil)
	if ide != "flag_ide" {
		t.Errorf("expected flag_ide, got %q", ide)
	}

	ide = ResolveIDE("", nil, nil)
	if ide != "claude" { // default fallback
		t.Errorf("expected default fallback to be 'claude', got %q", ide)
	}

	// CLIForIDE mappings
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

	// ResolveCLI tests
	cli := ResolveCLI("flag_cli", nil, nil, "")
	if cli != "flag_cli" {
		t.Errorf("expected flag_cli, got %q", cli)
	}

	cli = ResolveCLI("", nil, nil, "gemini")
	if cli != "gemini" {
		t.Errorf("expected gemini CLI, got %q", cli)
	}

	cli = ResolveCLI("", nil, nil, "")
	if cli != "claude" {
		t.Errorf("expected default CLI to be 'claude', got %q", cli)
	}

	// DefaultIDE and DefaultCLI
	if DefaultIDE() != "claude" {
		t.Errorf("expected DefaultIDE to be 'claude', got %q", DefaultIDE())
	}
	if DefaultCLI() != "claude" {
		t.Errorf("expected DefaultCLI to be 'claude', got %q", DefaultCLI())
	}
}

func TestResolveProjectIDE(t *testing.T) {
	// Test priority 1: flagValue
	ide := ResolveProjectIDE("flag_ide", nil, nil, nil)
	if ide != "flag_ide" {
		t.Errorf("ResolveProjectIDE flag = %q; want %q", ide, "flag_ide")
	}

	// Test priority 2: inlineCfg
	inlineCfg := ConfigMap{"ide": "inline_ide"}
	ide = ResolveProjectIDE("", inlineCfg, nil, nil)
	if ide != "inline_ide" {
		t.Errorf("ResolveProjectIDE inline = %q; want %q", ide, "inline_ide")
	}

	// Test priority 3: projectCfg
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
	// Test HubRepoURL, MemoryRepoURL, MemoryRepoDirPath, HubRepoDirPath, ResolveIndexSource, ResolveDocsDir
	inline := ConfigMap{
		"hub": map[string]any{
			"repo": "hub_repo_url",
		},
		"memory": map[string]any{
			"repo": "memory_repo_url",
		},
		"ast": map[string]any{
			"index_source": "false",
		},
		"knowledge": map[string]any{
			"docs_dir": "custom_docs",
		},
	}

	if ResolveHubRepo(inline, nil) != "hub_repo_url" {
		t.Errorf("expected hub_repo_url")
	}
	if ResolveMemoryRepo(inline, nil) != "memory_repo_url" {
		t.Errorf("expected memory_repo_url")
	}
	if ResolveIndexSource(inline, nil) != false {
		t.Errorf("expected index_source to be false")
	}
	if ResolveDocsDir(inline, nil) != "custom_docs" {
		t.Errorf("expected custom_docs")
	}
	if ResolveDocsDir(nil, nil) != "." {
		t.Errorf("expected default docs dir to be '.'")
	}

	// Test HubDirForRepo & sanitizeRepoName
	dir, err := HubDirForRepo("https://github.com/org/repo.git")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(dir, "github.com_org_repo") {
		t.Errorf("HubDirForRepo sanitized name incorrect, got %q", dir)
	}

	dirDefault, _ := HubDirForRepo("")
	if !strings.Contains(dirDefault, "default") {
		t.Errorf("expected default path, got %q", dirDefault)
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

	// Validate AppDir
	appDir, err := AppDir()
	if err != nil {
		t.Fatalf("AppDir() error: %v", err)
	}
	if !strings.HasPrefix(appDir, tempDir) {
		t.Errorf("AppDir() = %q; expected prefix %q", appDir, tempDir)
	}

	// Validate paths
	memPath, err := MemoryRepoDirPath()
	if err != nil {
		t.Fatalf("MemoryRepoDirPath() error: %v", err)
	}
	if !strings.Contains(memPath, "memory") {
		t.Errorf("expected path to contain 'memory', got %q", memPath)
	}

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

	// Unset key
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
	origHubEnv := os.Getenv("GRAPHIT_HUB_REPO")
	origMemEnv := os.Getenv("GRAPHIT_MEMORY_REPO")
	defer func() {
		_ = os.Setenv("GRAPHIT_HUB_REPO", origHubEnv)
		_ = os.Setenv("GRAPHIT_MEMORY_REPO", origMemEnv)
	}()

	_ = os.Setenv("GRAPHIT_HUB_REPO", "env-hub-repo")
	_ = os.Setenv("GRAPHIT_MEMORY_REPO", "env-mem-repo")

	if HubRepoURL() != "env-hub-repo" {
		t.Errorf("expected env-hub-repo, got %q", HubRepoURL())
	}
	if MemoryRepoURL() != "env-mem-repo" {
		t.Errorf("expected env-mem-repo, got %q", MemoryRepoURL())
	}
}

func TestDisabledModulesHelper(t *testing.T) {
	cfg := ConfigMap{
		"modules": map[string]any{
			"improvements": "true",
			"ast":          "false",
		},
	}
	disabled := DisabledModules(nil, cfg)
	foundAst := false
	foundImprovements := false
	for _, m := range disabled {
		if m == "ast" {
			foundAst = true
		}
		if m == "improvements" {
			foundImprovements = true
		}
	}
	if !foundAst {
		t.Error("expected ast module to be disabled")
	}
	if foundImprovements {
		t.Error("expected improvements module to be enabled (not disabled)")
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

	_, err = MemoryRepoDirPath()
	if err == nil {
		t.Error("expected error in MemoryRepoDirPath when HOME is unset")
	}

	_, err = HubRepoDirPath()
	if err == nil {
		t.Error("expected error in HubRepoDirPath when HOME is unset")
	}

	_, err = HubDirForRepo("url")
	if err == nil {
		t.Error("expected error in HubDirForRepo when HOME is unset")
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

	// 5a. Global config has it
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

	// 5c. resolveAmbientIDE fallback to "claude"
	CompiledDefaults = ""
	parsedDefaults = nil
	defaultsOnce = sync.Once{}
	ide = ResolveProjectIDE("", nil, nil, nil)
	if ide != "claude" {
		t.Errorf("expected fallback to claude, got %q", ide)
	}

	// 5d. ResolveConfig resolving from global config file
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

	// 7. sanitizeRepoName with @ symbol
	dir, err := HubDirForRepo("git@github.com:org/repo.git")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(dir, "github.com_org_repo") {
		t.Errorf("HubDirForRepo sanitized name incorrect, got %q", dir)
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

	// Create config file
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

	// Create the app dir
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

