package ast

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/textslice"
)

type SourceRequest struct {
	Path        string
	Entity      string
	EntityType  string
	Head        int
	Tail        int
	StartLine   int
	EndLine     int
	Pattern     string
	IsRegex     bool
	Before      int
	After       int
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

// SourceService serves indexed file text out of the search index.
//
// The graph is not consulted for it. File text lived in File.source as well, which
// meant every rebuild pushed the whole repository through one COPY — 2.4 GB on a
// 36k-file export — and when that COPY failed the graph published File nodes with no
// text and nothing said so. The search index is the only copy that is queryable, so
// it is now the only copy: one store, one writer, no divergence to reconcile.
type SourceService struct {
	db        GraphDB
	indexPath string
}

func NewSourceService(db GraphDB) *SourceService {
	return &SourceService{db: db}
}

// WithStore points the service at the graph store whose SearchFile rows hold the text.
func (s *SourceService) WithStore(dbPath string) *SourceService {
	if dbPath != "" {
		s.indexPath = dbPath
	}
	return s
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
	if s.indexPath == "" {
		return "", fmt.Errorf("source unavailable for %q: this service was built without a "+
			"store, so it cannot reach the search index that holds file text", path)
	}
	if src, ok := FileSourceAt(ctx, s.indexPath, path); ok {
		return src, nil
	}
	return "", fmt.Errorf("source not found for path %q: not in the search index. Either the "+
		"file is not indexed, or ast.index_source is false so the graph holds structure "+
		"without text", path)
}

// entityMatchExact and entityMatchFold are the same lookup twice: once comparing names as
// they are, once comparing them case-insensitively.
//
// The exact form runs first, and not only because it is cheaper. `toLower` over an
// unlabelled `MATCH (e)` asks the database to lowercase the name of every node in the
// project, so a single row holding bytes LOWER refuses — which the storage layer is known to
// produce occasionally, see docs/upstream/liblbug-string-corruption.md — failed every
// entity lookup in the project, including the thousands of entities whose own names are
// perfectly fine. Identifiers match exactly nearly always; folding is the fallback, so the
// blast radius of one bad row shrinks to the lookups that actually need it.
const (
	entityMatchExact = "e.name = $name"
	entityMatchFold  = "toLower(e.name) = toLower($name)"
)

func (s *SourceService) resolveEntity(ctx context.Context, req SourceRequest) (*EntityInfo, error) {
	params := map[string]any{
		"name": req.Entity,
		"path": req.Path,
	}

	query := func(nameMatch string) string {
		if req.EntityType != "" {
			return fmt.Sprintf(
				`MATCH (e:%s) WHERE %s AND e.path = $path RETURN e.name AS name, label(e) AS type, e.line_number AS start_line, e.end_line AS end_line LIMIT 1`,
				"`"+req.EntityType+"`", nameMatch)
		}
		return fmt.Sprintf(
			`MATCH (e) WHERE %s AND e.path = $path AND e.line_number IS NOT NULL AND e.end_line IS NOT NULL RETURN e.name AS name, label(e) AS type, e.line_number AS start_line, e.end_line AS end_line LIMIT 1`,
			nameMatch)
	}

	res, err := s.db.Query(ctx, query(entityMatchExact), params)
	if err != nil {
		return nil, fmt.Errorf("resolve entity %q: %w", req.Entity, err)
	}

	if len(res.Records) == 0 {
		foldRes, foldErr := s.db.Query(ctx, query(entityMatchFold), params)
		switch {
		case foldErr != nil:
			return nil, fmt.Errorf("entity %q not found in %q, and the case-insensitive "+
				"retry failed — a value in this graph cannot be lowercased, which a "+
				"reindex of the affected file repairs: %w", req.Entity, req.Path, foldErr)
		case len(foldRes.Records) == 0:
			return nil, fmt.Errorf("entity %q not found in %q", req.Entity, req.Path)
		default:
			res = foldRes
		}
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
	matches, err := textslice.Search(lines, lineOffset, req.Pattern, req.IsRegex, req.Before, req.After)
	if err != nil {
		return nil, err
	}
	out := make([]SourceMatch, 0, len(matches))
	for _, m := range matches {
		out = append(out, SourceMatch(m))
	}
	return out, nil
}

func formatMatches(matches []SourceMatch) string {
	converted := make([]textslice.Match, 0, len(matches))
	for _, m := range matches {
		converted = append(converted, textslice.Match(m))
	}
	return textslice.FormatMatches(converted)
}

func formatWithLineNumbers(lines []string, offset int) string {
	return textslice.FormatWithLineNumbers(lines, offset)
}
