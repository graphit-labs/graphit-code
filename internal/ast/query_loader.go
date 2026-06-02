package ast

import (
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
	sitter "github.com/smacker/go-tree-sitter"
	"gopkg.in/yaml.v3"
)

// ExternalQueryFile represents a YAML file with custom tree-sitter queries
// for a specific language. Files are loaded from .graphit/ast/queries/.
type ExternalQueryFile struct {
	Language   string             `yaml:"language"`
	Extensions []string           `yaml:"extensions,omitempty"`
	Replace    bool               `yaml:"replace"`
	Queries    []ExternalQueryDef `yaml:"queries"`
}

// ExternalQueryDef represents a single tree-sitter query pattern from an
// external YAML file. NameCapture defaults to "name" when omitted.
type ExternalQueryDef struct {
	DataKey     string `yaml:"data_key"`
	GraphLabel  string `yaml:"graph_label"`
	Pattern     string `yaml:"pattern"`
	NameCapture string `yaml:"name_capture,omitempty"`
}

// ---------------------------------------------------------------------------
// Directory paths
// ---------------------------------------------------------------------------

// userQueriesDir returns the user-editable global queries directory:
// ~/.graphit/ast/queries/
// This directory is NEVER written to by the framework — only the user modifies it.
func userQueriesDir() string {
	d := brand.GlobalDir()
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast", "queries")
}

// runtimeQueriesDir returns the version-scoped runtime queries directory:
// ~/.graphit/runtime/<version>/ast/queries/
// This directory is managed by the framework and overwritten on each version.
func runtimeQueriesDir() string {
	d := brand.RuntimeDir(version.Version)
	if d == "" {
		return ""
	}
	return filepath.Join(d, "ast", "queries")
}

// projectQueriesDir returns the project-level queries directory.
func projectQueriesDir(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
}

// ---------------------------------------------------------------------------
// Runtime defaults extraction
// ---------------------------------------------------------------------------

// EnsureDefaultQueries writes embedded default query files to the runtime
// directory (~/.graphit/runtime/<version>/ast/queries/).
// Files are ALWAYS overwritten because the runtime dir is version-scoped —
// each binary version gets its own clean set of defaults.
func EnsureDefaultQueries() error {
	dir := runtimeQueriesDir()
	if dir == "" {
		return nil
	}

	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("create runtime queries dir: %w", err)
	}

	entries, err := fs.ReadDir(embeddedQueryFS, "queries")
	if err != nil {
		return fmt.Errorf("read embedded queries: %w", err)
	}

	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		destPath := filepath.Join(dir, name)

		data, err := fs.ReadFile(embeddedQueryFS, "queries/"+name)
		if err != nil {
			slog.Warn("skip embedded query: read error", "name", name, "error", err)
			continue
		}

		// Always overwrite — runtime dir is version-scoped
		if err := os.WriteFile(destPath, data, 0o644); err != nil {
			slog.Warn("skip embedded query: write error", "name", name, "error", err)
			continue
		}
	}

	return nil
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

