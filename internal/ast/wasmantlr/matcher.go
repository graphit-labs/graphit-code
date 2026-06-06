package wasmantlr

import (
	"fmt"
	"strings"
)

// Pattern represents a compiled rule-path pattern for matching ANTLR parse tree nodes.
//
// Supported syntax:
//
//	//ruleName       — match any descendant with this rule name
//	/ruleName        — match direct child with this rule name
//	//a/b            — match b that is a direct child of any a descendant
//	//a//b           — match b anywhere under any a descendant
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
}

// CompilePattern parses a rule-path pattern string into a Pattern.
func CompilePattern(pattern string) (*Pattern, error) {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return nil, fmt.Errorf("empty pattern")
	}

	var segments []segment
	rest := pattern

	for rest != "" {
		if strings.HasPrefix(rest, "//") {
			rest = rest[2:]
			name, remaining := nextSegmentName(rest)
			if name == "" {
				return nil, fmt.Errorf("empty rule name after '//' in pattern %q", pattern)
			}
			segments = append(segments, segment{rule: name, mode: matchRecursive})
			rest = remaining
		} else if strings.HasPrefix(rest, "/") {
			rest = rest[1:]
			name, remaining := nextSegmentName(rest)
			if name == "" {
				return nil, fmt.Errorf("empty rule name after '/' in pattern %q", pattern)
			}
			segments = append(segments, segment{rule: name, mode: matchDirect})
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

// MatchResult represents a single match of a pattern against the parse tree.
type MatchResult struct {
	Node   *TreeNode
	Parent *TreeNode
}

// Match finds all nodes in the tree that match this pattern.
func (p *Pattern) Match(root *TreeNode) []MatchResult {
	if root == nil || len(p.segments) == 0 {
		return nil
	}

	var results []MatchResult
	first := p.segments[0]
	isLast := len(p.segments) == 1

	switch first.mode {
	case matchRecursive:
		// //rule — search entire tree including root
		p.walkRecursive(root, nil, first.rule, 0, isLast, &results)
	case matchDirect:
		// /rule — root itself must match this rule name
		if root.Rule == first.rule {
			if isLast {
				results = append(results, MatchResult{Node: root, Parent: nil})
			} else {
				p.matchSegment(root, nil, 1, &results)
			}
		}
	}
	return results
}

func (p *Pattern) matchSegment(node, parent *TreeNode, segIdx int, results *[]MatchResult) {
	if segIdx >= len(p.segments) {
		return
	}

	seg := p.segments[segIdx]
	isLast := segIdx == len(p.segments)-1

	switch seg.mode {
	case matchRecursive:
		// Find all descendants (including node itself) matching this rule name
		p.walkRecursive(node, parent, seg.rule, segIdx, isLast, results)
	case matchDirect:
		// Only check direct children
		for _, child := range node.Children {
			if child.Rule == seg.rule {
				if isLast {
					*results = append(*results, MatchResult{Node: child, Parent: node})
				} else {
					p.matchSegment(child, node, segIdx+1, results)
				}
			}
		}
	}
}

func (p *Pattern) walkRecursive(node, parent *TreeNode, rule string, segIdx int, isLast bool, results *[]MatchResult) {
	if node.Rule == rule {
		if isLast {
			*results = append(*results, MatchResult{Node: node, Parent: parent})
		} else {
			p.matchSegment(node, parent, segIdx+1, results)
		}
	}
	for _, child := range node.Children {
		p.walkRecursive(child, node, rule, segIdx, isLast, results)
	}
}

// String returns the original pattern string.
func (p *Pattern) String() string {
	return p.raw
}
