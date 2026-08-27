package antlrcommon

import (
	"fmt"
	"strings"
	"sync"
)

// Pattern represents a compiled rule-path pattern for matching ANTLR parse tree nodes.
//
// Supported syntax:
//
//	//ruleName       — match any descendant with this rule name
//	/ruleName        — match direct child with this rule name
//	//a/b            — match b that is a direct child of any a descendant
//	//a//b           — match b anywhere under any a descendant
//	//a[KEYWORD]     — match a only when it has KEYWORD as a direct terminal
//	//a[!KEYWORD]    — match a only when it does not
//
// The keyword guard exists because a grammar can spell two different things with
// one rule. Oracle has no constant_declaration: a constant is a
// variable_declaration carrying the CONSTANT keyword —
//
//	variable_declaration : identifier CONSTANT? type_spec (NOT? NULL_)? default_value_part? ';'
//
// so without a guard every PL/SQL constant is indexed as a Variable and the query
// written against //constant_declaration matches nothing at all. Comparison is
// case-insensitive: SQL keywords are written both ways in the wild.
//
// The captured name comes from the first terminal child of the matched node,
// or from a specific child rule if the YAML specifies name_capture.
type Pattern struct {
	segments []segment
	raw      string
}

type matchMode int

const (
	matchDirect    matchMode = iota // /ruleName — direct child only
	matchRecursive                  // //ruleName — any descendant
)

type segment struct {
	rule string
	mode matchMode
	// requireTerminal and rejectTerminal hold the keyword guard from a `[KW]` or
	// `[!KW]` suffix, upper-cased for case-insensitive comparison. Empty when the
	// segment has no guard.
	requireTerminal string
	rejectTerminal  string
}

// matches reports whether a node satisfies this segment: the rule name, and the
// keyword guard when there is one.
func (s segment) matches(node *TreeNode) bool {
	if node == nil || node.Rule != s.rule {
		return false
	}
	if s.requireTerminal == "" && s.rejectTerminal == "" {
		return true
	}
	has := hasTerminalChild(node, s.requireTerminal+s.rejectTerminal)
	if s.requireTerminal != "" {
		return has
	}
	return !has
}

// hasTerminalChild reports whether node has a direct terminal child whose text
// equals keyword, ignoring case.
func hasTerminalChild(node *TreeNode, keyword string) bool {
	for _, child := range node.Children {
		if child != nil && child.IsTerminal() && strings.EqualFold(child.Text, keyword) {
			return true
		}
	}
	return false
}

var patternCache sync.Map // map[string]*Pattern

// CompilePattern parses a rule-path pattern string into a Pattern.
func CompilePattern(pattern string) (*Pattern, error) {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return nil, fmt.Errorf("empty pattern")
	}

	if val, ok := patternCache.Load(pattern); ok {
		return val.(*Pattern), nil
	}

	compiled, err := compilePatternImpl(pattern)
	if err != nil {
		return nil, err
	}

	patternCache.Store(pattern, compiled)
	return compiled, nil
}

func compilePatternImpl(pattern string) (*Pattern, error) {
	var segments []segment
	rest := pattern

	for rest != "" {
		if strings.HasPrefix(rest, "//") {
			rest = rest[2:]
			name, remaining := nextSegmentName(rest)
			seg, err := parseSegment(name, matchRecursive, pattern, "//")
			if err != nil {
				return nil, err
			}
			segments = append(segments, seg)
			rest = remaining
		} else if strings.HasPrefix(rest, "/") {
			rest = rest[1:]
			name, remaining := nextSegmentName(rest)
			seg, err := parseSegment(name, matchDirect, pattern, "/")
			if err != nil {
				return nil, err
			}
			segments = append(segments, seg)
			rest = remaining
		} else {
			return nil, fmt.Errorf("unexpected character %q in pattern %q (expected '/' or '//')", rest[:1], pattern)
		}
	}

	if len(segments) == 0 {
		return nil, fmt.Errorf("no segments in pattern %q", pattern)
	}

	return &Pattern{segments: segments, raw: pattern}, nil
}

func nextSegmentName(s string) (name, remaining string) {
	idx := strings.IndexByte(s, '/')
	if idx < 0 {
		return s, ""
	}
	return s[:idx], s[idx:]
}

// parseSegment splits a segment into its rule name and optional `[KW]` /
// `[!KW]` keyword guard.
func parseSegment(raw string, mode matchMode, pattern, sep string) (segment, error) {
	open := strings.IndexByte(raw, '[')
	if open < 0 {
		if raw == "" {
			return segment{}, fmt.Errorf("empty rule name after %q in pattern %q", sep, pattern)
		}
		return segment{rule: raw, mode: mode}, nil
	}
	if !strings.HasSuffix(raw, "]") {
		return segment{}, fmt.Errorf("unterminated '[' in segment %q of pattern %q", raw, pattern)
	}
	rule := raw[:open]
	if rule == "" {
		return segment{}, fmt.Errorf("empty rule name after %q in pattern %q", sep, pattern)
	}
	guard := strings.TrimSpace(raw[open+1 : len(raw)-1])
	negate := strings.HasPrefix(guard, "!")
	guard = strings.ToUpper(strings.TrimSpace(strings.TrimPrefix(guard, "!")))
	if guard == "" {
		return segment{}, fmt.Errorf("empty keyword guard in segment %q of pattern %q", raw, pattern)
	}
	seg := segment{rule: rule, mode: mode}
	if negate {
		seg.rejectTerminal = guard
	} else {
		seg.requireTerminal = guard
	}
	return seg, nil
}

