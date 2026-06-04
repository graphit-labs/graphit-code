package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmts"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
	"gopkg.in/yaml.v3"
)

// ExternalQueryFile represents a YAML file with custom tree-sitter queries
// and language configuration. Files are loaded from .graphit/ast/queries/.
type ExternalQueryFile struct {
	Language   string             `yaml:"language"`
	Extensions []string           `yaml:"extensions,omitempty"`
	Replace    bool               `yaml:"replace"`
	Queries    []ExternalQueryDef `yaml:"queries"`

	// Language-level configuration (all optional — engine uses sensible defaults)
	Exports          *ExportConfig     `yaml:"exports,omitempty"`
	SelfKeywords     []string          `yaml:"self_keywords,omitempty"`
	ContextTypes     map[string]string `yaml:"context_types,omitempty"`
	AnonFuncTypes    []string          `yaml:"anon_func_types,omitempty"`
	DeclarationTypes []string          `yaml:"declaration_types,omitempty"`
	CommentTypes     []string          `yaml:"comment_types,omitempty"`

	// Entry point scoring — language-level base rules (merged with framework rules)
	EntryPoints *EntryPointConfig `yaml:"entry_points,omitempty"`
	// Import detection — language-level import patterns for framework detection
	ImportDetection []ImportRule `yaml:"import_detection,omitempty"`
}

// ExportConfig defines how the engine determines export/visibility for a language.
type ExportConfig struct {
	// Strategy is one of: capitalized_name, no_prefix, modifier, export_statement,
	// no_modifier, no_static, none.
	Strategy string            `yaml:"strategy"`
	Config   map[string]string `yaml:"config,omitempty"`
	ConfigList map[string][]string `yaml:"config_list,omitempty"`
}


type ExternalQueryDef struct {
	DataKey      string `yaml:"data_key"`
	GraphLabel   string `yaml:"graph_label"`
	Pattern      string `yaml:"pattern"`
	NameCapture  string `yaml:"name_capture,omitempty"`
	Type         string `yaml:"type,omitempty"`          // "entity" (default) or "relation"
	RelationType string `yaml:"relation_type,omitempty"` // e.g. CALLS, INHERITS, READS_FIELD
}

// ---------------------------------------------------------------------------
// Framework definitions
// ---------------------------------------------------------------------------

// FrameworkFile represents a YAML file with framework detection rules
// and entry point scoring overrides. Files are loaded from .graphit/ast/frameworks/.
type FrameworkFile struct {
	Framework string   `yaml:"framework"`
	Languages []string `yaml:"languages,omitempty"`

	DecoratorDetection []DecoratorRule   `yaml:"decorator_detection,omitempty"`
	HeritageDetection  []HeritageRule    `yaml:"heritage_detection,omitempty"`
	ImportDetection    []ImportRule      `yaml:"import_detection,omitempty"`
	EntryPoints        *EntryPointConfig `yaml:"entry_points,omitempty"`
}

// DecoratorRule maps a decorator name to a framework category.
type DecoratorRule struct {
	Name     string `yaml:"name"`
	Category string `yaml:"category"`
	// FrameworkName overrides the parent FrameworkFile.Framework name.
	FrameworkName string `yaml:"framework,omitempty"`
}

// HeritageRule maps a parent class/interface name to a framework category.
type HeritageRule struct {
	Parent   string `yaml:"parent"`
	Category string `yaml:"category"`
	// FrameworkName overrides the parent FrameworkFile.Framework name.
	FrameworkName string `yaml:"framework,omitempty"`
}

// ImportRule matches import paths to detect framework usage.
type ImportRule struct {
	Pattern  string `yaml:"pattern"`
	Match    string `yaml:"match"`
	Category string `yaml:"category"`
	// FrameworkName overrides the parent FrameworkFile.Framework name.
	FrameworkName string `yaml:"framework,omitempty"`
}

// EntryPointConfig defines entry point scoring rules for a framework.
type EntryPointConfig struct {
	// Name-based scoring (glob patterns: "main", "Test*", "*Handler")
	Names []NameScoreRule `yaml:"names,omitempty"`
	// Decorator-based scoring
	Decorators []DecoratorScoreRule `yaml:"decorators,omitempty"`
	// Bonus for exported functions
	ExportedBonus int `yaml:"exported_bonus,omitempty"`
	// Maximum score cap
	MaxScore int `yaml:"max_score,omitempty"`
}

