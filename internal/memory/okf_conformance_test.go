package memory

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// The memory wiki is an OKF bundle too, and the first pass at OKF converted only its
// concept pages: index.md kept a pre-OKF frontmatter block and emitted every catalog entry
// as a [[wikilink]], and log.md was never touched. This test covers the whole bundle.
func TestGeneratedMemoryWikiConformsToOKF(t *testing.T) {
	rawDir := t.TempDir()
	wikiDir := t.TempDir()

	// A title with a colon and a body whose first line is a folded-scalar marker: the
	// shapes that produce unparseable frontmatter when the block is built by concatenation.
	writeMem := func(name, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(rawDir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	writeMem("MEM1.md", "---\ntitle: \"Storage: one store, not two\"\ntype: decision\ncreated_at: 2026-05-20T00:00:00Z\n---\n\n# Storage\n\n> Chose one store over two.\n")
	writeMem("MEM2.md", "---\ntitle: Always run make install\ntype: convention\nimportant: true\ncreated_at: 2026-05-21T00:00:00Z\n---\n\n# Install\n\nRun it first.\n")

	if _, err := GenerateMemoryWiki(context.Background(), rawDir, wikiDir); err != nil {
		t.Fatal(err)
	}

	var concepts int
	entries, err := os.ReadDir(wikiDir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		data, err := os.ReadFile(filepath.Join(wikiDir, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		content := string(data)

		switch e.Name() {
		case "index.md":
			assertMemoryIndex(t, content)
			continue
		case "log.md":
			assertMemoryLog(t, content)
			continue
		}

		concepts++
		fm, ok := wiki.FrontmatterBlock(content)
		if !ok {
			t.Errorf("%s: §11.1 — no frontmatter block", e.Name())
			continue
		}
		var doc map[string]any
		if err := yaml.Unmarshal([]byte(fm), &doc); err != nil {
			t.Errorf("%s: §11.1 — frontmatter does not parse as YAML: %v\n%s", e.Name(), err, fm)
			continue
		}
		if typ, _ := doc["type"].(string); strings.TrimSpace(typ) == "" {
			t.Errorf("%s: §11.2 — `type` is missing or empty", e.Name())
		}
		if _, dotted := doc["generated.at"]; dotted {
			t.Errorf("%s: §5.2 — `generated.at` is not an OKF key", e.Name())
		}
		gen, ok := doc["generated"].(map[string]any)
		if !ok {
			t.Errorf("%s: §5.2 — `generated` must be a mapping, got %T", e.Name(), doc["generated"])
		} else if by, _ := gen["by"].(string); strings.TrimSpace(by) == "" {
			t.Errorf("%s: §5.2 — `generated.by` is REQUIRED", e.Name())
		}
		srcs, ok := doc["sources"].([]any)
		if !ok {
			t.Errorf("%s: §5.1 — `sources` must be a list, got %T", e.Name(), doc["sources"])
			continue
		}
		for i, raw := range srcs {
			entry, ok := raw.(map[string]any)
			if !ok {
				t.Errorf("%s: §5.1 — sources[%d] must be a mapping, got %T", e.Name(), i, raw)
				continue
			}
			if res, _ := entry["resource"].(string); strings.TrimSpace(res) == "" {
				t.Errorf("%s: §5.1 — sources[%d].resource is REQUIRED", e.Name(), i)
			}
		}
	}

	if concepts == 0 {
		t.Fatal("no memory pages were generated")
	}
}

func assertMemoryIndex(t *testing.T, content string) {
	t.Helper()
	// §6.1: links between concepts are standard markdown links. The catalog used to be
	// [[wikilinks]], which no OKF consumer resolves.
	if strings.Contains(content, "[[") {
		t.Errorf("index.md: §6.1 — a [[wikilink]] survived:\n%s", content)
	}
	fm, ok := wiki.FrontmatterBlock(content)
	if !ok {
		return
	}
	var doc map[string]any
	if err := yaml.Unmarshal([]byte(fm), &doc); err != nil {
		t.Errorf("index.md: frontmatter does not parse: %v", err)
		return
	}
	for k := range doc {
		if k != "okf_version" {
			t.Errorf("index.md: §8 — an index may only carry `okf_version`, found %q", k)
		}
	}
}

func assertMemoryLog(t *testing.T, content string) {
	t.Helper()
	if strings.HasPrefix(strings.TrimSpace(content), "---") {
		t.Error("log.md: §9 — a log file carries no frontmatter")
	}
	if !strings.Contains(content, "\n## 2") {
		t.Errorf("log.md: §9 — entries must be grouped under `## YYYY-MM-DD`:\n%s", content)
	}
	if !strings.Contains(content, "* **") {
		t.Errorf("log.md: §9 — entries lead with a bold kind word:\n%s", content)
	}
}
