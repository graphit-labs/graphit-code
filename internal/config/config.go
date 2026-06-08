package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type ConfigMap = map[string]any

func AppDir() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(home, brand.DotDir())
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	return dir, nil
}

const globalConfigFile = "config.json"

func globalConfigPath() (string, error) {
	dir, err := AppDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, globalConfigFile), nil
}

func LoadGlobalConfig() (ConfigMap, error) {
	path, err := globalConfigPath()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return make(ConfigMap), nil
		}
		return nil, fmt.Errorf("reading global config: %w", err)
	}

	var cfg ConfigMap
	if err := json.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("parsing global config: %w", err)
	}
	if cfg == nil {
		cfg = make(ConfigMap)
	}
	return cfg, nil
}

func SaveGlobalConfig(cfg ConfigMap) error {
	path, err := globalConfigPath()
	if err != nil {
		return err
	}

	for k, v := range cfg {
		if sec, ok := v.(map[string]any); ok && len(sec) == 0 {
			delete(cfg, k)
		}
	}

	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing global config: %w", err)
	}

	return os.WriteFile(path, data, 0o600)
}

func GetConfigValue(cfg ConfigMap, dotKey string) (string, bool) {
	section, key, nested := splitKey(dotKey)

	if !nested {

		if val, ok := cfg[dotKey]; ok {
			if s, ok := val.(string); ok {
				return s, true
			}
		}
		return "", false
	}

	if sec, ok := cfg[section]; ok {
		if m, ok := sec.(map[string]any); ok {
			if val, ok := m[key]; ok {
				if s, ok := val.(string); ok {
					return s, true
				}
			}
		}
	}
	return "", false
}

func SetConfigValue(cfg ConfigMap, dotKey, value string) {
	section, key, nested := splitKey(dotKey)

	if !nested {
		cfg[dotKey] = value
		return
	}

	sec, ok := cfg[section]
	if !ok {
		cfg[section] = map[string]any{key: value}
		return
	}
	if m, ok := sec.(map[string]any); ok {
		m[key] = value
	} else {
		cfg[section] = map[string]any{key: value}
	}
}

func UnsetConfigValue(cfg ConfigMap, dotKey string) {
	section, key, nested := splitKey(dotKey)

	if !nested {
		delete(cfg, dotKey)
		return
	}

	if sec, ok := cfg[section]; ok {
		if m, ok := sec.(map[string]any); ok {
			delete(m, key)
			if len(m) == 0 {
				delete(cfg, section)
			}
		}
	}
}

func ListConfigEntries(cfg ConfigMap) [][2]string {
	var entries [][2]string
	for k, v := range cfg {
		switch val := v.(type) {
		case string:
			entries = append(entries, [2]string{k, val})
		case map[string]any:
			for subK, subV := range val {
				if s, ok := subV.(string); ok {
					entries = append(entries, [2]string{k + "." + subK, s})
				}
			}
		}
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i][0] < entries[j][0] })
	return entries
}

func GetGlobalConfigValue(dotKey string) (string, bool, error) {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return "", false, err
	}
	val, ok := GetConfigValue(cfg, dotKey)
	return val, ok, nil
}

func SetGlobalConfigValue(dotKey, value string) error {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return err
	}
	SetConfigValue(cfg, dotKey, value)
	return SaveGlobalConfig(cfg)
}

func UnsetGlobalConfigValue(dotKey string) error {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return err
	}
	UnsetConfigValue(cfg, dotKey)
	return SaveGlobalConfig(cfg)
}

var CompiledDefaults string

var (
	parsedDefaults ConfigMap
	defaultsOnce   sync.Once
)

func getCompiledDefaults() ConfigMap {
	defaultsOnce.Do(func() {
		parsedDefaults = make(ConfigMap)
		if CompiledDefaults == "" {
			return
		}
		pairs := strings.Split(CompiledDefaults, ",")
		for _, pair := range pairs {
			parts := strings.SplitN(pair, "=", 2)
			if len(parts) == 2 {
				SetConfigValue(parsedDefaults, strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1]))
			}
		}
	})
	return parsedDefaults
}

