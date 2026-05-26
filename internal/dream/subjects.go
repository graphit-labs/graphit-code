package dream

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"golang.org/x/text/unicode/norm"
)

const (
	subjectsDir = "subjects"

	subjectExt = ".md"

	resultExt = ".done.md"
)

type Subject struct {
	Slug string

	Title string

	Body string

	Path string

	CreatedAt time.Time

	Done bool

	ResultPath string
}

func SubjectsDir(projectDir string) string {
	return filepath.Join(projectDir, brand.DotDir(), "dream", subjectsDir)
}

func AddSubject(projectDir, title, body string) (*Subject, error) {
	dir := SubjectsDir(projectDir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, fmt.Errorf("creating subjects dir: %w", err)
	}

	slug := slugify(title)
	if slug == "" {
		return nil, fmt.Errorf("title produces an empty slug — use alphanumeric characters")
	}

	path := filepath.Join(dir, slug+subjectExt)

	if _, err := os.Stat(path); err == nil {
		return nil, fmt.Errorf("subject %q already exists at %s", slug, path)
	}

	var content strings.Builder
	content.WriteString(fmt.Sprintf("# %s\n\n", title))
	if body != "" {
		content.WriteString(body)
		if !strings.HasSuffix(body, "\n") {
			content.WriteString("\n")
		}
	}

	if err := os.WriteFile(path, []byte(content.String()), 0o644); err != nil {
		return nil, fmt.Errorf("writing subject file: %w", err)
	}

	return &Subject{
		Slug:      slug,
		Title:     title,
		Body:      content.String(),
		Path:      path,
		CreatedAt: time.Now(),
	}, nil
}

func ListSubjects(projectDir string) ([]Subject, error) {
	dir := SubjectsDir(projectDir)

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("reading subjects dir: %w", err)
	}

	doneSet := make(map[string]string)
	for _, e := range entries {
		name := e.Name()
		if !e.IsDir() && strings.HasSuffix(name, resultExt) {
			slug := strings.TrimSuffix(name, resultExt)
			doneSet[slug] = filepath.Join(dir, name)
		}
	}

	var subjects []Subject
	for _, e := range entries {
		name := e.Name()

		if e.IsDir() || !strings.HasSuffix(name, subjectExt) || strings.HasSuffix(name, resultExt) {
			continue
		}

		slug := strings.TrimSuffix(name, subjectExt)
		path := filepath.Join(dir, name)

		info, err := e.Info()
		if err != nil {
			continue
		}

		subj := Subject{
			Slug:      slug,
			Path:      path,
			CreatedAt: info.ModTime(),
		}

		if data, err := os.ReadFile(path); err == nil {
			subj.Body = string(data)
			subj.Title = extractTitle(string(data), slug)
		} else {
			subj.Title = slug
		}

		if resultPath, ok := doneSet[slug]; ok {
			subj.Done = true
			subj.ResultPath = resultPath
		}

		subjects = append(subjects, subj)
	}

	sort.Slice(subjects, func(i, j int) bool {
		return subjects[i].CreatedAt.Before(subjects[j].CreatedAt)
	})

	return subjects, nil
}

func PendingSubjects(projectDir string) ([]Subject, error) {
	all, err := ListSubjects(projectDir)
	if err != nil {
		return nil, err
	}
	var pending []Subject
	for _, s := range all {
		if !s.Done {
			pending = append(pending, s)
		}
	}
	return pending, nil
}

func RemoveSubject(projectDir, slug string) error {
	dir := SubjectsDir(projectDir)

	subjectPath := filepath.Join(dir, slug+subjectExt)
	if _, err := os.Stat(subjectPath); os.IsNotExist(err) {
		return fmt.Errorf("subject %q not found", slug)
	}

	if err := os.Remove(subjectPath); err != nil {
		return fmt.Errorf("removing subject: %w", err)
	}

	resultPath := filepath.Join(dir, slug+resultExt)
	os.Remove(resultPath)

	return nil
}

func PickSubject(projectDir string) (*Subject, error) {
	pending, err := PendingSubjects(projectDir)
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