// NameScoreRule scores functions by name pattern.
type NameScoreRule struct {
	Pattern string `yaml:"pattern"`
	Score   int    `yaml:"score"`
}

// DecoratorScoreRule scores functions by decorator name.
type DecoratorScoreRule struct {
	Name  string `yaml:"name"`
	Score int    `yaml:"score"`
}

// ---------------------------------------------------------------------------
// Ecosystem definitions
// ---------------------------------------------------------------------------

// EcosystemFile represents the ecosystems.yaml configuration.
type EcosystemFile struct {
	ConfigFiles []EcosystemEntry `yaml:"config_files"`
}

// EcosystemEntry maps a config filename to a language and ecosystem.
type EcosystemEntry struct {
	Filename  string `yaml:"filename"`
	Language  string `yaml:"language"`
	Ecosystem string `yaml:"ecosystem"`
	Glob      bool   `yaml:"glob,omitempty"`
	// Extract allows extracting metadata from file content.
	Extract []EcosystemExtract `yaml:"extract,omitempty"`
}

// EcosystemExtract defines a field to extract from a config file.
type EcosystemExtract struct {
	Field string `yaml:"field"` // JSON field path
	Store string `yaml:"store"` // key to store in detected map
}

// ---------------------------------------------------------------------------
// Directory paths
// ---------------------------------------------------------------------------

// userASTDir returns the user-editable global AST directory: ~/.graphit/ast/
func userASTDir() string {
	d := brand.GlobalDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast")
}

// runtimeASTDir returns the version-scoped runtime AST directory.
func runtimeASTDir() string {
	d := brand.RuntimeDir(version.Version)
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast")
}

// projectASTDir returns the project-level AST directory.
func projectASTDir(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "ast")
}

func userQueriesDir() string {
	d := userASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "queries")
}

func runtimeQueriesDir() string {
	d := runtimeASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "queries")
}

func projectQueriesDir(projectDir string) string {
	return filepath.Join(projectASTDir(projectDir), "queries")
}

func userFrameworksDir() string {
	d := userASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "frameworks")
}

func runtimeFrameworksDir() string {
	d := runtimeASTDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "frameworks")
}

func projectFrameworksDir(projectDir string) string {
	return filepath.Join(projectASTDir(projectDir), "frameworks")
}



// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

// loadQueriesFromDir scans *.yaml / *.yml files in a directory and returns
// all valid external query files. Invalid entries are logged and skipped.
func loadQueriesFromDir(dir string) ([]ExternalQueryFile, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("read queries dir: %w", err)
	}

	var result []ExternalQueryFile
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if ext != ".yaml" && ext != ".yml" {
			continue
		}

		path := filepath.Join(dir, name)
		data, err := os.ReadFile(path)
		if err != nil {
			slog.Warn("skip external query file: read error", "path", path, "error", err)
			continue
		}

		qf, ok := parseQueryFile(data, path)
		if ok {
			result = append(result, qf)
		}
	}

	return result, nil
}



// parseQueryFile parses and validates a single YAML query file.
func parseQueryFile(data []byte, sourcePath string) (ExternalQueryFile, bool) {
	var qf ExternalQueryFile
	if err := yaml.Unmarshal(data, &qf); err != nil {
		slog.Warn("skip external query file: YAML parse error", "path", sourcePath, "error", err)
		return qf, false
	}

	if qf.Language == "" {
		slog.Warn("skip external query file: missing 'language' field", "path", sourcePath)
		return qf, false
	}


	var valid []ExternalQueryDef
	for i, q := range qf.Queries {
		if q.DataKey == "" {
			slog.Warn("skip query: missing 'data_key'", "path", sourcePath, "index", i)
			continue
		}
		if q.Pattern == "" {
			slog.Warn("skip query: missing 'pattern'", "path", sourcePath, "index", i, "data_key", q.DataKey)
			continue
		}
		if q.NameCapture == "" {
			q.NameCapture = "name"
		}
		valid = append(valid, q)
	}
	qf.Queries = valid

	if len(qf.Queries) == 0 && !hasLangConfig(&qf) {
		return qf, false
	}

	return qf, true
}

