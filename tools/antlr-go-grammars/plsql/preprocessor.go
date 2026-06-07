package main

import (
	"fmt"
	"strings"
	"unicode"
)

// matchKeywordAt checks case-insensitively if keyword kw starts at src[pos]
// followed by a word boundary.
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

// isStatementStart checks if a DDL keyword begins at src[pos].
// DML and PL/SQL block keywords are excluded — they can appear inside
// function/procedure bodies and must not trigger semicolon injection.
func isStatementStart(src string, pos int) bool {
	return matchKeywordAt(src, pos, "CREATE") ||
		matchKeywordAt(src, pos, "ALTER") ||
		matchKeywordAt(src, pos, "DROP") ||
		matchKeywordAt(src, pos, "GRANT") ||
		matchKeywordAt(src, pos, "REVOKE") ||
		matchKeywordAt(src, pos, "COMMENT") ||
		matchKeywordAt(src, pos, "TRUNCATE") ||
		matchKeywordAt(src, pos, "ANALYZE") ||
		matchKeywordAt(src, pos, "AUDIT") ||
		matchKeywordAt(src, pos, "NOAUDIT") ||
		matchKeywordAt(src, pos, "FLASHBACK") ||
		matchKeywordAt(src, pos, "PURGE") ||
		matchKeywordAt(src, pos, "RENAME")
}

func isWhitespace(c byte) bool {
	return c == ' ' || c == '\t' || c == '\r' || c == '\n'
}

// skipBackWhitespace walks backwards over whitespace, returning the
// position of the last non-whitespace byte.
func skipBackWhitespace(out string, pos int) int {
	for pos > 0 && isWhitespace(out[pos]) {
		pos--
	}
	return pos
}

// stripMviewUsing removes Data Pump's internal USING clause from materialized views.
// Pattern: `) USING ("name", ...) REFRESH|AS|BUILD`
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

// Preprocess normalizes PL/SQL source from DBMS_METADATA / Data Pump output.
//
// Three phases:
//  1. Strip Data Pump USING metadata from materialized views
//  2. Strip Oracle 12c+ EDITIONABLE/NONEDITIONABLE keywords (not in grammar)
//  3. Inject missing ';' terminators between concatenated DDL statements
//     and close unmatched parentheses from truncated storage clauses
func Preprocess(raw string) string {
	if len(raw) == 0 {
		return raw
	}

	// Phase 1: strip MVIEW USING metadata.
	src := stripMviewUsing(raw)

	// Phase 2: strip EDITIONABLE/NONEDITIONABLE keywords.
	{
		var cleaned strings.Builder
		cleaned.Grow(len(src))
		i2 := 0
		for i2 < len(src) {
			matched := false
			type kw struct {
				word string
				len  int
			}
			keywords := [2]kw{{"NONEDITIONABLE", 14}, {"EDITIONABLE", 11}}
			for _, k := range keywords {
				if matchKeywordAt(src, i2, k.word) {
					i2 += k.len
					for i2 < len(src) && isWhitespace(src[i2]) {
						i2++
					}
					matched = true
					break
				}
			}
			if !matched {
				cleaned.WriteByte(src[i2])
				i2++
			}
		}
		src = cleaned.String()
	}

	// Phase 3: inject missing ';' between DDL statements.
	var out strings.Builder
	out.Grow(len(src) + len(src)/64)

	i := 0
	length := len(src)

	parenDepth := 0
	inString := false
	inDqString := false
	inLineComment := false
	inBlockComment := false

	for i < length {
		c := src[i]

		if inLineComment {
			out.WriteByte(c)
			if c == '\n' {
				inLineComment = false
			}
			i++
			continue
		}
		if inBlockComment {
			out.WriteByte(c)
			if c == '*' && i+1 < length && src[i+1] == '/' {
				out.WriteByte(src[i+1])
				i += 2
				inBlockComment = false
			} else {
				i++
			}
			continue
		}
		if inString {
			out.WriteByte(c)
			if c == '\'' && i+1 < length && src[i+1] == '\'' {
				out.WriteByte(src[i+1])
				i += 2
			} else {
				if c == '\'' {
					inString = false
				}
				i++
			}
			continue
		}
		if inDqString {
			out.WriteByte(c)
			if c == '"' {
				inDqString = false
			}
			i++
			continue
		}

		// Detect comment/string starts.
		if c == '-' && i+1 < length && src[i+1] == '-' {
			inLineComment = true
			out.WriteByte(c)
			i++
			continue
		}
		if c == '/' && i+1 < length && src[i+1] == '*' {
			inBlockComment = true
			out.WriteByte(c)
			i++
			continue
		}
		if c == '\'' {
			inString = true
			out.WriteByte(c)
			i++
			continue
		}
		if c == '"' {
			inDqString = true
			out.WriteByte(c)
			i++
			continue
		}


		if c == '(' {
			parenDepth++
		}
		if c == ')' {
			if parenDepth > 0 {
				parenDepth--
			}
		}


		if c == '\n' {
			out.WriteByte(c)
			i++

			lineStart := i
			for i < length && (src[i] == ' ' || src[i] == '\t') {
				i++
			}


			if i < length && src[i] == '-' && i+1 < length && src[i+1] == '-' {
				outStr := out.String()
				for j := lineStart; j < i; j++ {
					out.WriteByte(src[j])
				}
				_ = outStr
				continue
			}

			if i < length && isStatementStart(src, i) {
				outStr := out.String()
				prevPos := skipBackWhitespace(outStr, len(outStr)-1)
				var prev byte
				if prevPos < len(outStr) {
					prev = outStr[prevPos]
				}

				if prev != ';' && prev != '/' && prev != 0 {
					for parenDepth > 0 {
						out.WriteByte(')')
						parenDepth--
					}
					out.WriteString(";\n")
				}
			}

			for j := lineStart; j < i; j++ {
				out.WriteByte(src[j])
			}
			continue
		}

		out.WriteByte(c)
		i++
	}

	// Close trailing unmatched parens at EOF.
	if parenDepth > 0 {
		for parenDepth > 0 {
			out.WriteByte(')')
			parenDepth--
		}
		out.WriteByte(';')
	}

	return out.String()
}