func ResolveConfig(key string, inlineCfg, projectCfg ConfigMap) string {

	if inlineCfg != nil {
		if val, ok := GetConfigValue(inlineCfg, key); ok && val != "" {
			return val
		}
	}

	envKey := brand.EnvPrefix() + "_" + strings.ToUpper(strings.ReplaceAll(key, ".", "_"))
	if val := os.Getenv(envKey); val != "" {
		return val
	}

	if projectCfg != nil {
		if val, ok := GetConfigValue(projectCfg, key); ok && val != "" {
			return val
		}
	}

	if val, ok, _ := GetGlobalConfigValue(key); ok && val != "" {
		return val
	}

	defs := getCompiledDefaults()
	if val, ok := GetConfigValue(defs, key); ok && val != "" {
		return val
	}

	return ""
}

func ResolveIDE(flagValue string, inlineCfg, projectCfg ConfigMap) string {

	if flagValue != "" {
		return flagValue
	}

	if val := ResolveConfig("ide", inlineCfg, projectCfg); val != "" {
		return val
	}

	return "claude"
}

func ResolveProjectIDE(flagValue string, inlineCfg, projectCfg ConfigMap, lockfileIDEs []string) string {

	if flagValue != "" {
		return flagValue
	}

	if inlineCfg != nil {
		if val, ok := GetConfigValue(inlineCfg, "ide"); ok && val != "" {
			return val
		}
	}

	if projectCfg != nil {
		if val, ok := GetConfigValue(projectCfg, "ide"); ok && val != "" {
			return val
		}
	}

	resolved := resolveAmbientIDE()

	if len(lockfileIDEs) > 0 {
		for _, registered := range lockfileIDEs {
			if strings.EqualFold(registered, resolved) {
				return resolved
			}
		}

		return lockfileIDEs[0]
	}

	return resolved
}

func resolveAmbientIDE() string {
	envKey := brand.EnvPrefix() + "_IDE"
	if val := os.Getenv(envKey); val != "" {
		return val
	}

	if val, ok, _ := GetGlobalConfigValue("ide"); ok && val != "" {
		return val
	}

	defs := getCompiledDefaults()
	if val, ok := GetConfigValue(defs, "ide"); ok && val != "" {
		return val
	}

	return "claude"
}

func DefaultIDE() string {
	return ResolveIDE("", nil, nil)
}

func ResolveCLI(flagValue string, inlineCfg, projectCfg ConfigMap, resolvedIDE string) string {

	if flagValue != "" {
		return flagValue
	}

	if val := ResolveConfig("cli", inlineCfg, projectCfg); val != "" {
		return val
	}

	if resolvedIDE != "" {
		if cli := CLIForIDE(resolvedIDE); cli != "" {
			return cli
		}
	}

	return "claude"
}

func DefaultCLI() string {
	return ResolveCLI("", nil, nil, DefaultIDE())
}

func CLIForIDE(ide string) string {
	switch strings.ToLower(ide) {
	case "antigravity":
		return "agy"
	case "gemini", "gemini-code":
		return "gemini"
	case "claude", "claude-code":
		return "claude"
	case "cursor":
		return "cursor-agent"
	case "codex":
		return "codex"
	case "opencode":
		return "opencode"
	case "kiro":
		return "kiro-cli"
	default:
		return ""
	}
}

func ResolveHubRepo(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("hub.repo", inlineCfg, projectCfg)
}

func HubRepoURL() string {
	return ResolveHubRepo(nil, nil)
}

func IsSetupDone() bool {
	path, err := globalConfigPath()
	if err != nil {
		return false
	}
	_, err = os.Stat(path)
	return err == nil
}

func ResolveMemoryRepo(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig("memory.repo", inlineCfg, projectCfg)
}

func MemoryRepoURL() string {
	return ResolveMemoryRepo(nil, nil)
}

func MemoryRepoDirPath() (string, error) {
	dir, err := AppDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "memory"), nil
}

func ResolveDocsDir(inlineCfg, projectCfg ConfigMap) string {
	val := ResolveConfig("knowledge.docs_dir", inlineCfg, projectCfg)
	if val != "" {
		return val
	}
	return "."
}

