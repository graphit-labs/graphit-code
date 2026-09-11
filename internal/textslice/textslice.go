// Package textslice slices text the way a reader asks for it: a line range, the
// head or the tail, or the lines around a pattern.
//
// It exists because two very different sources need the identical treatment once
// the text is in hand — source code fetched from the code graph, and wiki pages
// read from a wiki directory. Only the fetching differs, so only the fetching
// lives in the callers.
package textslice

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// Request describes which part of the text the caller wants. A zero Request
// returns everything.
type Request struct {
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

// Match is one line emitted by a pattern search. Context lines are included with
// IsMatch false so the caller can tell a hit from its surroundings.
type Match struct {
	LineNumber int    `json:"line_number"`
	Line       string `json:"line"`
	IsMatch    bool   `json:"is_match"`
}

// Result carries the sliced text plus where it came from in the original.
type Result struct {
	Source     string  `json:"source"`
	TotalLines int     `json:"total_lines"`
	StartLine  int     `json:"start_line,omitempty"`
	EndLine    int     `json:"end_line,omitempty"`
	Matches    []Match `json:"matches,omitempty"`
}

// Apply slices text according to req.
//
// The operations compose in a fixed order — line range, then head, then tail,
// then pattern — so that combining them stays predictable: `start_line` picks the
// window and `head` takes the first lines of that window, not of the file.
func Apply(text string, req Request) (*Result, error) {
	allLines := strings.Split(text, "\n")
	result := &Result{TotalLines: len(allLines)}

	lines := allLines
	offset := 1

	if req.StartLine > 0 || req.EndLine > 0 {
		start := req.StartLine
		if start < 1 {
			start = 1
		}
		if start > len(allLines) {
			start = len(allLines)
		}
		end := req.EndLine
		if end <= 0 || end > len(allLines) {
			end = len(allLines)
		}
		if end < start {
			end = start
		}
		lines = allLines[start-1 : end]
		offset = start
		result.StartLine = start
		result.EndLine = end
	}

	if req.Head > 0 && req.Head < len(lines) {
		lines = lines[:req.Head]
		result.StartLine = offset
		result.EndLine = offset + req.Head - 1
	}

	if req.Tail > 0 && req.Tail < len(lines) {
		tailStart := len(lines) - req.Tail
		offset += tailStart
		lines = lines[tailStart:]
		result.StartLine = offset
		result.EndLine = offset + len(lines) - 1
	}

	if req.Pattern != "" {
		matches, err := Search(lines, offset, req.Pattern, req.IsRegex, req.Before, req.After)
		if err != nil {
			return nil, err
		}
		result.Matches = matches
		if len(matches) > 0 {
			result.Source = FormatMatches(matches)
		}
		return result, nil
	}

	if req.LineNumbers {
		result.Source = FormatWithLineNumbers(lines, offset)
	} else {
		result.Source = strings.Join(lines, "\n")
	}

	if result.StartLine == 0 && result.EndLine == 0 {
		result.StartLine = offset
		result.EndLine = offset + len(lines) - 1
	}

	return result, nil
}

// Search returns every line matching pattern, plus before/after context lines.
// Overlapping context windows are merged rather than repeated. A plain pattern is
// matched case-insensitively; a regex is used as written.
func Search(lines []string, offset int, pattern string, isRegex bool, before, after int) ([]Match, error) {
	var matcher func(string) bool
	if isRegex {
		re, err := regexp.Compile(pattern)
		if err != nil {
			return nil, fmt.Errorf("invalid regex pattern %q: %w", pattern, err)
		}
		matcher = re.MatchString
	} else {
		lower := strings.ToLower(pattern)
		matcher = func(line string) bool {
			return strings.Contains(strings.ToLower(line), lower)
		}
	}

	var hits []int
	for i, line := range lines {
		if matcher(line) {
			hits = append(hits, i)
		}
	}
	if len(hits) == 0 {
		return nil, nil
	}

	included := make(map[int]bool)
	for _, h := range hits {
		start := h - before
		if start < 0 {
			start = 0
		}
		end := h + after
		if end >= len(lines) {
			end = len(lines) - 1
		}
		for j := start; j <= end; j++ {
			included[j] = true
		}
	}

	isHit := make(map[int]bool, len(hits))
	for _, h := range hits {
		isHit[h] = true
	}

	indices := make([]int, 0, len(included))
	for idx := range included {
		indices = append(indices, idx)
	}
	sort.Ints(indices)

	matches := make([]Match, 0, len(indices))
	for _, idx := range indices {
		matches = append(matches, Match{
			LineNumber: offset + idx,
			Line:       lines[idx],
			IsMatch:    isHit[idx],
		})
	}
	return matches, nil
}

// FormatMatches renders matches with a ">" marker on hits and a "---" separator
// between non-adjacent groups, so a reader can see where lines were skipped.
func FormatMatches(matches []Match) string {
	var sb strings.Builder
	prev := -1
	for i, m := range matches {
		if i > 0 && m.LineNumber > prev+1 {
			sb.WriteString("---\n")
		}
		marker := " "
		if m.IsMatch {
			marker = ">"
		}
		_, _ = fmt.Fprintf(&sb, "%s %4d: %s\n", marker, m.LineNumber, m.Line)
		prev = m.LineNumber
	}
	return strings.TrimRight(sb.String(), "\n")
}

// FormatWithLineNumbers prefixes each line with its absolute number.
func FormatWithLineNumbers(lines []string, offset int) string {
	var sb strings.Builder
	for i, line := range lines {
		_, _ = fmt.Fprintf(&sb, "%4d: %s\n", offset+i, line)
	}
	return strings.TrimRight(sb.String(), "\n")
}
