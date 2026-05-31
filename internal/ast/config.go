package ast

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"gopkg.in/yaml.v3"
)

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

type ImportedContext struct {
	Name       string `yaml:"name" json:"name"`
	SourcePath string `yaml:"source_path" json:"source_path"`
	DBPath     string `yaml:"db_path" json:"db_path"`
	ImportedAt string `yaml:"imported_at" json:"imported_at"`
}

type Config struct {
	LadybugPath      string                     `yaml:"ladybug_path" json:"ladybug_path"`
	ActiveContext    string                     `yaml:"active_context" json:"active_context"`
	Contexts         map[string]string          `yaml:"contexts" json:"contexts"`
	ImportedContexts map[string]ImportedContext `yaml:"imported_contexts" json:"imported_contexts"`
	OpenAIKey        string                     `yaml:"openai_key,omitempty" json:"-"`
	OpenAIModel      string                     `yaml:"openai_model" json:"openai_model"`
}

var configDir = filepath.Join(brand.DotDir(), "ast")
var configFile = filepath.Join(configDir, "config.yaml")

func DefaultConfig() *Config {
	return &Config{
		LadybugPath:      filepath.Join(configDir, "ladybug_db"),
		Contexts:         make(map[string]string),
		ImportedContexts: make(map[string]ImportedContext),
		OpenAIModel:      "gpt-4o-mini",
	}
}

func LoadConfig() *Config {
	cfg := DefaultConfig()

	data, err := os.ReadFile(configFile)
	if err == nil {
		_ = yaml.Unmarshal(data, cfg)
	}

	if cfg.Contexts == nil {
		cfg.Contexts = make(map[string]string)
	}
	if cfg.ImportedContexts == nil {
		cfg.ImportedContexts = make(map[string]ImportedContext)
	}

	if v := os.Getenv("LADYBUGDB_PATH"); v != "" {
		cfg.LadybugPath = v
	}
	if v := os.Getenv("OPENAI_API_KEY"); v != "" {
		cfg.OpenAIKey = v
	}
	if v := os.Getenv("OPENAI_MODEL"); v != "" {
		cfg.OpenAIModel = v
	}

	return cfg
}

func SaveConfig(cfg *Config) error {
	if err := os.MkdirAll(configDir, 0o755); err != nil {
		return err
	}
	data, err := yaml.Marshal(cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(configFile, data, 0o600)
}

func GetConfigValue(key string) string {
	cfg := LoadConfig()
	switch key {
	case "ladybug_path":
		return cfg.LadybugPath
	case "active_context":
		return cfg.ActiveContext
	case "openai_model":
		return cfg.OpenAIModel
	default:
		return ""
	}
}

func SetConfigValue(key, value string) error {
	cfg := LoadConfig()
	switch key {
	case "ladybug_path":
		cfg.LadybugPath = value
	case "active_context":
		cfg.ActiveContext = value
	case "openai_model":
		cfg.OpenAIModel = value
	}
	return SaveConfig(cfg)
}

func AddContext(name, path string) error {
	cfg := LoadConfig()
	if cfg.Contexts == nil {
		cfg.Contexts = make(map[string]string)
	}
	cfg.Contexts[name] = path
	return SaveConfig(cfg)
}

func SwitchContext(name string) error {
	cfg := LoadConfig()
	if _, ok := cfg.Contexts[name]; !ok {
		return fmt.Errorf("context %q not found", name)
	}
	cfg.ActiveContext = name
	return SaveConfig(cfg)
}

func ListContexts() map[string]string {
	cfg := LoadConfig()
	if cfg.Contexts == nil {
		return map[string]string{}
	}
	return cfg.Contexts
}

func sanitizeContextName(name string) string {
	name = strings.ToLower(strings.TrimSpace(name))
	name = strings.ReplaceAll(name, " ", "-")
	re := regexp.MustCompile(`[^a-z0-9_-]`)
	name = re.ReplaceAllString(name, "")
	if name == "" {
		name = "unnamed"
	}
	return name
}

func globalASTContextDir(sanitized string) string {
	d := brand.GlobalDir()
	if d == "" {

		return filepath.Join(configDir, sanitized)
	}
	return filepath.Join(d, "ast", sanitized)
}

func astContextProjectLinkDir(sanitized string) string {
	return filepath.Join(configDir, sanitized)
}

func ContextDBPath(name string) string {
	sanitized := sanitizeContextName(name)
	return filepath.Join(globalASTContextDir(sanitized), "ladybugdb")
}

func AddImportedContext(name, sourcePath string) (ImportedContext, error) {
	sanitized := sanitizeContextName(name)
	globalDir := globalASTContextDir(sanitized)
	dbPath := filepath.Join(globalDir, "ladybugdb")

	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		return ImportedContext{}, fmt.Errorf("create global context dir: %w", err)
	}

	linkDir := astContextProjectLinkDir(sanitized)
	if err := os.MkdirAll(filepath.Dir(linkDir), 0o755); err == nil {
		_ = paths.SafeSymlink(globalDir, linkDir)
	}

	ictx := ImportedContext{
		Name:       name,
		SourcePath: sourcePath,
		DBPath:     dbPath,
		ImportedAt: time.Now().Format(time.RFC3339),
	}

	cfg := LoadConfig()
	if cfg.ImportedContexts == nil {
		cfg.ImportedContexts = make(map[string]ImportedContext)
	}
	cfg.ImportedContexts[sanitized] = ictx
	if err := SaveConfig(cfg); err != nil {
		return ImportedContext{}, err
	}
	return ictx, nil
}

