package backlog

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode"

	"github.com/graphit-labs/graphit-code/internal/config"
	"golang.org/x/text/unicode/norm"
)

const (
	// ItemExt is the extension of a registered backlog item.
	ItemExt = ".md"

	legacyResultExt = ".done.md"
)

// Item is one unit of deferred work: something a review identified and
// deliberately did not do at the time.
type Item struct {
	Slug string `json:"slug"`

	Title string `json:"title"`

	Body string `json:"body,omitempty"`

	Path string `json:"path"`

	CreatedAt time.Time `json:"created_at"`
}

// Dir returns the absolute backlog directory for a project, honouring
// backlog.dir.
func Dir(projectDir string) string {
	return filepath.Join(projectDir, config.ResolveBacklogDir(nil, config.LoadProjectConfig(projectDir)))
}

func Add(projectDir, title, body string) (*Item, error) {
	dir := Dir(projectDir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, fmt.Errorf("creating backlog dir: %w", err)
	}

	slug := slugify(title)
	if slug == "" {
		return nil, fmt.Errorf("title produces an empty slug — use alphanumeric characters")
	}

	path := filepath.Join(dir, slug+ItemExt)

	if _, err := os.Stat(path); err == nil {
		return nil, fmt.Errorf("backlog item %q already exists at %s", slug, path)
	}

	var content strings.Builder
	_, _ = fmt.Fprintf(&content, "# %s\n\n", title)
	if body != "" {
		content.WriteString(body)
		if !strings.HasSuffix(body, "\n") {
			content.WriteString("\n")
		}
	}

	if err := os.WriteFile(path, []byte(content.String()), 0o644); err != nil {
		return nil, fmt.Errorf("writing backlog item: %w", err)
	}

	return &Item{
		Slug:      slug,
		Title:     title,
		Body:      content.String(),
		Path:      path,
		CreatedAt: time.Now(),
	}, nil
}

func List(projectDir string) ([]Item, error) {
	dir := Dir(projectDir)

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("reading backlog dir: %w", err)
	}

	var items []Item
	for _, e := range entries {
		name := e.Name()

		if e.IsDir() || !strings.HasSuffix(name, ItemExt) || strings.HasSuffix(name, legacyResultExt) {
			continue
		}

		slug := strings.TrimSuffix(name, ItemExt)
		path := filepath.Join(dir, name)

		info, err := e.Info()
		if err != nil {
			continue
		}

		item := Item{
			Slug:      slug,
			Path:      path,
			CreatedAt: info.ModTime(),
		}

		if data, err := os.ReadFile(path); err == nil {
			item.Body = string(data)
			item.Title = extractTitle(string(data), slug)
		} else {
			item.Title = slug
		}

		items = append(items, item)
	}

	sort.Slice(items, func(i, j int) bool {
		return items[i].CreatedAt.Before(items[j].CreatedAt)
	})

	return items, nil
}

func Remove(projectDir, slug string) error {
	dir := Dir(projectDir)

	itemPath := filepath.Join(dir, slug+ItemExt)
	if _, err := os.Stat(itemPath); os.IsNotExist(err) {
		return fmt.Errorf("backlog item %q not found", slug)
	}

	if err := os.Remove(itemPath); err != nil {
		return fmt.Errorf("removing backlog item: %w", err)
	}

	return nil
}

func slugify(title string) string {

	s := strings.ToLower(norm.NFKD.String(title))

	var clean strings.Builder
	for _, r := range s {
		if !unicode.Is(unicode.Mn, r) {
			clean.WriteRune(r)
		}
	}
	s = clean.String()

	re := regexp.MustCompile(`[^a-z0-9]+`)
	s = re.ReplaceAllString(s, "-")

	s = strings.Trim(s, "-")

	if len(s) > 60 {
		s = s[:60]

		s = strings.TrimRight(s, "-")
	}

	return s
}

func extractTitle(content, fallback string) string {
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "# ") {
			return strings.TrimPrefix(line, "# ")
		}
	}
	return fallback
}