// MatchResult represents a single match of a pattern against the parse tree.
type MatchResult struct {
	Node   *TreeNode
	Parent *TreeNode

	// Context is the nearest enclosing node whose rule the caller declared as a
	// context (a function body, a package, a table), linked outwards to the ones
	// enclosing it. Nil unless the match came from MatchWithContext.
	//
	// It exists because Parent alone cannot answer "what owns this?": for
	// //parameter/parameter_name the parent is `parameter`, while the owning
	// function body is several levels up. Resolving ownership from Parent silently
	// left every PL/SQL parameter without an owner, and a downstream rule drops
	// owner-less parameters — 967 of 2732 entities across a 367-file sample, 35%.
	//
	// The chain, and not just the nearest link, because a declaration's own node is
	// a context too: the match for //procedure_body/identifier sits inside the very
	// procedure_body it names, and the answer wanted there is the package around it.
	//
	// Carried down during the walk rather than reconstructed afterwards: no parent
	// pointers exist on TreeNode, and an ancestor map would cost one entry per node
	// on files that reach 700 KB.
	Context *ContextNode
}

// ContextNode is one enclosing declaration, linked to the one outside it. A linked
// list rather than a slice: the walk forks at every child, and appending to a shared
// slice would let sibling branches overwrite each other's ancestry.
type ContextNode struct {
	Node  *TreeNode
	Outer *ContextNode
}

// Match finds all nodes in the tree that match this pattern.
func (p *Pattern) Match(root *TreeNode) []MatchResult {
	return p.MatchWithContext(root, nil)
}

// MatchWithContext is Match, additionally reporting for each result the nearest
// enclosing node accepted by isContext. Pass nil to skip context tracking.
func (p *Pattern) MatchWithContext(root *TreeNode, isContext func(rule string) bool) []MatchResult {
	if root == nil || len(p.segments) == 0 {
		return nil
	}

	var results []MatchResult
	first := p.segments[0]
	isLast := len(p.segments) == 1

	ctx := (*ContextNode)(nil)
	if isContext != nil && isContext(root.Rule) {
		ctx = &ContextNode{Node: root}
	}

	switch first.mode {
	case matchRecursive:
		// //rule — search entire tree including root
		p.walkRecursive(root, nil, ctx, isContext, first, 0, isLast, &results)
	case matchDirect:
		// /rule — root itself must match this rule name
		if first.matches(root) {
			if isLast {
				results = append(results, MatchResult{Node: root, Parent: nil, Context: ctx})
			} else {
				p.matchSegment(root, nil, ctx, isContext, 1, &results)
			}
		}
	}
	return results
}

// MatchAt checks if the pattern matches starting at the given node.
// Returns matched descendants and true if there are matches.
func (p *Pattern) MatchAt(node, parent *TreeNode) ([]MatchResult, bool) {
	if len(p.segments) == 0 {
		return nil, false
	}
	first := p.segments[0]
	if first.mode == matchDirect && parent != nil {
		return nil, false
	}
	if !first.matches(node) {
		return nil, false
	}

	var results []MatchResult
	isLast := len(p.segments) == 1
	if isLast {
		results = []MatchResult{{Node: node, Parent: parent}}
	} else {
		p.matchSegment(node, parent, nil, nil, 1, &results)
	}
	return results, len(results) > 0
}

func (p *Pattern) matchSegment(node, parent *TreeNode, ctx *ContextNode, isContext func(string) bool,
	segIdx int, results *[]MatchResult) {
	if segIdx >= len(p.segments) {
		return
	}

	seg := p.segments[segIdx]
	isLast := segIdx == len(p.segments)-1

	switch seg.mode {
	case matchRecursive:
		// Find all descendants (including node itself) matching this rule name
		p.walkRecursive(node, parent, ctx, isContext, seg, segIdx, isLast, results)
	case matchDirect:
		// Only check direct children
		for _, child := range node.Children {
			if seg.matches(child) {
				childCtx := ctx
				if isContext != nil && isContext(child.Rule) {
					childCtx = &ContextNode{Node: child, Outer: ctx}
				}
				if isLast {
					*results = append(*results, MatchResult{Node: child, Parent: node, Context: childCtx})
				} else {
					p.matchSegment(child, node, childCtx, isContext, segIdx+1, results)
				}
			}
		}
	}
}

func (p *Pattern) walkRecursive(node, parent *TreeNode, ctx *ContextNode, isContext func(string) bool,
	seg segment, segIdx int, isLast bool, results *[]MatchResult) {
	if isContext != nil && isContext(node.Rule) {
		ctx = &ContextNode{Node: node, Outer: ctx}
	}
	if seg.matches(node) {
		if isLast {
			*results = append(*results, MatchResult{Node: node, Parent: parent, Context: ctx})
		} else {
			p.matchSegment(node, parent, ctx, isContext, segIdx+1, results)
		}
	}
	for _, child := range node.Children {
		p.walkRecursive(child, node, ctx, isContext, seg, segIdx, isLast, results)
	}
}

// String returns the original pattern string.
func (p *Pattern) String() string {
	return p.raw
}
