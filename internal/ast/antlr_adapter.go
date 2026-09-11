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
	"github.com/graphit-labs/graphit-code/internal/brand"
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

var antlrDrivers map[string]antlrcommon.GrammarDriver
var antlrDriversOnce sync.Once

// antlrGrammarProjectDir is where initAntlrDrivers looks for a project's own
// sidecar grammar binaries. Guarded because every parse worker publishes into it
// (parseWithConfig, below) and the parse pool runs one worker per CPU: workers
// all write the same absolute path, so the outcome was never wrong, but it is a
// data race all the same and -race fails the run on it.
var (
	antlrGrammarProjectDirMu sync.RWMutex
	antlrGrammarProjectDir   string
)

func setAntlrGrammarProjectDirIfUnset(dir string) {
	if dir == "" {
		return
	}
	antlrGrammarProjectDirMu.Lock()
	defer antlrGrammarProjectDirMu.Unlock()
	if antlrGrammarProjectDir == "" {
		antlrGrammarProjectDir = dir
	}
}

func grammarProjectDir() string {
	antlrGrammarProjectDirMu.RLock()
	defer antlrGrammarProjectDirMu.RUnlock()
	return antlrGrammarProjectDir
}

func ResetAntlrCaches() {
	antlrcommon.ResetAllCaches()
}

func initAntlrDrivers() {
	antlrDrivers = make(map[string]antlrcommon.GrammarDriver)

	for name, drv := range nativeAntlrDrivers {
		antlrDrivers[name] = drv
	}

	searchDirs := antlrGrammarSearchDirs(grammarProjectDir())
	allGrammars := []string{"plsql", "postgresql", "tsql", "db2", "cobol85"}

	for _, grammar := range allGrammars {
		bin := findAntlrGrammarBin(grammar, searchDirs)
		if bin != "" {
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
		dirs = append(dirs, filepath.Join(projectDir, brand.DotDir(), "grammars", "antlr"))
	}

	if global := brand.GlobalDir(); global != "" {
		dirs = append(dirs, filepath.Join(global, "grammars", "antlr"))
	}

	return dirs
}

var antlrExtMap map[string][]*antlrLangConfig
var antlrGrammarMap map[string]*antlrLangConfig

type antlrLangConfig struct {
	Language   string
	Grammar    string // Grammar name (e.g. "antlr-plsql")
	Extensions []string
	StartRule  string
	Exclusive  bool
}

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
		Exclusive:  qf.Exclusive,
	}
}

// AntlrParser implements LanguageParser for ANTLR v4 grammars.
type AntlrParser struct {
	projectDir string
}

