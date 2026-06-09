package ast

import (
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
)

// antlrExtMap maps file extensions to ANTLR language configs.
// antlrGrammarMap maps grammar names (e.g. "antlr-plsql") to configs.
// Both populated during init from YAML query files with parser=antlr4.
var antlrExtMap map[string]*antlrLangConfig
var antlrGrammarMap map[string]*antlrLangConfig

type antlrLangConfig struct {
	Language   string
	Grammar    string // Grammar name (e.g. "antlr-plsql")
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
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfg, ok := antlrExtMap[ext]
	if !ok {
		// Check for local YAML query configurations matching the extension
		langName := strings.TrimPrefix(ext, ".")
		qfs := resolveQueriesForLang(a.projectDir, langName, ext)
		for _, qf := range qfs {
			if qf.Parser == "antlr4" {
				grammar := qf.Grammar
				if grammar == "" {
					grammar = "antlr-" + qf.Language
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

	// 1. Get a pooled WASM module instance for this ANTLR grammar
	mod, cleanup, err := getAntlrLanguage(cfg.Grammar, a.projectDir)
	if err != nil {
		return nil, fmt.Errorf("get antlr language %q: %w", cfg.Grammar, err)
	}
	defer cleanup()

	// 2. Parse via WASM and retrieve JSON tree
	jsonBytes, err := mod.Parse(src)
	if err != nil {
		return nil, fmt.Errorf("wasm parse %q: %w", cfg.Grammar, err)
	}

	// 3. Deserialize JSON to Tree
	antlrTree, err := wasmantlr.ParseTreeFromJSON(jsonBytes)
	if err != nil {
		return nil, fmt.Errorf("failed to parse antlr JSON tree: %w", err)
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
		pattern *wasmantlr.Pattern
	}

	var activeQueries []queryDefAndPattern
	for _, qdef := range rpcQueries {
		pattern, pErr := wasmantlr.CompilePattern(qdef.Pattern)
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

func extractNameFromMatch(node *wasmantlr.TreeNode, nameCapture string) string {
	if nameCapture == "" || nameCapture == "name" {
		return node.FirstTerminalText()
	}

	child := node.ChildByRule(nameCapture)
	if child != nil {
		return child.FirstTerminalText()
	}

	return node.FirstTerminalText()
}

func resolveParentContextAntlr(match wasmantlr.MatchResult, langConfig *ExternalQueryFile) (string, string) {
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
