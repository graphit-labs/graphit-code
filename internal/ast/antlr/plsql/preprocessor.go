package plsql

import (
	"strings"
)

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

// Preprocess normalizes PL/SQL source from DBMS_METADATA / Data Pump output.
//
// Currently only strips Data Pump USING metadata from materialized views.
// EDITIONABLE/NONEDITIONABLE keywords and statement separation are handled
// natively by the ANTLR grammar (sql_script rule).
func Preprocess(raw string) string {
	if len(raw) == 0 {
		return raw
	}
	return stripMviewUsing(raw)
}