// loadQueriesFromEmbed loads query files from the embedded filesystem.
func loadQueriesFromEmbed() []ExternalQueryFile {
	entries, err := fs.ReadDir(embeddedQueryFS, "queries")
	if err != nil {
		return nil
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

		data, err := fs.ReadFile(embeddedQueryFS, "queries/"+name)
		if err != nil {
			continue
		}

		if qf, ok := parseQueryFile(data, "embedded:"+name); ok {
			result = append(result, qf)
		}
	}

	return result
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

	// Validate individual queries
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

	if len(qf.Queries) == 0 {
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
// Merging
// ---------------------------------------------------------------------------

// toTSQueryDefs converts external query definitions to internal tsQueryDef
// format used by the tree-sitter adapter.
func toTSQueryDefs(external []ExternalQueryDef) []tsQueryDef {
	result := make([]tsQueryDef, 0, len(external))
	for _, eq := range external {
		result = append(result, tsQueryDef(eq))
	}
	return result
}

// MergeQueries merges external query definitions into built-in queries for a
// given language and extension. When replace is true, built-in queries are
// completely replaced. When false, external queries are appended.
//
// Invalid patterns (those that fail tree-sitter compilation) are logged and
// skipped.
func MergeQueries(builtIn []tsQueryDef, externals []ExternalQueryFile, lang string, ext string, tsLang *sitter.Language) []tsQueryDef {
	var applicable []ExternalQueryFile
	for _, ef := range externals {
		if ef.Language != lang {
			continue
		}
		// If the external file specifies extensions, only apply if our ext matches
		if len(ef.Extensions) > 0 {
			found := false
			for _, e := range ef.Extensions {
				if strings.EqualFold(e, ext) {
					found = true
					break
				}
			}
			if !found {
				continue
			}
		}
		applicable = append(applicable, ef)
	}

	if len(applicable) == 0 {
		return builtIn
	}

	// Check if any applicable file wants full replacement
	replace := false
	for _, ef := range applicable {
		if ef.Replace {
			replace = true
			break
		}
	}

	var base []tsQueryDef
	if !replace {
		base = make([]tsQueryDef, len(builtIn))
		copy(base, builtIn)
	}

	// Append all external queries, validating patterns
	for _, ef := range applicable {
		for _, eq := range ef.Queries {
			qd := tsQueryDef(eq)

			// Validate pattern compiles
			if tsLang != nil {
				q, err := sitter.NewQuery([]byte(qd.Pattern), tsLang)
				if err != nil {
					slog.Warn("skip external query: invalid pattern",
						"language", lang, "data_key", qd.DataKey, "error", err)
					continue
				}
				q.Close()
			}

			base = append(base, qd)
		}
	}

	return base
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

// embeddedQueriesOnce ensures embedded queries are loaded only once.
var embeddedQueriesOnce sync.Once
var embeddedQueriesCache []ExternalQueryFile

// ensureOnce ensures default queries are extracted only once per process.
var ensureOnce sync.Once

// resetQueryCaches clears all cached queries. Used by tests.
func resetQueryCaches() {
	externalQueryCache = sync.Map{}
	mergedQueryCache = sync.Map{}
	userQueriesOnce = sync.Once{}
	userQueriesCache = nil
	runtimeQueriesOnce = sync.Once{}
	runtimeQueriesCache = nil
	embeddedQueriesOnce = sync.Once{}
	embeddedQueriesCache = nil
	ensureOnce = sync.Once{}
}

// loadEmbeddedCached loads embedded default queries (once).
func loadEmbeddedCached() []ExternalQueryFile {
	embeddedQueriesOnce.Do(func() {
		embeddedQueriesCache = loadQueriesFromEmbed()
	})
	return embeddedQueriesCache
}

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
//	project > user global > runtime > embedded
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

	// 3. Check runtime (~/.graphit/runtime/<version>/ast/queries/) — framework defaults
	runtimeQ := loadRuntimeCached()
	runtimeMatch := filterByLangExt(runtimeQ, lang, ext)
	if len(runtimeMatch) > 0 {
		return runtimeMatch
	}

	// 4. Fall back to embedded (compiled into binary)
	embeddedQ := loadEmbeddedCached()
	return filterByLangExt(embeddedQ, lang, ext)
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
// language, extension, and built-in queries. Results are cached.
//
// Resolution order:
//
//	project > user global > runtime > embedded > hardcoded Go
//
// The external queries (from whichever level wins) completely replace the
// built-in Go queries when found. Built-in Go queries serve as last-resort
// fallback only.
func mergedQueriesFor(projectDir, lang, ext string, builtIn []tsQueryDef, tsLang *sitter.Language) []tsQueryDef {
	cacheKey := projectDir + "|" + lang + "|" + ext
	if cached, ok := mergedQueryCache.Load(cacheKey); ok {
		return cached.([]tsQueryDef)
	}

	// Ensure defaults are extracted to global dir on first use
	ensureOnce.Do(func() {
		if err := EnsureDefaultQueries(); err != nil {
			slog.Warn("failed to ensure default queries", "error", err)
		}
	})

	resolved := resolveQueriesForLang(projectDir, lang, ext)
	if len(resolved) == 0 {
		// No external queries at any level — use hardcoded Go fallback
		return builtIn
	}

	// Convert resolved external queries to tsQueryDef, validating patterns
	var result []tsQueryDef
	for _, ef := range resolved {
		for _, eq := range ef.Queries {
			qd := tsQueryDef(eq)
			if tsLang != nil {
				q, err := sitter.NewQuery([]byte(qd.Pattern), tsLang)
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

	if len(result) == 0 {
		return builtIn
	}

	mergedQueryCache.Store(cacheKey, result)
	return result
}