// LoadExternalQueries scans .graphit/ast/queries/*.yaml in the given project
// directory and returns all valid external query files. This is the project-level
// loader. For the full resolution chain, use resolveQueries.
func LoadExternalQueries(projectDir string) ([]ExternalQueryFile, error) {
	return loadQueriesFromDir(projectQueriesDir(projectDir))
}

// LoadUserQueries loads query files from the user-editable global directory:
// ~/.graphit/ast/queries/
func LoadUserQueries() ([]ExternalQueryFile, error) {
	dir := userQueriesDir()
	if dir == "" {
		return nil, nil
	}
	return loadQueriesFromDir(dir)
}

// LoadRuntimeQueries loads query files from the version-scoped runtime directory:
// ~/.graphit/runtime/<version>/ast/queries/
func LoadRuntimeQueries() ([]ExternalQueryFile, error) {
	dir := runtimeQueriesDir()
	if dir == "" {
		return nil, nil
	}
	return loadQueriesFromDir(dir)
}

// ---------------------------------------------------------------------------
// Caching & Resolution
// ---------------------------------------------------------------------------

// externalQueryCache caches loaded external queries per project directory
// to avoid re-reading YAML files on every parse call.
var externalQueryCache sync.Map // map[string][]ExternalQueryFile

// mergedQueryCache caches the merged queries per (projectDir, lang, ext) key.
var mergedQueryCache sync.Map // map[string][]tsQueryDef

// userQueriesOnce ensures user global queries are loaded only once.
var userQueriesOnce sync.Once
var userQueriesCache []ExternalQueryFile

// runtimeQueriesOnce ensures runtime queries are loaded only once.
var runtimeQueriesOnce sync.Once
var runtimeQueriesCache []ExternalQueryFile



// loadRuntimeCached loads runtime queries from
// ~/.graphit/runtime/<version>/ast/queries/ (once).
func loadRuntimeCached() []ExternalQueryFile {
	runtimeQueriesOnce.Do(func() {
		rq, err := LoadRuntimeQueries()
		if err != nil {
			slog.Warn("runtime query load error", "error", err)
		}
		runtimeQueriesCache = rq
	})
	return runtimeQueriesCache
}

// loadUserCached loads user global queries from ~/.graphit/ast/queries/ (once).
func loadUserCached() []ExternalQueryFile {
	userQueriesOnce.Do(func() {
		uq, err := LoadUserQueries()
		if err != nil {
			slog.Warn("user query load error", "error", err)
		}
		userQueriesCache = uq
	})
	return userQueriesCache
}

// loadProjectCached loads project-level queries, using cached results.
func loadProjectCached(projectDir string) []ExternalQueryFile {
	if cached, ok := externalQueryCache.Load(projectDir); ok {
		return cached.([]ExternalQueryFile)
	}

	externals, err := LoadExternalQueries(projectDir)
	if err != nil {
		slog.Warn("external query load error", "dir", projectDir, "error", err)
		externals = nil
	}

	externalQueryCache.Store(projectDir, externals)
	return externals
}

// resolveQueriesForLang returns the resolved query files for a given language
// and extension using the precedence chain:
//
//	project > user global > runtime
//
// For each language+extension pair, the highest-priority source that provides
// queries wins. This is per-language override, not merge.
func resolveQueriesForLang(projectDir, lang, ext string) []ExternalQueryFile {
	// 1. Check project-level (.graphit/ast/queries/)
	projectQ := loadProjectCached(projectDir)
	projectMatch := filterByLangExt(projectQ, lang, ext)
	if len(projectMatch) > 0 {
		return projectMatch
	}

	// 2. Check user global (~/.graphit/ast/queries/) — user customizations
	userQ := loadUserCached()
	userMatch := filterByLangExt(userQ, lang, ext)
	if len(userMatch) > 0 {
		return userMatch
	}

	// 3. Check runtime (~/.graphit/runtime/<version>/ast/queries/) — launcher-extracted defaults
	runtimeQ := loadRuntimeCached()
	return filterByLangExt(runtimeQ, lang, ext)
}

