package ast

import (
	"context"
	"fmt"
	"regexp"
	"strings"
)

type SourceRequest struct {
	Path       string
	Entity     string
	EntityType string
	Head       int
	Tail       int
	StartLine  int
	EndLine    int
	Pattern    string
	IsRegex    bool
	Before     int
	After      int
	LineNumbers bool
}

type SourceResult struct {
	Path       string        `json:"path"`
	Source     string        `json:"source"`
	TotalLines int           `json:"total_lines"`
	StartLine  int           `json:"start_line,omitempty"`
	EndLine    int           `json:"end_line,omitempty"`
	Matches    []SourceMatch `json:"matches,omitempty"`
	Entity     *EntityInfo   `json:"entity,omitempty"`
}

type SourceMatch struct {
	LineNumber int    `json:"line_number"`
	Line       string `json:"line"`
	IsMatch    bool   `json:"is_match"`
}

type EntityInfo struct {
	Name      string `json:"name"`
	Type      string `json:"type"`
	StartLine int    `json:"start_line"`
	EndLine   int    `json:"end_line"`
}

type SourceService struct {
	db GraphDB
}

func NewSourceService(db GraphDB) *SourceService {
	return &SourceService{db: db}
}

func (s *SourceService) GetSource(ctx context.Context, req SourceRequest) (*SourceResult, error) {
	src, err := s.fetchFileSource(ctx, req.Path)
	if err != nil {
		return nil, err
	}

	allLines := strings.Split(src, "\n")
	totalLines := len(allLines)

	result := &SourceResult{
		Path:       req.Path,
		TotalLines: totalLines,
	}

	lines := allLines
	lineOffset := 1

	if req.Entity != "" {
		entityInfo, err := s.resolveEntity(ctx, req)
		if err != nil {
			return nil, err
		}
		result.Entity = entityInfo

		start := entityInfo.StartLine
		end := entityInfo.EndLine
		if start < 1 {
			start = 1
		}
		if end > totalLines {
			end = totalLines
		}
		if end < start {
			end = start
		}

		lines = allLines[start-1 : end]
		lineOffset = start
		result.StartLine = start
		result.EndLine = end
	}

	if req.StartLine > 0 || req.EndLine > 0 {
		start := req.StartLine
		end := req.EndLine
		if start < 1 {
			start = 1
		}
		if end <= 0 || end > len(lines)+lineOffset-1 {
			end = len(lines) + lineOffset - 1
		}

		absStart := start
		absEnd := end
		if req.Entity != "" {
			absStart = result.Entity.StartLine + start - 1
			absEnd = result.Entity.StartLine + end - 1
			if absEnd > result.Entity.EndLine {
				absEnd = result.Entity.EndLine
			}
		}

		relStart := absStart - lineOffset
		relEnd := absEnd - lineOffset
		if relStart < 0 {
			relStart = 0
		}
		if relEnd >= len(lines) {
			relEnd = len(lines) - 1
		}
		if relEnd < relStart {
			relEnd = relStart
		}

		lines = lines[relStart : relEnd+1]
		lineOffset = absStart
		result.StartLine = absStart
		result.EndLine = absEnd
	}

	if req.Head > 0 && req.Head < len(lines) {
		lines = lines[:req.Head]
		result.StartLine = lineOffset
		result.EndLine = lineOffset + req.Head - 1
	}

	if req.Tail > 0 && req.Tail < len(lines) {
		tailStart := len(lines) - req.Tail
		lineOffset += tailStart
		lines = lines[tailStart:]
		result.StartLine = lineOffset
		result.EndLine = lineOffset + len(lines) - 1
	}

	if req.Pattern != "" {
		matches, err := s.searchPattern(lines, lineOffset, req)
		if err != nil {
			return nil, err
		}
		result.Matches = matches
		if len(matches) > 0 {
			result.Source = formatMatches(matches)
		} else {
			result.Source = ""
		}
		return result, nil
	}

	if req.LineNumbers {
		result.Source = formatWithLineNumbers(lines, lineOffset)
	} else {
		result.Source = strings.Join(lines, "\n")
	}

	if result.StartLine == 0 && result.EndLine == 0 {
		result.StartLine = lineOffset
		result.EndLine = lineOffset + len(lines) - 1
	}

	return result, nil
}

func (s *SourceService) fetchFileSource(ctx context.Context, path string) (string, error) {
	res, err := s.db.Query(ctx,
		`MATCH (f:File {path: $path}) RETURN f.source AS source`,
		map[string]any{"path": path})
	if err != nil {
		return "", fmt.Errorf("query file source: %w", err)
	}

	if len(res.Records) == 0 {
		return "", fmt.Errorf("source not found for path %q", path)
	}

	src, ok := res.Records[0]["source"].(string)
	if !ok || src == "" {
		return "", fmt.Errorf("file source is empty: %s", path)
	}
	return src, nil
}

