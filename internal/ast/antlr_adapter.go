package ast

import (
	"fmt"
	"log/slog"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
)

// antlrExtMap maps file extensions to ANTLR language configs.
// antlrGrammarMap maps grammar names (e.g. "antlr-plsql") to configs.
// Both populated during init from YAML query files with parser=antlr4.
// The YAML is the single source of truth — grammar binaries are loaded lazily.
var antlrExtMap map[string]*antlrLangConfig
var antlrGrammarMap map[string]*antlrLangConfig

type antlrLangConfig struct {
	Language   string
	Grammar    string // Binary name (e.g. "antlr-plsql")
	Extensions []string
	StartRule  string
}

func initAntlrExtMap() {
	antlrExtMap = make(map[string]*antlrLangConfig)
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
			antlrExtMap[ext] = cfg
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
	workerModules *AntlrWorkerModules
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := antlrExtMap[ext]
	if !ok {
		// Try plug-and-play: check for ANTLR grammar configs in the project
		langName := strings.TrimPrefix(ext, ".")
		qfs := resolveQueriesForLang(a.projectDir, langName, ext)
		for _, qf := range qfs {
			if qf.Parser == "antlr4" {
				grammar := qf.Grammar
				if grammar == "" {
					grammar = "antlr-" + qf.Language
				}
				_, err := getAntlrModule(grammar, a.projectDir)
				if err != nil {
					return nil, fmt.Errorf("no ANTLR grammar for %s: %w", ext, err)
				}
				cfg = &antlrLangConfig{
					Language:   qf.Language,
					Grammar:    grammar,
					Extensions: qf.Extensions,
					StartRule:  qf.StartRule,
				}
				break
			}
		}
		if cfg == nil {
			return nil, fmt.Errorf("no ANTLR grammar for %s", ext)
		}
	}

	return a.parseWithConfig(path, ext, cfg, isDepend, opts)
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
	engine, err := getAntlrModule(cfg.Grammar, a.projectDir)
	if err != nil {
		return nil, fmt.Errorf("load ANTLR grammar %s: %w", cfg.Grammar, err)
	}

	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}

	var tree *wasmantlr.TreeNode
	if a.workerModules != nil {
		tree, err = a.workerModules.Parse(cfg.Grammar, src)
	} else {
		tree, err = engine.Parse(cfg.Grammar, src)
	}
	if err != nil {
		return nil, fmt.Errorf("ANTLR parse %s: %w", path, err)
	}

	result := &ParsedFile{
		Path:     path,
		Language: cfg.Language,
		IsDepend: isDepend,
		Source:   string(src),
		Entities: make(map[string][]Entity),
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

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	for _, qdef := range queries {
		pattern, pErr := wasmantlr.CompilePattern(qdef.Pattern)
		if pErr != nil {
			slog.Warn("skip ANTLR query: invalid pattern",
				"language", cfg.Language, "data_key", qdef.DataKey, "error", pErr)
			continue
		}

		matches := pattern.Match(tree)
		for _, match := range matches {
			name := extractNameFromMatch(match.Node, qdef.NameCapture)
			if name == "" {
				continue
			}

			if qdef.DataKey == "imports" {
				name = strings.Trim(name, "'\"")
			}

			if !specificLabels[qdef.GraphLabel] && seenNames[name] {
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

			result.AddEntity(qdef.DataKey, Entity{
				Name:        name,
				Line:        startLine,
				EndLine:     endLine,
				Source:      capEntitySource(entitySource),
				GraphLabel:  qdef.GraphLabel,
				Complexity:  complexity,
				Context:     contextName,
				ContextType: contextType,
			})

			if specificLabels[qdef.GraphLabel] {
				seenNames[name] = true
			}
		}
	}

	relationTypes := buildRelationTypeMap(queries)
	attachDecorators(result, relationTypes)

	if langConfig != nil {
		detectExportsAntlr(result, cfg.Language, langConfig, relationTypes)
	}

	processRelations(result, relationTypes)
	resolveReceiverTypes(result, src, cfg.Language, langConfig)

	return result, nil
}

// extractNameFromMatch extracts the entity name from a matched node.
// It tries name_capture child rule first, then falls back to first terminal.
func extractNameFromMatch(node *wasmantlr.TreeNode, nameCapture string) string {
	if nameCapture == "" || nameCapture == "name" {
		// Default: use the first terminal text of the matched node
		return node.FirstTerminalText()
	}

	// Look for a child rule matching the nameCapture
	child := node.ChildByRule(nameCapture)
	if child != nil {
		return child.FirstTerminalText()
	}

	return node.FirstTerminalText()
}

// resolveParentContextAntlr walks the parse tree upward via match metadata
// to find the enclosing context (class, function, etc).
func resolveParentContextAntlr(match wasmantlr.MatchResult, langConfig *ExternalQueryFile) (string, string) {
	if langConfig == nil || len(langConfig.ContextTypes) == 0 {
		return "", ""
	}

	if match.Parent == nil {
		return "", ""
	}

	// ANTLR JSON trees don't have parent pointers, so context resolution
	// is limited to the immediate parent. For deeper context, the YAML
	// should use patterns that capture the context hierarchy explicitly.
	node := match.Parent
	if label, ok := langConfig.ContextTypes[node.Rule]; ok {
		name := node.FirstTerminalText()
		if name != "" {
			return name, label
		}
	}

	return "", ""
}

// detectExportsAntlr applies export detection strategies for ANTLR-parsed files.
// Delegates to the existing strategy functions (capitalized_name, no_prefix, etc).
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

	delete(result.Entities, "exports")
}

// HasAntlrForExtension returns true if there's an ANTLR grammar for the extension.
func HasAntlrForExtension(ext string) bool {
	_, ok := antlrExtMap[strings.ToLower(ext)]
	return ok
}

// AntlrConfigForGrammar returns the config for a named ANTLR grammar, or nil.
func AntlrConfigForGrammar(name string) *antlrLangConfig {
	return antlrGrammarMap[name]
}

// AntlrLangForExtension returns the language name for an ANTLR-supported extension.
func AntlrLangForExtension(ext string) string {
	if cfg, ok := antlrExtMap[strings.ToLower(ext)]; ok {
		return cfg.Language
	}
	return ""
}

// HasParserForExtension returns true if any parser (tree-sitter or ANTLR) handles the extension.
func HasParserForExtension(ext string) bool {
	return HasTreeSitterForExtension(ext) || HasAntlrForExtension(ext)
}

// AntlrSupportedExtensions returns all file extensions handled by ANTLR grammars.
func AntlrSupportedExtensions() []string {
	var exts []string
	for ext := range antlrExtMap {
		exts = append(exts, ext)
	}
	return exts
}

// AllSupportedExtensions returns all extensions supported by any parser.
func AllSupportedExtensions() []string {
	seen := make(map[string]bool)
	var exts []string
	for ext := range tsExtMap {
		if !seen[ext] {
			seen[ext] = true
			exts = append(exts, ext)
		}
	}
	for ext := range antlrExtMap {
		if !seen[ext] {
			seen[ext] = true
			exts = append(exts, ext)
		}
	}
	return exts
}