func RemoveImportedContext(name string) error {
	sanitized := sanitizeContextName(name)
	cfg := LoadConfig()

	if _, ok := cfg.ImportedContexts[sanitized]; !ok {
		return fmt.Errorf("imported context %q not found", name)
	}

	linkDir := astContextProjectLinkDir(sanitized)
	if info, err := os.Lstat(linkDir); err == nil {
		if info.Mode()&os.ModeSymlink != 0 {
			_ = os.Remove(linkDir)
		} else {

			_ = os.RemoveAll(linkDir)
		}
	}

	delete(cfg.ImportedContexts, sanitized)
	return SaveConfig(cfg)
}

func GetImportedContext(name string) (*ImportedContext, error) {
	sanitized := sanitizeContextName(name)
	cfg := LoadConfig()
	ictx, ok := cfg.ImportedContexts[sanitized]
	if !ok {
		return nil, fmt.Errorf("imported context %q not found", name)
	}
	return &ictx, nil
}

func ListImportedContexts() map[string]ImportedContext {
	result := map[string]ImportedContext{}

	astDir := filepath.Join(brand.DotDir(), "ast")
	entries, err := os.ReadDir(astDir)
	if err != nil {
		return result
	}

	projectNames := loadProjectIDNamesFromRegistry()

	for _, entry := range entries {
		name := entry.Name()
		if name == "project" || name == "imports" || name == "export" || strings.HasPrefix(name, ".") || name == "config.yaml" {
			continue
		}

		fullPath := filepath.Join(astDir, name)
		info, err := os.Stat(fullPath)
		if err != nil || !info.IsDir() {
			continue
		}

		dbPath := filepath.Join(fullPath, "ladybugdb")
		if _, err := os.Stat(dbPath); err != nil {
			continue
		}

		displayName := name
		if readable, ok := projectNames[name]; ok {
			displayName = readable
		}

		result[name] = ImportedContext{
			Name:   displayName,
			DBPath: dbPath,
		}
	}

	return result
}

func loadProjectIDNamesFromRegistry() map[string]string {
	names := map[string]string{}
	registryPath := filepath.Join(brand.GlobalDir(), "hub.registry.json")
	data, err := os.ReadFile(registryPath)
	if err != nil {
		return names
	}
	var cache struct {
		Projects map[string]struct {
			Name string `json:"name"`
		} `json:"projects"`
	}
	if json.Unmarshal(data, &cache) == nil {
		for id, proj := range cache.Projects {
			if proj.Name != "" {
				names[id] = proj.Name
			}
		}
	}
	return names
}
