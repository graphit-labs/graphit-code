package ast

import (
	"fmt"
	"sort"
	"strings"
)

func FormatRecordsTOON(records []QueryRecord) string {
	if len(records) == 0 {
		return "results[0]{}:"
	}

	columns := make([]string, 0, len(records[0]))
	for k := range records[0] {
		columns = append(columns, k)
	}
	sort.Strings(columns)

	displayColumns := make([]string, len(columns))
	for i, col := range columns {
		if strings.HasPrefix(col, "LABEL(") {
			displayColumns[i] = "label"
		} else {
			displayColumns[i] = col
		}
	}

	var sb strings.Builder

	fmt.Fprintf(&sb, "results[%d]{%s}:\n",
		len(records), strings.Join(displayColumns, "|"))

	for _, rec := range records {
		vals := make([]string, len(columns))
		for i, col := range columns {
			v := rec[col]
			vals[i] = formatTOONValue(v)
		}
		sb.WriteString("  ")
		sb.WriteString(strings.Join(vals, "|"))
		sb.WriteByte('\n')
	}

	return strings.TrimRight(sb.String(), "\n")
}

func formatTOONValue(v any) string {
	if v == nil {
		return ""
	}
	switch val := v.(type) {
	case string:
		return val
	case float64:
		if val == float64(int64(val)) {
			return fmt.Sprintf("%d", int64(val))
		}
		return fmt.Sprintf("%g", val)
	case int:
		return fmt.Sprintf("%d", val)
	case int64:
		return fmt.Sprintf("%d", val)
	case bool:
		if val {
			return "true"
		}
		return "false"
	case []any:
		parts := make([]string, len(val))
		for i, item := range val {
			parts[i] = formatTOONValue(item)
		}
		return "[" + strings.Join(parts, ",") + "]"
	case map[string]any:
		parts := make([]string, 0, len(val))
		for k, v2 := range val {
			parts = append(parts, k+":"+formatTOONValue(v2))
		}
		sort.Strings(parts)
		return "{" + strings.Join(parts, ",") + "}"
	default:
		return fmt.Sprintf("%v", val)
	}
}

func FormatSearchResultsTOON(results []SearchResult) string {
	if len(results) == 0 {
		return "results[0]{}:"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "results[%d]{type|name|path|line|score|search_type}:\n", len(results))
	for _, r := range results {
		fmt.Fprintf(&sb, "  %s|%s|%s|%d|%.2f|%s\n", r.Type, r.Name, r.Path, r.Line, r.RelevanceScore, r.SearchType)
	}
	return strings.TrimRight(sb.String(), "\n")
}
