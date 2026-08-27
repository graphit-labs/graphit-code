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
	// ItemExt is the extension of a queued backlog item.
	ItemExt = ".md"

	// ResultExt marks an item as closed: the agent that worked it writes
	// <slug>.done.md beside <slug>.md, and Pending stops returning it.
	ResultExt = ".done.md"
)

// Item is one unit of deferred work: something a review identified and
// deliberately did not do at the time.
type Item struct {
	Slug string `json:"slug"`

	Title string `json:"title"`

	Body string `json:"body,omitempty"`

	Path string `json:"path"`

	CreatedAt time.Time `json:"created_at"`

	Done bool `json:"done"`

	ResultPath string `json:"result_path,omitempty"`
}

// Dir returns the absolute backlog directory for a project, honouring
// improvements.backlog_dir.
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

	doneSet := make(map[string]string)
	for _, e := range entries {
		name := e.Name()
		if !e.IsDir() && strings.HasSuffix(name, ResultExt) {
			slug := strings.TrimSuffix(name, ResultExt)
			doneSet[slug] = filepath.Join(dir, name)
		}
	}

	var items []Item
	for _, e := range entries {
		name := e.Name()

		if e.IsDir() || !strings.HasSuffix(name, ItemExt) || strings.HasSuffix(name, ResultExt) {
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

		if resultPath, ok := doneSet[slug]; ok {
			item.Done = true
			item.ResultPath = resultPath
		}

		items = append(items, item)
	}

	sort.Slice(items, func(i, j int) bool {
		return items[i].CreatedAt.Before(items[j].CreatedAt)
	})

	return items, nil
}

func Pending(projectDir string) ([]Item, error) {
	all, err := List(projectDir)
	if err != nil {
		return nil, err
	}
	var pending []Item
	for _, item := range all {
		if !item.Done {
			pending = append(pending, item)
		}
	}
	return pending, nil
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

	resultPath := filepath.Join(dir, slug+ResultExt)
	_ = os.Remove(resultPath)

	return nil
}

// Pick returns the oldest pending item, or nil when the backlog is empty.
func Pick(projectDir string) (*Item, error) {
	pending, err := Pending(projectDir)
	if err != nil {
		return nil, err
	}
	if len(pending) == 0 {
		return nil, nil
	}
	return &pending[0], nil
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
