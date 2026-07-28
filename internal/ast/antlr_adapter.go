package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	"github.com/graphit-labs/graphit-code/internal/sysutil"

	antlrCobol85 "github.com/graphit-labs/graphit-code/internal/ast/antlr/cobol85"
	_ "github.com/graphit-labs/graphit-code/internal/ast/antlr/cobol85/preprocessor"
	antlrDB2 "github.com/graphit-labs/graphit-code/internal/ast/antlr/db2"
	antlrPLSQL "github.com/graphit-labs/graphit-code/internal/ast/antlr/plsql"
	antlrPostgreSQL "github.com/graphit-labs/graphit-code/internal/ast/antlr/postgresql"
	antlrTSQL "github.com/graphit-labs/graphit-code/internal/ast/antlr/tsql"
)

var nativeAntlrDrivers = map[string]antlrcommon.GrammarDriver{
	"antlr-plsql":      &antlrPLSQL.Driver{},
	"antlr-postgresql": &antlrPostgreSQL.Driver{},
	"antlr-tsql":       &antlrTSQL.Driver{},
	"antlr-db2":        &antlrDB2.Driver{},
	"antlr-cobol85":    &antlrCobol85.Driver{},
}

// antlrDrivers maps grammar names to their GrammarDriver.
// Project/user sidecar overrides take priority over native drivers.
var antlrDrivers map[string]antlrcommon.GrammarDriver
var antlrDriversOnce sync.Once
var antlrGrammarProjectDir string

func SetAntlrGrammarProjectDir(dir string) {
	antlrGrammarProjectDir = dir
}

// ResetAntlrCaches discards the accumulated ANTLR DFA / prediction-context caches
// of every native grammar. Those caches are package-level, grow with each newly
// seen input pattern and are never evicted (antlr4-go has no ClearDFA), which
// measured ~2 MB of retention per parsed PL/SQL file — unbounded across a large
// repository. Resetting trades a partial cache re-warm for bounded memory.
//
// NOT safe to call while a parse is in flight: it swaps shared static state. Call
// it only at a barrier where no goroutine is parsing.
func ResetAntlrCaches() {
	antlrcommon.ResetAllCaches()
}

func initAntlrDrivers() {
	antlrDrivers = make(map[string]antlrcommon.GrammarDriver)

	for name, drv := range nativeAntlrDrivers {
		antlrDrivers[name] = drv
	}

	searchDirs := antlrGrammarSearchDirs(antlrGrammarProjectDir)
	allGrammars := []string{"plsql", "postgresql", "tsql", "db2", "cobol85"}

	for _, grammar := range allGrammars {
		bin := findAntlrGrammarBin(grammar, searchDirs)
		if bin != "" {
			// The pool size caps how many files can be parsed concurrently
			// through the sidecar. It was hardcoded to 2, which throttled the
			// whole indexer to two files at a time whenever a grammar pack was
			// installed, regardless of how many cores the machine has.
			antlrDrivers["antlr-"+grammar] = NewSidecarDriver(bin, grammar, sysutil.CPUBudget())
		}
	}
}

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

func antlrGrammarSearchDirs(projectDir string) []string {
	var dirs []string

	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, ".graphit", "grammars", "antlr"))
	}

	if home, err := os.UserHomeDir(); err == nil {
		dirs = append(dirs, filepath.Join(home, ".graphit", "grammars", "antlr"))
	}

	return dirs
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

// antlrConfigOf builds the extension config an ANTLR query file describes.
func antlrConfigOf(qf ExternalQueryFile) *antlrLangConfig {
	grammar := qf.Grammar
	if grammar == "" {
		grammar = "antlr-" + qf.Language
	}
	return &antlrLangConfig{
		Language:   qf.Language,
		Grammar:    grammar,
		Extensions: qf.Extensions,
		StartRule:  qf.StartRule,
	}
}

// The ANTLR tables are built by rebuildExtTables together with the tree-sitter
// ones, from a single sweep of the query directories, and initialised by the
// init in treesitter_adapter.go. Having a second init here would run before or
// after that one depending on file order and build the tables from sources not
// yet read.

