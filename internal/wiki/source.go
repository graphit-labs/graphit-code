package wiki

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/textslice"
)

// ErrPageNotFound reports a page reference that resolved to nothing. Callers use
// it to decide whether listing the available pages would help — it would for a
// mistyped slug, and it would not for a reference that was refused for escaping
// the wiki directory, where suggesting alternatives only buries the reason.
var ErrPageNotFound = errors.New("wiki page not found")

// PageResult is one wiki page, sliced as requested.
type PageResult struct {
	Page       string            `json:"page"`
	File       string            `json:"file"`
	Title      string            `json:"title,omitempty"`
	Source     string            `json:"source"`
	TotalLines int               `json:"total_lines"`
	StartLine  int               `json:"start_line,omitempty"`
	EndLine    int               `json:"end_line,omitempty"`
	Matches    []textslice.Match `json:"matches,omitempty"`
}

// ReadPage returns the content of one wiki page, sliced according to req.
//
// It exists because an agent is frequently confined to its own workspace, and a
// wiki page it needs may live under another project's directory. Every other wiki
// tool already takes the project as a parameter and reads on the agent's behalf;
// reading the page itself was the one step that fell back to a direct file read,
// which is exactly the step that fails across projects.
//
// page accepts what the other wiki tools hand back: a slug ("auth-flow"), the same
// slug with its extension, or a path relative to the wiki directory. Lookup is
// case-insensitive, because a slug that came from a title rarely matches the file
// name exactly.
func ReadPage(wikiDir, page string, req textslice.Request) (*PageResult, error) {
	if wikiDir == "" {
		return nil, fmt.Errorf("wiki directory not found — the wiki may not have been built yet")
	}
	if strings.TrimSpace(page) == "" {
		return nil, fmt.Errorf("page is required")
	}

	file, err := resolvePageFile(wikiDir, page)
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(file)
	if err != nil {
		return nil, fmt.Errorf("reading wiki page %q: %w", page, err)
	}

	sliced, err := textslice.Apply(string(data), req)
	if err != nil {
		return nil, err
	}

	rel, relErr := filepath.Rel(wikiDir, file)
	if relErr != nil {
		rel = filepath.Base(file)
	}

	return &PageResult{
		Page:       strings.TrimSuffix(rel, ".md"),
		File:       rel,
		Title:      firstHeading(string(data)),
		Source:     sliced.Source,
		TotalLines: sliced.TotalLines,
		StartLine:  sliced.StartLine,
		EndLine:    sliced.EndLine,
		Matches:    sliced.Matches,
	}, nil
}

// ListPages returns the page names available in wikiDir, so a caller that guessed
// wrong can be told what does exist instead of just "not found".
func ListPages(wikiDir string) []string {
	entries, err := os.ReadDir(wikiDir)
	if err != nil {
		return nil
	}
	var pages []string
	for _, e := range entries {
		if e.IsDir() || !strings.EqualFold(filepath.Ext(e.Name()), ".md") {
			continue
		}
		pages = append(pages, strings.TrimSuffix(e.Name(), filepath.Ext(e.Name())))
	}
	return pages
}

// resolvePageFile maps a caller-supplied page reference onto a file inside
// wikiDir, refusing anything that escapes it.
func resolvePageFile(wikiDir, page string) (string, error) {
	if filepath.IsAbs(page) {
		return "", fmt.Errorf("page must be relative to the wiki directory, got absolute path %q", page)
	}

	cleaned := filepath.Clean(filepath.FromSlash(page))
	if cleaned == ".." || strings.HasPrefix(cleaned, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("page %q escapes the wiki directory", page)
	}

	base, err := filepath.Abs(wikiDir)
	if err != nil {
		return "", err
	}

	candidates := []string{cleaned}
	if !strings.EqualFold(filepath.Ext(cleaned), ".md") {
		candidates = append(candidates, cleaned+".md")
	}

	for _, c := range candidates {
		full := filepath.Join(base, c)
		if !withinDir(base, full) {
			return "", fmt.Errorf("page %q escapes the wiki directory", page)
		}
		if info, statErr := os.Stat(full); statErr == nil && !info.IsDir() {
			return full, nil
		}
	}

	// Wiki file names are generated from titles, so an exact match is the
	// exception rather than the rule. Fall back to a case-insensitive scan.
	if match := findPageInsensitive(base, cleaned); match != "" {
		return match, nil
	}

	return "", fmt.Errorf("%w: %q in %s", ErrPageNotFound, page, wikiDir)
}

func findPageInsensitive(base, cleaned string) string {
	dir := filepath.Join(base, filepath.Dir(cleaned))
	if !withinDir(base, dir) {
		return ""
	}
	wanted := strings.ToLower(filepath.Base(cleaned))
	wantedMD := wanted
	if !strings.HasSuffix(wantedMD, ".md") {
		wantedMD += ".md"
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		return ""
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := strings.ToLower(e.Name())
		if name == wanted || name == wantedMD {
			return filepath.Join(dir, e.Name())
		}
	}
	return ""
}

func withinDir(base, target string) bool {
	rel, err := filepath.Rel(base, target)
	if err != nil {
		return false
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

// firstHeading returns the page's first markdown heading, skipping YAML
// frontmatter, so the caller can confirm it opened the page it meant to.
func firstHeading(content string) string {
	lines := strings.Split(content, "\n")
	i := 0
	if len(lines) > 0 && strings.TrimSpace(lines[0]) == "---" {
		for i = 1; i < len(lines); i++ {
			if strings.TrimSpace(lines[i]) == "---" {
				i++
				break
			}
		}
	}
	for ; i < len(lines); i++ {
		line := strings.TrimSpace(lines[i])
		if strings.HasPrefix(line, "#") {
			return strings.TrimSpace(strings.TrimLeft(line, "#"))
		}
	}
	return ""
}