// filterByLangExt filters query files that match a language and extension.
func filterByLangExt(files []ExternalQueryFile, lang, ext string) []ExternalQueryFile {
	var result []ExternalQueryFile
	for _, f := range files {
		if f.Language != lang {
			continue
		}
		if len(f.Extensions) > 0 {
			found := false
			for _, e := range f.Extensions {
				if strings.EqualFold(e, ext) {
					found = true
					break
				}
			}
			if !found {
				continue
			}
		}
		result = append(result, f)
	}
	return result
}

// mergedQueriesFor returns the final merged queries for a given project,
// language, and extension. Results are cached.
//
// Resolution order:
//
//	project > user global > runtime
//
// YAML is the only source of queries — there is no hardcoded Go fallback.
func mergedQueriesFor(projectDir, lang, ext string, tsLang *wasmts.Language) []tsQueryDef {
	cacheKey := projectDir + "|" + lang + "|" + ext
	if cached, ok := mergedQueryCache.Load(cacheKey); ok {
		return cached.([]tsQueryDef)
	}

	resolved := resolveQueriesForLang(projectDir, lang, ext)
	if len(resolved) == 0 {
		return nil
	}


	var result []tsQueryDef
	for _, ef := range resolved {
		for _, eq := range ef.Queries {
			qd := tsQueryDef(eq)
			if tsLang != nil {
				q, err := tsLang.NewQuery(qd.Pattern)
				if err != nil {
					slog.Warn("skip resolved query: invalid pattern",
						"language", lang, "data_key", qd.DataKey, "error", err)
					continue
				}
				q.Close()
			}
			result = append(result, qd)
		}
	}

	if len(result) > 0 {
		mergedQueryCache.Store(cacheKey, result)
	}
	return result
}

// hasLangConfig returns true if the file has any language configuration
// sections beyond queries (exports, self_keywords, context_types, etc).
func hasLangConfig(qf *ExternalQueryFile) bool {
	return qf.Exports != nil ||
		len(qf.SelfKeywords) > 0 ||
		len(qf.ContextTypes) > 0 ||
		len(qf.AnonFuncTypes) > 0 ||
		len(qf.DeclarationTypes) > 0 ||
		len(qf.CommentTypes) > 0 ||
		qf.EntryPoints != nil ||
		len(qf.ImportDetection) > 0
}

// ---------------------------------------------------------------------------
// Language Config Resolution
// ---------------------------------------------------------------------------

// resolvedLangConfigFor returns the language configuration for a given language
// and extension. It walks the resolution chain and returns the first file that
// provides the requested language config.
func resolvedLangConfigFor(projectDir, lang, ext string) *ExternalQueryFile {
	resolved := resolveQueriesForLang(projectDir, lang, ext)
	for i := range resolved {
		if hasLangConfig(&resolved[i]) {
			return &resolved[i]
		}
	}
	return nil
}

// ResolveAllLangConfigs returns all language configurations from every level of the
// resolution chain. It collects unique language configs, one per language, using the
// same precedence as queries (project > user > runtime).
func ResolveAllLangConfigs(projectDir string) []*ExternalQueryFile {
	seen := make(map[string]bool)
	var result []*ExternalQueryFile

	sources := [][]ExternalQueryFile{
		loadRuntimeCached(),
		loadUserCached(),
	}
	if projectDir != "" {
		sources = append(sources, loadProjectCached(projectDir))
	}

	for _, files := range sources {
		for _, f := range files {
			if f.Language != "" && !seen[f.Language] {
				seen[f.Language] = true

				ext := ""
				if len(f.Extensions) > 0 {
					ext = f.Extensions[0]
				}
				cfg := resolvedLangConfigFor(projectDir, f.Language, ext)
				if cfg != nil {
					result = append(result, cfg)
				}
			}
		}
	}

	return result
}

// ---------------------------------------------------------------------------
// Framework Loading
// ---------------------------------------------------------------------------

