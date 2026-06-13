package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

// antlrDrivers maps grammar names to their GrammarDriver implementations.
// With plug-and-play architecture, these are SidecarDrivers that communicate
// with external sidecar processes.
var antlrDrivers map[string]antlrcommon.GrammarDriver
var antlrDriversOnce sync.Once
var antlrGrammarProjectDir string

// SetAntlrGrammarProjectDir sets the project directory used for ANTLR grammar search.
// Must be called before the first ANTLR parse.
func SetAntlrGrammarProjectDir(dir string) {
	antlrGrammarProjectDir = dir
}

func initAntlrDrivers() {
	antlrDrivers = make(map[string]antlrcommon.GrammarDriver)

	// Search for per-grammar ANTLR sidecar binaries.
	searchDirs := antlrGrammarSearchDirs(antlrGrammarProjectDir)
	grammars := []string{"plsql", "postgresql", "tsql", "db2", "cobol85"}

	for _, grammar := range grammars {
		bin := findAntlrGrammarBin(grammar, searchDirs)
		if bin != "" {
			antlrDrivers["antlr-"+grammar] = NewSidecarDriver(bin, grammar, 2)
		}
	}

	// Fallback: check for monolithic sidecar binary (backward compatibility).
	if len(antlrDrivers) == 0 {
		sidecarBin := findAntlrSidecarBin()
		if sidecarBin != "" {
			for _, grammar := range grammars {
				antlrDrivers["antlr-"+grammar] = NewSidecarDriver(sidecarBin, grammar, 2)
			}
		}
	}

	if len(antlrDrivers) == 0 {
		slog.Warn("no ANTLR grammar binaries found, ANTLR parsing disabled")
	}
}

// findAntlrGrammarBin searches for a per-grammar ANTLR sidecar binary.
func findAntlrGrammarBin(grammar string, searchDirs []string) string {
	candidates := []string{
		fmt.Sprintf("antlr-sidecar-%s", grammar),
	}
	if runtime.GOOS == "windows" {
		candidates = append(candidates, fmt.Sprintf("antlr-sidecar-%s.exe", grammar))
	}

	for _, dir := range searchDirs {
		for _, candidate := range candidates {
			path := filepath.Join(dir, candidate)
			if _, err := os.Stat(path); err == nil {
				return path
			}
		}
	}
	return ""
}

// antlrGrammarSearchDirs returns the ordered list of directories to search
// for per-grammar ANTLR sidecar binaries.
func antlrGrammarSearchDirs(projectDir string) []string {
	var dirs []string

	// 1. Project-level grammars.
	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, brand.DotDir(), "grammars", "antlr"))
	}

	// 2. User-level grammars.
	if home, err := os.UserHomeDir(); err == nil {
		dirs = append(dirs, filepath.Join(home, brand.DotDir(), "grammars", "antlr"))
	}

	// 3. Runtime-versioned grammars.
	runtimeDir := brand.RuntimeDir(version.Version)
	if runtimeDir != "" {
		dirs = append(dirs, filepath.Join(runtimeDir, "grammars", "antlr"))
	}

	// 4. Alongside current binary.
	exe, err := os.Executable()
	if err == nil {
		dirs = append(dirs, filepath.Dir(exe))
	}

	return dirs
}

// findAntlrSidecarBin searches for the monolithic ANTLR sidecar binary
// in standard locations. Used as a fallback when per-grammar binaries
// are not found.
// Search order:
// 1. ANTLR_SIDECAR_BIN environment variable
// 2. ~/.graphit/runtime/<version>/graphit-antlr-sidecar
// 3. Same directory as the running binary
func findAntlrSidecarBin() string {
	// Check env var first.
	if bin := os.Getenv("ANTLR_SIDECAR_BIN"); bin != "" {
		if _, err := os.Stat(bin); err == nil {
			return bin
		}
	}

	// Check runtime dir.
	runtimeDir := brand.RuntimeDir(version.Version)
	if runtimeDir != "" {
		bin := filepath.Join(runtimeDir, "graphit-antlr-sidecar")
		if runtime.GOOS == "windows" {
			bin += ".exe"
		}
		if _, err := os.Stat(bin); err == nil {
			return bin
		}
	}

	// Check alongside current binary.
	exe, err := os.Executable()
	if err == nil {
		bin := filepath.Join(filepath.Dir(exe), "graphit-antlr-sidecar")
		if runtime.GOOS == "windows" {
			bin += ".exe"
		}
		if _, err := os.Stat(bin); err == nil {
			return bin
		}
	}

	return ""
}


