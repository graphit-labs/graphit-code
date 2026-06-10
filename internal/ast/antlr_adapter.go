package ast

import (
	"fmt"
	"strings"

	"github.com/antlr4-go/antlr/v4"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/plsql"
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

	// 1. Parse using native ANTLR PL/SQL parser
	var antlrTree *antlrcommon.TreeNode
	if cfg.Grammar == "antlr-plsql" {
		preprocessed := plsql.Preprocess(string(src))
		input := antlr.NewInputStream(preprocessed)
		lexer := plsql.NewPlSqlLexer(input)
		lexer.RemoveErrorListeners()
		tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)

		p := plsql.NewPlSqlParser(tokens)
		p.RemoveErrorListeners()

		nativeTree := plsql.ParseSLLThenLL(
			lexer,
			func() antlr.ParseTree { return p.Sql_script() },
			func(mode int) { plsql.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
		)

		if nativeTree == nil {
			return nil, fmt.Errorf("native antlr parse plsql failed")
		}

		antlrTree = convertParseTree(nativeTree, p.RuleNames, p.SymbolicNames, p.LiteralNames)
	} else {
		return nil, fmt.Errorf("unsupported native ANTLR grammar: %s", cfg.Grammar)
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

func convertParseTree(node antlr.Tree, ruleNames, symbolicNames, literalNames []string) *antlrcommon.TreeNode {
	switch n := node.(type) {
	case antlr.TerminalNode:
		tok := n.GetSymbol()
		if tok.GetTokenType() == antlr.TokenEOF {
			return nil
		}

		name := tokenDisplayName(tok.GetTokenType(), symbolicNames, literalNames)
		text := tok.GetText()
		endCol := tok.GetColumn() + len(text) - 1
		return &antlrcommon.TreeNode{
			Token: name,
			Text:  text,
			Start: [2]int{tok.GetLine(), tok.GetColumn()},
			End:   [2]int{tok.GetLine(), endCol},
		}

	case antlr.ParserRuleContext:
		var ruleName string
		ruleIdx := n.GetRuleIndex()
		if ruleIdx >= 0 && ruleIdx < len(ruleNames) {
			ruleName = ruleNames[ruleIdx]
		}

		startLine, startCol := 0, 0
		start := n.GetStart()
		if start != nil {
			startLine, startCol = start.GetLine(), start.GetColumn()
		}

		endLine, endCol := 0, 0
		stop := n.GetStop()
		if stop != nil {
			endLine = stop.GetLine()
			endCol = stop.GetColumn() + len(stop.GetText()) - 1
		}

		var children []*antlrcommon.TreeNode
		antlrChildren := n.GetChildren()
		for _, child := range antlrChildren {
			if t, ok := child.(antlr.TerminalNode); ok {
				if t.GetSymbol().GetTokenType() == antlr.TokenEOF {
					continue
				}
			}
			converted := convertParseTree(child, ruleNames, symbolicNames, literalNames)
			if converted != nil {
				children = append(children, converted)
			}
		}

		return &antlrcommon.TreeNode{
			Rule:     ruleName,
			Start:    [2]int{startLine, startCol},
			End:      [2]int{endLine, endCol},
			Children: children,
		}
	}
	return nil
}

func tokenDisplayName(tokenType int, symbolicNames, literalNames []string) string {
	if tokenType >= 0 && tokenType < len(literalNames) && literalNames[tokenType] != "" {
		return literalNames[tokenType]
	}
	if tokenType >= 0 && tokenType < len(symbolicNames) && symbolicNames[tokenType] != "" {
		return symbolicNames[tokenType]
	}
	return fmt.Sprintf("%d", tokenType)
}
