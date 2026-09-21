package ast

import (
	"fmt"
	"regexp"
	"strings"
)

var canonicalRelationshipStart = regexp.MustCompile(`(?:<-|-)\s*\[`)
var canonicalAnonymousRelationshipPattern = regexp.MustCompile(`\)\s*(?:<-\s*-|-\s*->|-\s*-)\s*\(`)

// The 0.19 Icebug multi-table scan can retain a previous table's Parquet reader.
// Fail closed instead of returning plausible but incorrect endpoints. The
// canonical traversal planner and the typed sample never use that operator.
func canonicalUnsafeScan(cypher string) error {
	code := []byte(cypher)
	for i := 0; i < len(code); {
		start := i
		switch {
		case code[i] == '\'' || code[i] == '"':
			quote := code[i]
			i++
			for i < len(code) {
				if code[i] == '\\' {
					i += 2
					continue
				}
				if code[i] == quote {
					i++
					break
				}
				i++
			}
		case code[i] == '`':
			i++
			for i < len(code) {
				if code[i] == '`' {
					i++
					if i < len(code) && code[i] == '`' {
						i++
						continue
					}
					break
				}
				code[i] = 'x'
				i++
			}
			continue
		case i+1 < len(code) && code[i] == '/' && code[i+1] == '/':
			for i < len(code) && code[i] != '\n' {
				i++
			}
		case i+1 < len(code) && code[i] == '/' && code[i+1] == '*':
			i += 2
			for i+1 < len(code) && (code[i] != '*' || code[i+1] != '/') {
				i++
			}
			i += 2
		default:
			i++
			continue
		}
		if i > len(code) {
			i = len(code)
		}
		for j := start; j < i; j++ {
			code[j] = ' '
		}
	}
	clean := string(code)
	unsafe := canonicalAnonymousRelationshipPattern.MatchString(clean)
	for _, location := range canonicalRelationshipStart.FindAllStringIndex(clean, -1) {
		start := location[1]
		depth, end := 1, start
		for ; end < len(clean); end++ {
			if clean[end] == '[' {
				depth++
			}
			if clean[end] == ']' {
				depth--
				if depth == 0 {
					break
				}
			}
		}
		if depth != 0 {
			unsafe = true
			continue
		}
		header := clean[start:end]
		// Properties and recursive path predicates are not type declarations.
		if cutoff := strings.IndexAny(header, "{*("); cutoff >= 0 {
			header = header[:cutoff]
		}
		if !strings.Contains(header, ":") || strings.Contains(header, "|") {
			unsafe = true
		}
	}
	if unsafe {
		return fmt.Errorf("canonical catalog: wildcard and multiple-type relationship scans are unavailable with Icebug 0.19 because they can return incorrect endpoints. Use a filtered logical traversal, for example MATCH (a:Function)-[:CALLS]->(b:Function) WHERE a.name = 'Checkout' RETURN DISTINCT b.name; use Relationship map for a bounded cross-type sample")
	}
	return nil
}