// antlrExtMap maps file extensions to ANTLR language configs.
// Multiple grammars may share the same extension (e.g. ".sql"); the first
// grammar that successfully extracts entities wins.
// antlrGrammarMap maps grammar names (e.g. "antlr-plsql") to configs.
var antlrExtMap map[string][]*antlrLangConfig
var antlrGrammarMap map[string]*antlrLangConfig

type antlrLangConfig struct {
	Language   string
	Grammar    string // Grammar name (e.g. "antlr-plsql")
	Extensions []string
	StartRule  string
}

func initAntlrExtMap() {
	antlrExtMap = make(map[string][]*antlrLangConfig)
	antlrGrammarMap = make(map[string]*antlrLangConfig)

	runtimeQ := loadRuntimeCached()
	for _, qf := range runtimeQ {
		if qf.Parser != "antlr4" {
			continue
		}
		grammar := qf.Grammar
		if grammar == "" {
			grammar = "antlr-" + qf.Language
		}
		cfg := &antlrLangConfig{
			Language:   qf.Language,
			Grammar:    grammar,
			Extensions: qf.Extensions,
			StartRule:  qf.StartRule,
		}
		for _, ext := range qf.Extensions {
			antlrExtMap[ext] = append(antlrExtMap[ext], cfg)
		}
		antlrGrammarMap[grammar] = cfg
	}
}

func init() {
	initAntlrExtMap()
}

// AntlrParser implements LanguageParser for ANTLR v4 grammars.
type AntlrParser struct {
	projectDir    string
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfgs := antlrExtMap[ext]

	if len(cfgs) == 0 {
		// Check for local YAML query configurations matching the extension
		langName := strings.TrimPrefix(ext, ".")
		qfs := resolveQueriesForLang(a.projectDir, langName, ext)
		for _, qf := range qfs {
			if qf.Parser == "antlr4" {
				grammar := qf.Grammar
				if grammar == "" {
					grammar = "antlr-" + qf.Language
				}
				cfgs = append(cfgs, &antlrLangConfig{
					Language:   qf.Language,
					Grammar:    grammar,
					Extensions: qf.Extensions,
					StartRule:  qf.StartRule,
				})
			}
		}
		if len(cfgs) == 0 {
			return nil, fmt.Errorf("no ANTLR grammar for %s", ext)
		}
	}

	// Try each grammar; return the first that successfully extracts entities.
	var lastErr error
	for _, cfg := range cfgs {
		pf, err := a.parseWithConfig(path, ext, cfg, isDepend, opts)
		if err == nil && pf != nil && pf.EntityCount() > 0 {
			return pf, nil
		}
		if err != nil {
			lastErr = err
		}
	}
	// If none extracted entities but parsing succeeded, return the last successful parse.
	for _, cfg := range cfgs {
		pf, err := a.parseWithConfig(path, ext, cfg, isDepend, opts)
		if err == nil {
			return pf, nil
		}
	}
	if lastErr != nil {
		return nil, lastErr
	}
	return nil, fmt.Errorf("no ANTLR grammar matched for %s", ext)
}

