package ast

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
	"unicode"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

const (
	icebugTraversalBatchSize = 512
	icebugTraversalMaxHops   = 8
	icebugUIDColumn          = "__graphit_uid"
)

var (
	nodePattern     = regexp.MustCompile(`(?is)^\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::\s*` + "`?" + `([A-Za-z_][A-Za-z0-9_]*)` + "`?" + `)?\s*(?:\{\s*(.*?)\s*\})?\s*\)$`)
	propertyPattern = regexp.MustCompile(`(?is)^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+?)\s*$`)
)

type icebugNodePattern struct {
	variable   string
	label      string
	properties []string
}




func (k *LadybugBackend) queryRecordsLocked(cypher string, params map[string]any) ([]QueryRecord, error) {
	res, err := k.runQuery(cypher, params)
	if err != nil {
		return nil, err
	}
	defer res.Close()
	result, err := ladybugResultToQueryResult(res)
	if err != nil {
		return nil, err
	}
	return result.Records, nil
}


func parseIcebugNodePattern(raw string) (icebugNodePattern, bool) {
	match := nodePattern.FindStringSubmatch(raw)
	if match == nil {
		return icebugNodePattern{}, false
	}
	node := icebugNodePattern{variable: match[1], label: match[2]}
	if strings.TrimSpace(match[3]) == "" {
		return node, true
	}
	for _, property := range splitTopLevel(match[3], ",") {
		parts := propertyPattern.FindStringSubmatch(property)
		if parts == nil {
			return icebugNodePattern{}, false
		}
		node.properties = append(node.properties,
			fmt.Sprintf("%s.%s = %s", node.variable, parts[1], strings.TrimSpace(parts[2])))
	}
	return node, true
}



func (n icebugNodePattern) selective(predicates []string) bool {
	return len(n.properties) > 0 || len(predicates) > 0
}


func referencesVariable(expression, variable string) bool {
	if variable == "" {
		return false
	}
	for i := 0; i < len(expression); {
		if expression[i] == '\'' || expression[i] == '"' || expression[i] == '`' {
			i = skipQuoted(expression, i)
			continue
		}
		if isIdentifierStart(rune(expression[i])) {
			start := i
			i++
			for i < len(expression) && isIdentifierPart(rune(expression[i])) {
				i++
			}
			if expression[start:i] == variable {
				return true
			}
			continue
		}
		i++
	}
	return false
}


func splitTopLevel(expression, separator string) []string {
	expression = trimOuterParentheses(strings.TrimSpace(expression))
	if expression == "" {
		return nil
	}
	var parts []string
	start, depth := 0, 0
	for i := 0; i < len(expression); {
		switch expression[i] {
		case '\'', '"', '`':
			i = skipQuoted(expression, i)
			continue
		case '(', '[', '{':
			depth++
		case ')', ']', '}':
			if depth > 0 {
				depth--
			}
		}
		if depth == 0 && keywordAt(expression, i, separator) {
			parts = append(parts, trimOuterParentheses(strings.TrimSpace(expression[start:i])))
			i += len(separator)
			start = i
			continue
		}
		i++
	}
	parts = append(parts, trimOuterParentheses(strings.TrimSpace(expression[start:])))
	return parts
}

func trimOuterParentheses(expression string) string {
	for len(expression) >= 2 && expression[0] == '(' && expression[len(expression)-1] == ')' {
		depth, closesAtEnd := 0, false
		for i := 0; i < len(expression); {
			if expression[i] == '\'' || expression[i] == '"' || expression[i] == '`' {
				i = skipQuoted(expression, i)
				continue
			}
			switch expression[i] {
			case '(':
				depth++
			case ')':
				depth--
				if depth == 0 {
					closesAtEnd = i == len(expression)-1
					i = len(expression)
					continue
				}
			}
			i++
		}
		if !closesAtEnd {
			break
		}
		expression = strings.TrimSpace(expression[1 : len(expression)-1])
	}
	return expression
}

func keywordAt(expression string, offset int, keyword string) bool {
	if offset+len(keyword) > len(expression) || !strings.EqualFold(expression[offset:offset+len(keyword)], keyword) {
		return false
	}
	beforeOK := offset == 0 || !isIdentifierPart(rune(expression[offset-1]))
	after := offset + len(keyword)
	afterOK := after == len(expression) || !isIdentifierPart(rune(expression[after]))
	return beforeOK && afterOK
}

func skipQuoted(expression string, start int) int {
	quote := expression[start]
	for i := start + 1; i < len(expression); i++ {
		if expression[i] != quote {
			continue
		}
		if i+1 < len(expression) && expression[i+1] == quote {
			i++
			continue
		}
		return i + 1
	}
	return len(expression)
}

func isIdentifierStart(r rune) bool { return r == '_' || unicode.IsLetter(r) }
func isIdentifierPart(r rune) bool  { return isIdentifierStart(r) || unicode.IsDigit(r) }

func isIdentifier(value string) bool {
	if value == "" || !isIdentifierStart(rune(value[0])) {
		return false
	}
	for _, r := range value[1:] {
		if !isIdentifierPart(r) {
			return false
		}
	}
	return true
}

func uidValues(records []QueryRecord) []string {
	set := make(map[string]bool, len(records))
	for _, record := range records {
		if uid, ok := record[icebugUIDColumn].(string); ok && uid != "" {
			set[uid] = true
		}
	}
	return sortedUIDs(set)
}

func sortedUIDs(set map[string]bool) []string {
	values := make([]string, 0, len(set))
	for value := range set {
		values = append(values, value)
	}
	sort.Strings(values)
	return values
}

func icebugStringList(values []string) string {
	quoted := make([]string, len(values))
	for i, value := range values {
		quoted[i] = "'" + ladybug.EscapeLiteral(value) + "'"
	}
	return strings.Join(quoted, ",")
}

func icebugRecordKey(record QueryRecord) string {
	columns := make([]string, 0, len(record))
	for column := range record {
		columns = append(columns, column)
	}
	sort.Strings(columns)
	var key strings.Builder
	for _, column := range columns {
		fmt.Fprintf(&key, "%d:%s=%#v;", len(column), column, record[column])
	}
	return key.String()
}