// loadFrameworksFromDir scans *.yaml files in a directory and returns
// all valid framework files.
func loadFrameworksFromDir(dir string) ([]FrameworkFile, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("read frameworks dir: %w", err)
	}

	var result []FrameworkFile
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if ext != ".yaml" && ext != ".yml" {
			continue
		}

		path := filepath.Join(dir, name)
		data, err := os.ReadFile(path)
		if err != nil {
			slog.Warn("skip framework file: read error", "path", path, "error", err)
			continue
		}

		var ff FrameworkFile
		if err := yaml.Unmarshal(data, &ff); err != nil {
			slog.Warn("skip framework file: YAML parse error", "path", path, "error", err)
			continue
		}
		if ff.Framework == "" {
			slog.Warn("skip framework file: missing 'framework' field", "path", path)
			continue
		}
		result = append(result, ff)
	}

	return result, nil
}

// Framework caches
var frameworkCache sync.Map // map[string][]FrameworkFile (projectDir -> frameworks)
var userFrameworksOnce sync.Once
var userFrameworksCache []FrameworkFile
var runtimeFrameworksOnce sync.Once
var runtimeFrameworksCache []FrameworkFile

func loadRuntimeFrameworksCached() []FrameworkFile {
	runtimeFrameworksOnce.Do(func() {
		dir := runtimeFrameworksDir()
		if dir != "" {
			ff, err := loadFrameworksFromDir(dir)
			if err != nil {
				slog.Warn("runtime framework load error", "error", err)
			}
			runtimeFrameworksCache = ff
		}
	})
	return runtimeFrameworksCache
}

func loadUserFrameworksCached() []FrameworkFile {
	userFrameworksOnce.Do(func() {
		dir := userFrameworksDir()
		if dir != "" {
			ff, err := loadFrameworksFromDir(dir)
			if err != nil {
				slog.Warn("user framework load error", "error", err)
			}
			userFrameworksCache = ff
		}
	})
	return userFrameworksCache
}

func loadProjectFrameworksCached(projectDir string) []FrameworkFile {
	if cached, ok := frameworkCache.Load(projectDir); ok {
		return cached.([]FrameworkFile)
	}

	ff, err := loadFrameworksFromDir(projectFrameworksDir(projectDir))
	if err != nil {
		slog.Warn("project framework load error", "dir", projectDir, "error", err)
		ff = nil
	}

	frameworkCache.Store(projectDir, ff)
	return ff
}

// ResolveFrameworks returns all applicable framework files for a project,
// merging from all levels. Unlike queries (which use precedence override),
// frameworks MERGE from all levels — project frameworks extend runtime+user ones.
func ResolveFrameworks(projectDir string) []FrameworkFile {
	var all []FrameworkFile

	// Runtime (base — launcher-extracted defaults)
	all = append(all, loadRuntimeFrameworksCached()...)

	// User global (extends/overrides)
	all = append(all, loadUserFrameworksCached()...)

	// Project (highest priority — extends)
	if projectDir != "" {
		all = append(all, loadProjectFrameworksCached(projectDir)...)
	}

	return all
}

// ---------------------------------------------------------------------------
// Ecosystem Loading
// ---------------------------------------------------------------------------

// loadEcosystemFile loads an ecosystems.yaml file from a path.
func loadEcosystemFile(path string) (*EcosystemFile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var ef EcosystemFile
	if err := yaml.Unmarshal(data, &ef); err != nil {
		return nil, fmt.Errorf("YAML parse error: %w", err)
	}
	return &ef, nil
}


// ResolveEcosystems returns the merged ecosystem entries from all levels.
// Like frameworks, ecosystems MERGE from all levels.
func ResolveEcosystems(projectDir string) []EcosystemEntry {
	var all []EcosystemEntry

	// Runtime (base — launcher-extracted defaults)
	if dir := runtimeASTDir(); dir != "" {
		if ef, err := loadEcosystemFile(filepath.Join(dir, "ecosystems.yaml")); err == nil && ef != nil {
			all = append(all, ef.ConfigFiles...)
		}
	}

	// User global
	if dir := userASTDir(); dir != "" {
		if ef, err := loadEcosystemFile(filepath.Join(dir, "ecosystems.yaml")); err == nil && ef != nil {
			all = append(all, ef.ConfigFiles...)
		}
	}

	// Project
	if projectDir != "" {
		if ef, err := loadEcosystemFile(filepath.Join(projectASTDir(projectDir), "ecosystems.yaml")); err == nil && ef != nil {
			all = append(all, ef.ConfigFiles...)
		}
	}

	return all
}

