package ast

import (
	"fmt"
	"strings"

	"github.com/antlr4-go/antlr/v4"
	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql/parser"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared"
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

	preprocessed := antlrPreprocess(string(src))

	input := antlr.NewInputStream(preprocessed)
	lexer := parser.NewPlSqlLexer(input)
	lexer.RemoveErrorListeners()
	tokens := antlr.NewCommonTokenStream(lexer, antlr.TokenDefaultChannel)

	p := parser.NewPlSqlParser(tokens)
	p.RemoveErrorListeners()

	tree := shared.ParseSLLThenLL(
		lexer,
		func() antlr.ParseTree { return p.Sql_script() },
		func(mode int) { shared.ConfigureParser(p, tokens, &p.BuildParseTrees, mode) },
	)

	if tree == nil {
		return nil, fmt.Errorf("antlr parse error")
	}

	antlrTree := convertParseTree(tree, p.RuleNames, p.SymbolicNames, p.LiteralNames)
	if antlrTree == nil {
		return nil, fmt.Errorf("failed to convert antlr parse tree")
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

	for _, qdef := range rpcQueries {
		pattern, pErr := wasmantlr.CompilePattern(qdef.Pattern)
		if pErr != nil {
			continue
		}

		matches := pattern.Match(antlrTree)
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
				Source:      entitySource,
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

func antlrPreprocess(raw string) string {
	if len(raw) == 0 {
		return raw
	}
	return stripMviewUsing(raw)
}

func stripMviewUsing(src string) string {
	var out strings.Builder
	out.Grow(len(src))
	i := 0
	length := len(src)

	for i < length {
		if src[i] == ')' {
			j := i + 1
			for j < length && (src[j] == ' ' || src[j] == '\t' || src[j] == '\n' || src[j] == '\r') {
				j++
			}
			if j < length && matchKeywordAt(src, j, "USING") {
				usingEnd := j + 5
				for usingEnd < length && (src[usingEnd] == ' ' || src[usingEnd] == '\t' || src[usingEnd] == '\n' || src[usingEnd] == '\r') {
					usingEnd++
				}
				if usingEnd < length && src[usingEnd] == '(' {
					depth := 1
					k := usingEnd + 1
					inSq := false
					inDq := false
					for k < length && depth > 0 {
						c := src[k]
						if inSq {
							if c == '\'' && k+1 < length && src[k+1] == '\'' {
								k++
							} else if c == '\'' {
								inSq = false
							}
						} else if inDq {
							if c == '"' {
								inDq = false
							}
						} else {
							switch c {
							case '\'':
								inSq = true
							case '"':
								inDq = true
							case '(':
								depth++
							case ')':
								depth--
							}
						}
						k++
					}
					afterUsing := k
					for afterUsing < length && (src[afterUsing] == ' ' || src[afterUsing] == '\t' || src[afterUsing] == '\n' || src[afterUsing] == '\r') {
						afterUsing++
					}
					if afterUsing < length && (matchKeywordAt(src, afterUsing, "REFRESH") ||
						matchKeywordAt(src, afterUsing, "AS") ||
						matchKeywordAt(src, afterUsing, "BUILD")) {
						out.WriteString(") ")
						i = k
						continue
					}
				}
			}
		}
		out.WriteByte(src[i])
		i++
	}
	return out.String()
}

func matchKeywordAt(src string, pos int, kw string) bool {
	if pos+len(kw) > len(src) {
		return false
	}
	for i := 0; i < len(kw); i++ {
		c := src[pos+i]
		if c >= 'a' && c <= 'z' {
			c -= 32
		}
		if c != kw[i] {
			return false
		}
	}
	if pos+len(kw) < len(src) {
		next := src[pos+len(kw)]
		if (next >= 'A' && next <= 'Z') || (next >= 'a' && next <= 'z') || (next >= '0' && next <= '9') || next == '_' {
			return false
		}
	}
	return true
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

func convertParseTree(node antlr.Tree, ruleNames, symbolicNames, literalNames []string) *wasmantlr.TreeNode {
	switch n := node.(type) {
	case antlr.TerminalNode:
		tok := n.GetSymbol()
		if tok.GetTokenType() == antlr.TokenEOF {
			return nil
		}
		tokenName := tokenDisplayName(tok.GetTokenType(), symbolicNames, literalNames)
		text := tok.GetText()
		endCol := tok.GetColumn() + len(text) - 1

		return &wasmantlr.TreeNode{
			Token: tokenName,
			Text:  text,
			Start: [2]int{tok.GetLine(), tok.GetColumn()},
			End:   [2]int{tok.GetLine(), endCol},
		}

	case antlr.ParserRuleContext:
		ruleName := ""
		ruleIdx := n.GetRuleIndex()
		if ruleIdx >= 0 && ruleIdx < len(ruleNames) {
			ruleName = ruleNames[ruleIdx]
		}

		res := &wasmantlr.TreeNode{
			Rule: ruleName,
		}

		start := n.GetStart()
		if start != nil {
			res.Start = [2]int{start.GetLine(), start.GetColumn()}
		}
		stop := n.GetStop()
		if stop != nil {
			endCol := stop.GetColumn() + len(stop.GetText()) - 1
			res.End = [2]int{stop.GetLine(), endCol}
		}

		children := n.GetChildren()
		if len(children) > 0 {
			res.Children = make([]*wasmantlr.TreeNode, 0, len(children))
			for _, child := range children {
				converted := convertParseTree(child, ruleNames, symbolicNames, literalNames)
				if converted != nil {
					res.Children = append(res.Children, converted)
				}
			}
		}
		return res
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