func (a *AntlrParser) Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
	ext := strings.ToLower(path[strings.LastIndex(path, "."):])
	cfgs := enabledAntlrConfigsFor(a.projectDir, ext)

	if len(cfgs) == 0 {
		langName := strings.TrimPrefix(ext, ".")
		qfs := resolveQueriesForLang(a.projectDir, langName, ext)
		for _, qf := range qfs {
			if qf.Parser == "antlr4" && !qf.Exclusive {
				cfgs = append(cfgs, antlrConfigOf(qf))
			}
		}
		if len(cfgs) == 0 {
			return nil, fmt.Errorf("no ANTLR grammar for %s", ext)
		}
	}

	src, readErr := ReadFileBytes(path)
	if readErr != nil {
		return nil, readErr
	}

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
	if !grammarEnabledIn(a.projectDir, cfg.Language, cfg.Grammar) {
		return nil, fmt.Errorf("grammar disabled by configuration: %s", grammarName)
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

	setAntlrGrammarProjectDirIfUnset(a.projectDir)
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
	complexM := newAntlrComplexityMatcher(langConfig)

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

			name = unquoteIdentifier(name)
			if name == "" {
				continue
			}
			if re := nameRejectMatcher(aq.qdef.NameReject); re != nil && re.MatchString(name) {
				continue
			}

			if aq.qdef.QualifierCapture != "" {
				qualifier := qualifierForMatch(match, aq.qdef.QualifierCapture)
				if qualifier == "" {
					continue
				}
				name = qualifier + "." + name
			}

			if !specificLabels[aq.qdef.GraphLabel] && seenNames[name] {
				continue
			}

			startLine := match.Node.StartLine()
			endLine := match.Node.EndLine()

			scopeNode := match.Node
			entitySource := match.Node.FullText()
			if match.Parent != nil {
				entitySource = match.Parent.FullText()
				scopeNode = match.Parent
			}
			complexity := complexM.score(scopeNode)

			contextName, contextType := resolveParentContextAntlr(match, langConfig, name, aq.qdef.GraphLabel)

			valueText := ""
			if aq.qdef.ValueCapture != "" && aq.qdef.Type != "relation" {
				valueText = extractValueFromMatch(match.Node, aq.qdef.ValueCapture)
			}
			var props map[string]string
			if valueText != "" {
				props = map[string]string{"value": valueText}
			}

			result.AddOrMergeEntity(aq.qdef.DataKey, Entity{
				Name:           name,
				Line:           startLine,
				EndLine:        endLine,
				ModifierExport: ModifierExportVerdict(exportStrategy, entitySource, exportCfg, exportCfgList),
				GraphLabel:     aq.qdef.GraphLabel,
				Complexity:     complexity,
				Context:        contextName,
				ContextType:    contextType,
				Properties:     props,
			})

			if valueText != "" && aq.qdef.ValueLabel != "" {
				result.AddOrMergeEntity(aq.qdef.DataKey, Entity{
					Name:        valueText,
					Line:        startLine,
					EndLine:     endLine,
					GraphLabel:  aq.qdef.ValueLabel,
					Complexity:  1,
					Context:     name,
					ContextType: aq.qdef.GraphLabel,
				})
			}

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

	extractCommentsAntlr(antlrTree, result, filepath.Base(path))

	return result, nil
}

// antlrComplexityMatcher is the ANTLR-side counterpart of complexityMatcher: it
// scores cyclomatic complexity by walking an entity's real parse subtree —
// never its text. branches matches rule names (TreeNode.Rule); operators
// matches a terminal's text, case-insensitively, for grammars that spell a
// combinator as a bare keyword (SQL's AND/OR) rather than a rule of its own.
// boundary holds the rule names that start a nested declaration, where the
// walk stops — that entity is scored on its own. on is false when the
// language YAML has no `complexity:` block, and score then returns the base
// 1 — a language with no `complexity:` block yet has no complexity signal,
// not one guessed by scanning its text for keywords.
type antlrComplexityMatcher struct {
	branches  map[string]bool
	operators map[string]bool
	boundary  map[string]bool
	on        bool
}

func newAntlrComplexityMatcher(langConfig *ExternalQueryFile) antlrComplexityMatcher {
	if langConfig == nil || langConfig.Complexity == nil || len(langConfig.Complexity.NodeTypes) == 0 {
		return antlrComplexityMatcher{}
	}
	branches := make(map[string]bool, len(langConfig.Complexity.NodeTypes))
	for _, t := range langConfig.Complexity.NodeTypes {
		branches[t] = true
	}
	var operators map[string]bool
	if len(langConfig.Complexity.Operators) > 0 {
		operators = make(map[string]bool, len(langConfig.Complexity.Operators))
		for _, o := range langConfig.Complexity.Operators {
			operators[strings.ToUpper(o)] = true
		}
	}
	var boundary map[string]bool
	if len(langConfig.ContextTypes) > 0 {
		boundary = make(map[string]bool, len(langConfig.ContextTypes))
		for k := range langConfig.ContextTypes {
			boundary[k] = true
		}
	}
	return antlrComplexityMatcher{branches: branches, operators: operators, boundary: boundary, on: true}
}