// ParseWithGrammar parses using a specific ANTLR grammar name (e.g. "antlr-plsql"),
// bypassing the extension-based lookup. Used by CompositeParser for --grammar overrides.
func (a *AntlrParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	cfg, ok := antlrGrammarMap[grammarName]
	if !ok {
		return nil, fmt.Errorf("unknown ANTLR grammar: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	return a.parseWithConfig(path, ext, cfg, isDepend, opts)
}

func (a *AntlrParser) parseWithConfig(path, ext string, cfg *antlrLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	var langConfig *ExternalQueryFile
	var queries []tsQueryDef

	if a.projectDir != "" {
		resolved := resolveQueriesForLang(a.projectDir, cfg.Language, ext)
		for _, qf := range resolved {
			if qf.Parser == "antlr4" {
				langConfig = &qf
				break
			}
		}
		if langConfig != nil {
			for _, eq := range langConfig.Queries {
				queries = append(queries, tsQueryDef(eq))
			}
		}
	}

	var rpcQueries []ExternalQueryDef
	for _, q := range queries {
		rpcQueries = append(rpcQueries, ExternalQueryDef(q))
	}

	// 1. Parse using registered ANTLR grammar driver
	if a.projectDir != "" && antlrGrammarProjectDir == "" {
		antlrGrammarProjectDir = a.projectDir
	}
	antlrDriversOnce.Do(initAntlrDrivers)
	driver, ok := antlrDrivers[cfg.Grammar]
	if !ok {
		return nil, fmt.Errorf("unsupported native ANTLR grammar: %s", cfg.Grammar)
	}
	antlrTree, err := driver.Parse(src)
	if err != nil {
		return nil, err
	}

	result := &ParsedFile{
		Path:     path,
		Language: cfg.Language,
		IsDepend: isDepend,
		Source:   string(src),
		Entities: make(map[string][]Entity),
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	type queryDefAndPattern struct {
		qdef    ExternalQueryDef
		pattern *antlrcommon.Pattern
	}

	var activeQueries []queryDefAndPattern
	for _, qdef := range rpcQueries {
		pattern, pErr := antlrcommon.CompilePattern(qdef.Pattern)
		if pErr != nil {
			continue
		}
		activeQueries = append(activeQueries, queryDefAndPattern{qdef, pattern})
	}

	for _, aq := range activeQueries {
		matches := aq.pattern.Match(antlrTree)
		for _, match := range matches {
			name := extractNameFromMatch(match.Node, aq.qdef.NameCapture)
			if name == "" {
				continue
			}

			if aq.qdef.DataKey == "imports" {
				name = strings.Trim(name, "'\"")
			}

			if !specificLabels[aq.qdef.GraphLabel] && seenNames[name] {
				continue
			}

			startLine := match.Node.StartLine()
			endLine := match.Node.EndLine()

			entitySource := match.Node.FullText()
			if match.Parent != nil {
				entitySource = match.Parent.FullText()
			}
			complexity := ComputeCyclomaticComplexity(entitySource)

			contextName, contextType := resolveParentContextAntlr(match, langConfig)

			result.AddEntity(aq.qdef.DataKey, Entity{
				Name:        name,
				Line:        startLine,
				EndLine:     endLine,
				Source:      entitySource,
				GraphLabel:  aq.qdef.GraphLabel,
				Complexity:  complexity,
				Context:     contextName,
				ContextType: contextType,
			})

			if specificLabels[aq.qdef.GraphLabel] {
				seenNames[name] = true
			}
		}
	}

	relationTypes := buildRelationTypeMap(rpcQueries)
	attachDecorators(result, relationTypes)

	if langConfig != nil {
		detectExportsAntlr(result, cfg.Language, langConfig, relationTypes)
	}

	processRelations(result, relationTypes)
	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	return result, nil
}

func extractNameFromMatch(node *antlrcommon.TreeNode, nameCapture string) string {
	if nameCapture == "" || nameCapture == "name" {
		return node.FirstTerminalText()
	}

	child := node.ChildByRule(nameCapture)
	if child != nil {
		return child.FirstTerminalText()
	}

	return node.FirstTerminalText()
}

func resolveParentContextAntlr(match antlrcommon.MatchResult, langConfig *ExternalQueryFile) (string, string) {
	if langConfig == nil || len(langConfig.ContextTypes) == 0 {
		return "", ""
	}

	if match.Parent == nil {
		return "", ""
	}

	node := match.Parent
	if label, ok := langConfig.ContextTypes[node.Rule]; ok {
		name := node.FirstTerminalText()
		if name != "" {
			return name, label
		}
	}

	return "", ""
}

func detectExportsAntlr(result *ParsedFile, lang string, langConfig *ExternalQueryFile, relationTypes map[string]string) {
	if langConfig.Exports == nil {
		return
	}

	strategy := langConfig.Exports.Strategy
	stratConfig := langConfig.Exports.Config
	stratConfigList := langConfig.Exports.ConfigList

	for dataKey := range result.Entities {
		if _, isRelation := relationTypes[dataKey]; isRelation {
			continue
		}
		for i := range result.Entities[dataKey] {
			e := &result.Entities[dataKey][i]
			if e.GraphLabel == "" || e.Name == "" {
				continue
			}
			if isExported(strategy, e, nil, stratConfig, stratConfigList) {
				if e.Properties == nil {
					e.Properties = make(map[string]string)
				}
				e.Properties["is_exported"] = "true"
			}
		}
	}
}



// HasAntlrForExtension returns true if there's an ANTLR grammar for the extension.
func HasAntlrForExtension(ext string) bool {
	cfgs := antlrExtMap[strings.ToLower(ext)]
	return len(cfgs) > 0
}

// HasParserForExtension returns true if any parser (tree-sitter or ANTLR) handles the extension.
func HasParserForExtension(ext string) bool {
	return HasTreeSitterForExtension(ext) || HasAntlrForExtension(ext)
}