// AntlrParser implements LanguageParser for ANTLR v4 grammars.
type AntlrParser struct {
	projectDir string
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	extTablesMu.RLock()
	cfgs := antlrExtMap[ext]
	extTablesMu.RUnlock()

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

	// Read the source once and share it across all candidate grammars.
	src, readErr := ReadFileBytes(path)
	if readErr != nil {
		return nil, readErr
	}

	// Try each grammar exactly once: return the first that extracts entities,
	// otherwise the first that parsed without error. (Previously a second loop
	// re-parsed every candidate from scratch — up to 2x work, and each parse
	// re-read the file — on files that yield no entities, e.g. .sql mapped to
	// plsql+postgresql+tsql.)
	var firstSuccess *ParsedFile
	var lastErr error
	for _, cfg := range cfgs {
		pf, err := a.parseWithConfig(path, ext, cfg, src, isDepend, opts)
		if err != nil {
			lastErr = err
			continue
		}
		if pf != nil && pf.EntityCount() > 0 {
			return pf, nil
		}
		if firstSuccess == nil && pf != nil {
			firstSuccess = pf
		}
	}
	if firstSuccess != nil {
		return firstSuccess, nil
	}
	if lastErr != nil {
		return nil, lastErr
	}
	return nil, fmt.Errorf("no ANTLR grammar matched for %s", ext)
}

// ParseWithGrammar parses using a specific ANTLR grammar name (e.g. "antlr-plsql"),
// bypassing the extension-based lookup. Used by CompositeParser for --grammar overrides.
func (a *AntlrParser) ParseWithGrammar(path, grammarName string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	extTablesMu.RLock()
	cfg, ok := antlrGrammarMap[grammarName]
	extTablesMu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("unknown ANTLR grammar: %s", grammarName)
	}
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	src, err := ReadFileBytes(path)
	if err != nil {
		return nil, err
	}
	return a.parseWithConfig(path, ext, cfg, src, isDepend, opts)
}