var defaultKnowledgeExtensions = []string{
	".md", ".markdown", ".mdx",
	".txt", ".adoc", ".rst",
	".puml", ".plantuml",
	".yaml", ".yml", ".json",
	".proto", ".graphql", ".gql",
	".wsdl", ".xml",
}

// ResolveKnowledgeExtensions returns the set of file extensions the knowledge
// wiki should index. Configurable via knowledge.extensions (comma-separated).
// Falls back to the built-in default set.
func ResolveKnowledgeExtensions(inlineCfg, projectCfg ConfigMap) map[string]bool {
	val := ResolveConfig("knowledge.extensions", inlineCfg, projectCfg)

	var exts []string
	if val != "" {
		for _, e := range strings.Split(val, ",") {
			e = strings.TrimSpace(strings.ToLower(e))
			if e == "" {
				continue
			}
			if !strings.HasPrefix(e, ".") {
				e = "." + e
			}
			exts = append(exts, e)
		}
	}

	if len(exts) == 0 {
		exts = defaultKnowledgeExtensions
	}

	m := make(map[string]bool, len(exts))
	for _, e := range exts {
		m[e] = true
	}
	return m
}

var AllModuleNames = []string{
	"knowledge", "ast", "hub", "memory", "improvements",
}

var OptInModules = []string{
	"dream",
}

func IsModuleDisabled(module string, inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("modules."+module, inlineCfg, projectCfg)

	if strings.EqualFold(val, "false") {
		return true
	}

	if strings.EqualFold(val, "true") {
		return false
	}

	if isOptInModule(module) {
		return true
	}

	return false
}

func ResolveIndexSource(inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("ast.index_source", inlineCfg, projectCfg)
	return !strings.EqualFold(val, "false")
}

// ParseGrammarOverrides parses a comma-separated grammar override string
// into a map[string]string. Format: ".ext=grammar-name,.ext2=grammar-name2".
// Returns nil if s is empty or contains no valid pairs.
func ParseGrammarOverrides(s string) map[string]string {
	if s == "" {
		return nil
	}
	m := make(map[string]string)
	for _, pair := range strings.Split(s, ",") {
		parts := strings.SplitN(strings.TrimSpace(pair), "=", 2)
		if len(parts) != 2 {
			continue
		}
		ext := strings.TrimSpace(parts[0])
		name := strings.TrimSpace(parts[1])
		if ext == "" || name == "" {
			continue
		}
		if !strings.HasPrefix(ext, ".") {
			ext = "." + ext
		}
		m[strings.ToLower(ext)] = name
	}
	if len(m) == 0 {
		return nil
	}
	return m
}

// ResolveGrammarOverrides returns the grammar override map from config.
// Configurable via ast.grammar (comma-separated .ext=grammar-name pairs).
// Uses the standard resolution chain: inline → env → project → global → defaults.
// Returns nil if no overrides are configured.
func ResolveGrammarOverrides(inlineCfg, projectCfg ConfigMap) map[string]string {
	val := ResolveConfig("ast.grammar", inlineCfg, projectCfg)
	return ParseGrammarOverrides(val)
}

// MergeGrammarOverrides merges base overrides with higher-priority overrides.
// Priority entries overwrite base entries for the same extension.
// Returns nil if both inputs are nil.
func MergeGrammarOverrides(base, priority map[string]string) map[string]string {
	if base == nil && priority == nil {
		return nil
	}
	if base == nil {
		return priority
	}
	if priority == nil {
		return base
	}
	merged := make(map[string]string, len(base)+len(priority))
	for k, v := range base {
		merged[k] = v
	}
	for k, v := range priority {
		merged[k] = v
	}
	return merged
}


func HubRepoDirPath() (string, error) {
	dir, err := AppDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "hub"), nil
}



func splitKey(dotKey string) (section, key string, nested bool) {
	parts := strings.SplitN(dotKey, ".", 2)
	if len(parts) == 2 && parts[0] != "" && parts[1] != "" {
		return parts[0], parts[1], true
	}
	return dotKey, "", false
}

func isOptInModule(module string) bool {
	for _, m := range OptInModules {
		if strings.EqualFold(m, module) {
			return true
		}
	}
	return false
}