func (m antlrComplexityMatcher) score(root *antlrcommon.TreeNode) int {
	if !m.on || root == nil {
		return 1
	}
	score := 1
	var walk func(n *antlrcommon.TreeNode, depth int)
	walk = func(n *antlrcommon.TreeNode, depth int) {
		if n == nil {
			return
		}
		if depth > 0 && n.IsRule() && m.boundary[n.Rule] {
			return
		}
		if n.IsRule() {
			if m.branches[n.Rule] {
				score++
			}
		} else if len(m.operators) > 0 && m.operators[strings.ToUpper(n.Text)] {
			score++
		}
		for _, c := range n.Children {
			walk(c, depth+1)
		}
	}
	walk(root, 0)
	return score
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

	seen := map[[2]int]bool{}
	for _, c := range tree.Comments {
		if seen[c.Start] {
			continue
		}
		seen[c.Start] = true
		text := cleanDocstring(c.Text)
		if text == "" {
			continue
		}

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
			SourceName: commentUIDName(c.Start[0]),
			TargetName: target,
			RelType:    "REFERENCES",
			Line:       c.Start[0],
		})
	}
}

const commentAttachGap = 2

func extractNameFromMatch(node *antlrcommon.TreeNode, nameCapture string) string {
	if nameCapture != "" && nameCapture != "name" {
		if target := ruleByPath(node, nameCapture); target != nil {
			return nodeName(target)
		}
	}
	return nodeName(node)
}

func extractValueFromMatch(node *antlrcommon.TreeNode, valueCapture string) string {
	target := ruleByPath(node, valueCapture)
	if target == nil {
		return ""
	}
	return dataText(unquoteIdentifier(target.FullText()))
}

func ruleByPath(node *antlrcommon.TreeNode, path string) *antlrcommon.TreeNode {
	if node == nil || path == "" {
		return nil
	}
	current := node
	anyDepth := false
	for _, seg := range strings.Split(path, "/") {
		if seg == "" {
			continue
		}
		if seg == "**" {
			anyDepth = true
			continue
		}
		if anyDepth {
			current = nearestDescendantByRule(current, seg)
			anyDepth = false
		} else {
			current = current.ChildByRule(seg)
		}
		if current == nil {
			return nil
		}
	}
	if current == node {
		return nil
	}
	return current
}

func nearestDescendantByRule(node *antlrcommon.TreeNode, rule string) *antlrcommon.TreeNode {
	queue := append([]*antlrcommon.TreeNode(nil), node.Children...)
	for len(queue) > 0 {
		n := queue[0]
		queue = queue[1:]
		if n == nil {
			continue
		}
		if n.Rule == rule {
			return n
		}
		queue = append(queue, n.Children...)
	}
	return nil
}

func nodeName(node *antlrcommon.TreeNode) string {
	if name := node.QualifiedNameText(); name != "" {
		return name
	}
	return node.FirstTerminalText()
}

// contextRulePredicate turns the language's context_types map into the predicate the
// matcher uses to track the enclosing declaration while it descends.
// contextRulePredicate says which rules the matcher should remember as ancestors.
//
// Two kinds, and they are deliberately not the same thing. context_types are the
// OWNERS — the declaration a match belongs to. Qualifier anchors are merely rules some
// query needs to climb to, and they must NOT become owners: making `update_statement`
// a context type would hand it ownership of everything inside it, and every variable
// of a procedure would suddenly belong to the statement it appears in.
//
// Tracking them costs one extra link in a linked list per enclosing match, which is
// why this is a predicate rather than the ancestor map the matcher deliberately avoids.
func contextRulePredicate(langConfig *ExternalQueryFile) func(string) bool {
	if langConfig == nil {
		return nil
	}
	types := langConfig.ContextTypes
	anchors := qualifierAnchors(langConfig)
	if len(types) == 0 && len(anchors) == 0 {
		return nil
	}
	return func(rule string) bool {
		if _, ok := types[rule]; ok {
			return true
		}
		return anchors[rule]
	}
}