func (a *AntlrParser) parseWithConfig(path, ext string, cfg *antlrLangConfig, src []byte, isDepend bool, opts ParseOptions) (*ParsedFile, error) {

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
		Entities: make(map[string][]Entity),
	}
	if opts.IndexSource {
		result.Source = string(src)
	}

	specificLabels := map[string]bool{
		"Struct": true, "Interface": true, "Class": true, "Trait": true, "Enum": true,
	}
	seenNames := map[string]bool{}

	type queryDefAndPattern struct {
		qdef    ExternalQueryDef
		pattern *antlrcommon.Pattern
	}

	exportStrategy, exportCfg, exportCfgList := exportStrategyOf(langConfig)

	var activeQueries []queryDefAndPattern
	for _, qdef := range rpcQueries {
		pattern, pErr := antlrcommon.CompilePattern(qdef.Pattern)
		if pErr != nil {
			continue
		}
		activeQueries = append(activeQueries, queryDefAndPattern{qdef, pattern})
	}

	for _, aq := range activeQueries {
		matches := aq.pattern.MatchWithContext(antlrTree, contextRulePredicate(langConfig))
		for _, match := range matches {
			name := extractNameFromMatch(match.Node, aq.qdef.NameCapture)
			if name == "" {
				continue
			}

			if aq.qdef.DataKey == "imports" {
				name = strings.Trim(name, "'\"")
			}

			// Oracle delimits quoted identifiers and string literals; the delimiters are
			// not part of the name. This matters most for Comment entities, whose name IS
			// the comment text: 'Indicador se Caixa e para Almoxarifado' would otherwise
			// carry its quotes into the UID and into search results.
			name = unquoteIdentifier(name)
			if name == "" {
				continue
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

			contextName, contextType := resolveParentContextAntlr(match, langConfig, name, aq.qdef.GraphLabel)

			result.AddEntity(aq.qdef.DataKey, Entity{
				Name:           name,
				Line:           startLine,
				EndLine:        endLine,
				ModifierExport: ModifierExportVerdict(exportStrategy, entitySource, exportCfg, exportCfgList),
				GraphLabel:     aq.qdef.GraphLabel,
				Complexity:     complexity,
				Context:        contextName,
				ContextType:    contextType,
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

	// Comments last: attaching one to the declaration it precedes needs every
	// entity's line to be known already.
	extractCommentsAntlr(antlrTree, result, filepath.Base(path))

	return result, nil
}

// extractCommentsAntlr records the hidden-channel comments the driver recovered
// as entities named after their own text, each pointing at what it documents.
//
// The tree-sitter side decides ownership structurally — a comment owns the
// declaration that is its next sibling. There is no equivalent here, because the
// comments were never in the tree to have siblings. Proximity is the substitute:
// a comment belongs to the first entity that starts within commentAttachGap
// lines below it, and to the file when nothing is that close.
func extractCommentsAntlr(tree *antlrcommon.TreeNode, result *ParsedFile, fileName string) {
	if tree == nil || len(tree.Comments) == 0 {
		return
	}

	// Entity start lines, so the nearest one below a comment can be found.
	type lineName struct {
		line int
		name string
	}
	var starts []lineName
	for _, ents := range result.Entities {
		for _, e := range ents {
			if e.GraphLabel != "" && e.GraphLabel != LabelComment && e.Line > 0 {
				starts = append(starts, lineName{e.Line, e.Name})
			}
		}
	}
	sort.Slice(starts, func(i, j int) bool { return starts[i].line < starts[j].line })

	seen := map[string]bool{}
	for _, c := range tree.Comments {
		text := cleanDocstring(c.Text)
		if text == "" || seen[text] {
			continue
		}
		seen[text] = true

		endLine := c.End[0]
		target := fileName
		for _, s := range starts {
			if s.line > endLine {
				if s.line-endLine <= commentAttachGap {
					target = s.name
				}
				break
			}
		}

		result.AddEntity("comments", Entity{
			Name:       text,
			Line:       c.Start[0],
			EndLine:    endLine,
			GraphLabel: LabelComment,
		})
		result.References = append(result.References, ReferenceInfo{
			SourceName: text,
			TargetName: target,
			RelType:    "REFERENCES",
			Line:       c.Start[0],
		})
	}
}

// commentAttachGap is how many blank or intervening lines may sit between a
// comment and the declaration it documents. One line covers the usual case of a
// comment written directly above; more than that and the two are unrelated text
// that happens to be nearby.
const commentAttachGap = 2

func extractNameFromMatch(node *antlrcommon.TreeNode, nameCapture string) string {
	if nameCapture != "" && nameCapture != "name" {
		if child := node.ChildByRule(nameCapture); child != nil {
			return nodeName(child)
		}
	}
	return nodeName(node)
}

// nodeName is the name a matched node carries: the object name of a qualified
// name when the node is shaped like one, the leading terminal otherwise (a
// comment's text, a COMMIT, an expression).
func nodeName(node *antlrcommon.TreeNode) string {
	if name := node.QualifiedNameText(); name != "" {
		return name
	}
	return node.FirstTerminalText()
}

// contextRulePredicate turns the language's context_types map into the predicate the
// matcher uses to track the enclosing declaration while it descends.
func contextRulePredicate(langConfig *ExternalQueryFile) func(string) bool {
	if langConfig == nil || len(langConfig.ContextTypes) == 0 {
		return nil
	}
	types := langConfig.ContextTypes
	return func(rule string) bool {
		_, ok := types[rule]
		return ok
	}
}

// resolveParentContextAntlr names the declaration a match belongs to.
//
// It reads the enclosing contexts the matcher tracked, not the immediate parent. Reading
// the parent was wrong for anything nested more than one level: for
// //parameter/parameter_name the parent is `parameter`, so every PL/SQL parameter came
// out with an empty context — and ConvertToCache drops parameters and fields that have
// none, discarding 967 of 2732 entities across a 367-file sample of the Oracle corpus.
//
// selfName and selfLabel are the entity being placed, because the nearest context is
// often the entity's OWN declaration: the match for //create_table/table_name sits
// inside that create_table. Naming itself as its parent made every top-level object
// contain an object of its own name — a CONTAINS self-loop per table, procedure and
// package, and a uid spelled path::X.X. Skipping the self link answers with what
// actually encloses it: the package around a procedure, nothing at all around a table.
// A constructor still gets its class, since there the labels differ.
//
// The immediate parent is still consulted as a fallback, which keeps the previous
// behaviour for grammars whose context node IS the parent.
func resolveParentContextAntlr(match antlrcommon.MatchResult, langConfig *ExternalQueryFile,
	selfName, selfLabel string) (string, string) {
	if langConfig == nil || len(langConfig.ContextTypes) == 0 {
		return "", ""
	}

	owner := func(node *antlrcommon.TreeNode) (string, string, bool) {
		if node == nil {
			return "", "", false
		}
		label, ok := langConfig.ContextTypes[node.Rule]
		if !ok {
			return "", "", false
		}
		name := declarationName(node)
		if name == "" || (name == selfName && label == selfLabel) {
			return "", "", false
		}
		return name, label, true
	}

	for ctx := match.Context; ctx != nil; ctx = ctx.Outer {
		if name, label, ok := owner(ctx.Node); ok {
			return name, label
		}
	}
	if name, label, ok := owner(match.Parent); ok {
		return name, label
	}

	return "", ""
}

// declarationName returns the identifier a declaration node declares.
//
// Not the first terminal: a declaration starts with keywords, so create_function_body
// reported "CREATE" and every entity inside it was attributed to a context named CREATE —
// which no node has, leaving HAS_PARAMETER edges pointing at a phantom.
//
// Not the first "_name" child either, which is what this used to read. A declaration
// spells its name qualified — `CREATE TABLE (schema_name '.')? table_name` — so the
// first such child is the SCHEMA: every column of every table in an Oracle export was
// attributed to one phantom parent named GC. TreeNode.DeclaredNameText walks the
// qualified name to its last component instead, and finds names that no "_name" child
// carries (create_type hides it inside type_definition; a package-level function_body
// declares a plain identifier, which used to resolve to the keyword FUNCTION).
//
// Falls back to the first terminal so a grammar without a name-shaped child behaves as
// it did before.
func declarationName(node *antlrcommon.TreeNode) string {
	if name := unquoteIdentifier(node.DeclaredNameText()); name != "" {
		return name
	}
	return unquoteIdentifier(node.FirstTerminalText())
}

// unquoteIdentifier strips the delimiters Oracle puts around quoted identifiers and string
// literals, so "GC" becomes GC and 'some text' becomes some text.
func unquoteIdentifier(s string) string {
	if len(s) >= 2 {
		if (s[0] == '"' && s[len(s)-1] == '"') || (s[0] == '\'' && s[len(s)-1] == '\'') {
			return s[1 : len(s)-1]
		}
	}
	return s
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
	return HasAntlrForExtensionIn("", ext)
}

// HasAntlrForExtensionIn also counts grammars a project's own query files
// declare. AntlrParser.Parse has always fallen back to those, but nothing could
// reach it: discovery and the watcher filter by extension first, and they were
// asking the global table only.
func HasAntlrForExtensionIn(projectDir, ext string) bool {
	ext = strings.ToLower(ext)
	extTablesMu.RLock()
	registered := len(antlrExtMap[ext]) > 0
	extTablesMu.RUnlock()
	if registered {
		return true
	}
	if projectDir == "" {
		return false
	}
	for _, qf := range loadProjectCached(projectDir) {
		if qf.Parser != "antlr4" {
			continue
		}
		for _, e := range qf.Extensions {
			if strings.EqualFold(e, ext) {
				return true
			}
		}
	}
	return false
}

// HasParserForExtension returns true if any parser (tree-sitter or ANTLR) handles the extension.
//
// This form knows only the languages shared by every project — the installed
// runtime and the user's global query directory. Use HasParserForExtensionIn
// wherever a project directory is at hand.
func HasParserForExtension(ext string) bool {
	return HasParserForExtensionIn("", ext)
}

// HasParserForExtensionIn is HasParserForExtension including the languages a
// project declares for itself.
func HasParserForExtensionIn(projectDir, ext string) bool {
	return HasTreeSitterForExtensionIn(projectDir, ext) || HasAntlrForExtensionIn(projectDir, ext)
}