func (s *SourceService) resolveEntity(ctx context.Context, req SourceRequest) (*EntityInfo, error) {
	var cypher string
	params := map[string]any{
		"name": req.Entity,
		"path": req.Path,
	}

	if req.EntityType != "" {
		cypher = fmt.Sprintf(
			`MATCH (e:%s) WHERE toLower(e.name) = toLower($name) AND e.path = $path RETURN e.name AS name, label(e) AS type, e.line_number AS start_line, e.end_line AS end_line LIMIT 1`,
			"`"+req.EntityType+"`")
	} else {
		cypher = `MATCH (e) WHERE toLower(e.name) = toLower($name) AND e.path = $path AND e.line_number IS NOT NULL AND e.end_line IS NOT NULL RETURN e.name AS name, label(e) AS type, e.line_number AS start_line, e.end_line AS end_line LIMIT 1`
	}

	res, err := s.db.Query(ctx, cypher, params)
	if err != nil {
		return nil, fmt.Errorf("resolve entity %q: %w", req.Entity, err)
	}

	if len(res.Records) == 0 {
		return nil, fmt.Errorf("entity %q not found in %q", req.Entity, req.Path)
	}

	rec := res.Records[0]
	info := &EntityInfo{}

	if v, ok := rec["name"].(string); ok {
		info.Name = v
	}
	if v, ok := rec["type"].(string); ok {
		info.Type = v
	}
	info.StartLine = toInt(rec["start_line"])
	info.EndLine = toInt(rec["end_line"])

	if info.StartLine <= 0 || info.EndLine <= 0 {
		return nil, fmt.Errorf("entity %q has no line range information", req.Entity)
	}

	return info, nil
}

func (s *SourceService) searchPattern(lines []string, lineOffset int, req SourceRequest) ([]SourceMatch, error) {
	var matcher func(string) bool

	if req.IsRegex {
		re, err := regexp.Compile(req.Pattern)
		if err != nil {
			return nil, fmt.Errorf("invalid regex pattern %q: %w", req.Pattern, err)
		}
		matcher = re.MatchString
	} else {
		patLower := strings.ToLower(req.Pattern)
		matcher = func(line string) bool {
			return strings.Contains(strings.ToLower(line), patLower)
		}
	}

	matchLines := make([]int, 0)
	for i, line := range lines {
		if matcher(line) {
			matchLines = append(matchLines, i)
		}
	}

	if len(matchLines) == 0 {
		return nil, nil
	}

	included := make(map[int]bool)
	for _, mi := range matchLines {
		start := mi - req.Before
		if start < 0 {
			start = 0
		}
		end := mi + req.After
		if end >= len(lines) {
			end = len(lines) - 1
		}
		for j := start; j <= end; j++ {
			included[j] = true
		}
	}

	matchSet := make(map[int]bool, len(matchLines))
	for _, mi := range matchLines {
		matchSet[mi] = true
	}

	sortedIndices := make([]int, 0, len(included))
	for idx := range included {
		sortedIndices = append(sortedIndices, idx)
	}
	sortInts(sortedIndices)

	var matches []SourceMatch
	for _, idx := range sortedIndices {
		matches = append(matches, SourceMatch{
			LineNumber: lineOffset + idx,
			Line:       lines[idx],
			IsMatch:    matchSet[idx],
		})
	}

	return matches, nil
}

func formatMatches(matches []SourceMatch) string {
	var sb strings.Builder
	prevLine := -1
	for i, m := range matches {
		if i > 0 && m.LineNumber > prevLine+1 {
			sb.WriteString("---\n")
		}
		marker := " "
		if m.IsMatch {
			marker = ">"
		}
		fmt.Fprintf(&sb, "%s %4d: %s\n", marker, m.LineNumber, m.Line)
		prevLine = m.LineNumber
	}
	return strings.TrimRight(sb.String(), "\n")
}

func formatWithLineNumbers(lines []string, offset int) string {
	var sb strings.Builder
	for i, line := range lines {
		fmt.Fprintf(&sb, "%4d: %s\n", offset+i, line)
	}
	return strings.TrimRight(sb.String(), "\n")
}



func sortInts(a []int) {
	for i := 1; i < len(a); i++ {
		for j := i; j > 0 && a[j] < a[j-1]; j-- {
			a[j], a[j-1] = a[j-1], a[j]
		}
	}
}