func qualifierAnchors(langConfig *ExternalQueryFile) map[string]bool {
	var anchors map[string]bool
	for _, q := range langConfig.Queries {
		if q.QualifierCapture == "" {
			continue
		}
		anchor, _, _ := strings.Cut(q.QualifierCapture, "/")
		if anchor == "" {
			continue
		}
		if anchors == nil {
			anchors = make(map[string]bool)
		}
		anchors[anchor] = true
	}
	return anchors
}

func qualifierForMatch(match antlrcommon.MatchResult, capture string) string {
	anchor, rest, _ := strings.Cut(capture, "/")
	if anchor == "" {
		return ""
	}

	var node *antlrcommon.TreeNode
	for ctx := match.Context; ctx != nil; ctx = ctx.Outer {
		if ctx.Node != nil && ctx.Node.Rule == anchor {
			node = ctx.Node
			break
		}
	}
	if node == nil && match.Parent != nil && match.Parent.Rule == anchor {
		node = match.Parent
	}
	if node == nil {
		return ""
	}

	target := node
	if rest != "" {
		if target = ruleByPath(node, rest); target == nil {
			return ""
		}
	}
	return dataText(unquoteIdentifier(target.FullText()))
}

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
		name := contextNameAntlr(node, langConfig)
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

func contextNameAntlr(node *antlrcommon.TreeNode, langConfig *ExternalQueryFile) string {
	if langConfig != nil {
		if path := langConfig.ContextNamePaths[node.Rule]; path != "" {
			if target := ruleByPath(node, path); target != nil {
				if name := unquoteIdentifier(dataText(target.FullText())); name != "" {
					return name
				}
			}
			return ""
		}
	}
	return declarationName(node)
}

func declarationName(node *antlrcommon.TreeNode) string {
	if name := unquoteIdentifier(node.DeclaredNameText()); name != "" {
		return name
	}
	return unquoteIdentifier(node.FirstTerminalText())
}

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

// HasAntlrForExtensionIn also counts grammars a project's own query files
// declare. AntlrParser.Parse has always fallen back to those, but nothing could
// reach it: discovery and the watcher filter by extension first, and they were
// asking the global table only.
func HasAntlrForExtensionIn(projectDir, ext string) bool {
	ext = strings.ToLower(ext)
	if len(enabledAntlrConfigsFor(projectDir, ext)) > 0 {
		return true
	}
	if projectDir == "" {
		return false
	}
	for _, qf := range effectiveProjectQueryFiles(projectDir) {
		if qf.Parser != "antlr4" || qf.Exclusive || !queryFileClaims(qf, ext) {
			continue
		}
		if grammarEnabledIn(projectDir, qf.Language, effectiveGrammarName(qf)) {
			return true
		}
	}
	return false
}

func queryFileClaims(qf ExternalQueryFile, ext string) bool {
	for _, e := range qf.Extensions {
		if strings.EqualFold(e, ext) {
			return true
		}
	}
	return false
}

func enabledAntlrConfigsFor(projectDir, ext string) []*antlrLangConfig {
	extTablesMu.RLock()
	registered := antlrExtMap[ext]
	extTablesMu.RUnlock()
	if len(registered) == 0 {
		return nil
	}
	filter := grammarFilterFor(projectDir)
	if filter.inert() {
		return registered
	}
	enabled := make([]*antlrLangConfig, 0, len(registered))
	for _, cfg := range registered {
		if filter.allows(cfg.Language, cfg.Grammar) {
			enabled = append(enabled, cfg)
		}
	}
	return enabled
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
// project declares for itself, and the grammar its configuration binds to the
// extension — which is the only way an exclusive grammar is ever reached.
func HasParserForExtensionIn(projectDir, ext string) bool {
	if grammar := overriddenGrammarFor(projectDir, ext); grammar != "" {
		return grammarKnownIn(projectDir, grammar)
	}
	return HasTreeSitterForExtensionIn(projectDir, ext) || HasAntlrForExtensionIn(projectDir, ext)
}
