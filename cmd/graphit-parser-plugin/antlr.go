package main

import (
	"fmt"
	"strings"

	"github.com/antlr4-go/antlr/v4"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/ast/wasmantlr"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql/parser"
	"github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared"
)

func parseAntlr(req ast.ParseRequest) (*ast.ParsedFile, error) {
	preprocessed := Preprocess(string(req.Content))

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

	var out strings.Builder
	out.Grow(len(req.Content) * 2)
	shared.TreeToJSON(&out, tree, shared.ParserMeta{
		RuleNames:     p.RuleNames,
		SymbolicNames: p.SymbolicNames,
		LiteralNames:  p.LiteralNames,
	})

	antlrTree, err := wasmantlr.ParseTreeFromJSON([]byte(out.String()))
	if err != nil {
		return nil, fmt.Errorf("failed to parse antlr JSON tree: %w", err)
	}

	result := &ast.ParsedFile{
		Path:     req.Path,
		Language: req.Language,
		IsDepend: req.IsDepend,
		Source:   string(req.Content),
		Entities: make(map[string][]ast.Entity),
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	for _, qdef := range req.Queries {
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
			complexity := ast.ComputeCyclomaticComplexity(entitySource)

			contextName, contextType := resolveParentContextAntlr(match, req.LangConfig)

			result.AddEntity(qdef.DataKey, ast.Entity{
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

	relationTypes := buildRelationTypeMap(req.Queries)
	attachDecorators(result, relationTypes)

	if req.LangConfig != nil {
		detectExportsAntlr(result, req.Language, req.LangConfig, relationTypes)
	}

	processRelations(result, relationTypes)
	resolveReceiverTypes(result, req.Content, req.Language, req.LangConfig)

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

func resolveParentContextAntlr(match wasmantlr.MatchResult, langConfig *ast.ExternalQueryFile) (string, string) {
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

func detectExportsAntlr(result *ast.ParsedFile, lang string, langConfig *ast.ExternalQueryFile, relationTypes map[string]string) {
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

func Preprocess(raw string) string {
	if len(raw) == 0 {
		return raw
	}
	return stripMviewUsing(raw)
}
