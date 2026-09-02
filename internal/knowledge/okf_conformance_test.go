//go:build lancedb

package knowledge

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// OKF v0.2 §11 states conformance as three checkable properties. This is that check, run against
// markdown the pipeline actually produced rather than against a hand-written sample.
//
// It exists because the previous OKF pass was verified by updating the tests to the new output —
// which proves the output did not change unexpectedly, and proves nothing about the spec. Every
// assertion below cites the section it enforces.
//
// WHAT CHANGED: the generator no longer writes markdown, so the subject is the EXPORT. The
// conformance is the same conformance and matters for the same reason — the exported tree is what an
// Obsidian vault or a git repository receives — and it is now asserted against a renderer that
// marshals its frontmatter with a YAML encoder instead of assembling it with Fprintf, which is what
// the `Storage: where it lives` case below is really testing.
func TestExportedWikiConformsToOKF(t *testing.T) {
	src := t.TempDir()
	docs := filepath.Join(src, "docs")
	if err := os.MkdirAll(filepath.Join(docs, "architecture"), 0o755); err != nil {
		t.Fatal(err)
	}

	write := func(rel, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(docs, rel), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	// A description lifted from a folded scalar, a title with a colon, and a body linking
	// to a repository file: the three shapes that broke the frontmatter or the link graph.
	write("architecture/storage.md", "---\ntitle: \"Storage: where it lives\"\ndescription: >\n  Where every artifact lives\n---\n\n# Storage\n\nSee [pipeline.go](../../internal/ast/pipeline.go) and [Routing](routing.md).\n")
	write("architecture/routing.md", "# Routing\n\nRoutes.\n")

	wikiDir := t.TempDir()
	if _, err := GenerateKnowledgeWiki(context.Background(), src, wikiDir, nil, WikiScope{}); err != nil {
		t.Fatal(err)
	}

	out := t.TempDir()
	if _, err := wiki.ExportMarkdown(context.Background(), wikiDir, out, "knowledge"); err != nil {
		t.Fatal(err)
	}

	entries, err := os.ReadDir(out)
	if err != nil {
		t.Fatal(err)
	}

	var concepts int
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		data, err := os.ReadFile(filepath.Join(out, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		content := string(data)

		// §3.1 + §8 + §9: the reserved filenames are not concept documents.
		if e.Name() == "index.md" {
			assertIndexFile(t, content)
			continue
		}
		if e.Name() == "log.md" {
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
			t.Errorf("%s: §11.2 — `type` is the one required field and it is missing or empty", e.Name())
		}

		// §5.2: `generated` is a mapping, and `by` is REQUIRED inside it. Not a flat
		// `generated.at` key, which is the spec's prose notation and not a field.
		if _, dotted := doc["generated.at"]; dotted {
			t.Errorf("%s: §5.2 — `generated.at` is not an OKF key; `generated` is a mapping", e.Name())
		}
		gen, ok := doc["generated"].(map[string]any)
		if !ok {
			t.Errorf("%s: §5.2 — `generated` must be a mapping, got %T", e.Name(), doc["generated"])
		} else if by, _ := gen["by"].(string); strings.TrimSpace(by) == "" {
			t.Errorf("%s: §5.2 — `generated.by` is REQUIRED", e.Name())
		} else if strings.HasPrefix(by, "human:") {
			t.Errorf("%s: §5.3 — a generated page must not claim a human actor (%q)", e.Name(), by)
		}

		// §5.1: every `sources` entry is a mapping whose `resource` is REQUIRED.
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
		t.Fatal("no concept pages were generated")
	}
}

func assertIndexFile(t *testing.T, content string) {
	t.Helper()
	fm, ok := wiki.FrontmatterBlock(content)
	if !ok {
		return
	}
	// §8: an index carries no frontmatter, with one exception — a bundle-root index.md MAY
	// declare okf_version, and §12 makes that the only place a bundle may declare it.
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
	if v, _ := doc["okf_version"].(string); v != wiki.OKFVersion {
		t.Errorf("index.md: §12 — okf_version = %q, want %q", v, wiki.OKFVersion)
	}
}

// §6.1 says a concept links to another concept with a plain markdown link — which is also
// what a link to a source file looks like. Only the first is a cross-reference.
func TestGeneratedPagesDoNotCrossReferenceRepositoryPaths(t *testing.T) {
	src := t.TempDir()
	docs := filepath.Join(src, "docs")
	if err := os.MkdirAll(docs, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(docs, "a.md"),
		[]byte("# A\n\nSee [the pipeline](../internal/ast/pipeline.go).\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	wikiDir := t.TempDir()
	if _, err := GenerateKnowledgeWiki(context.Background(), src, wikiDir, nil, WikiScope{}); err != nil {
		t.Fatal(err)
	}
	graph, err := wiki.BuildCrossRefGraphFromIndex(context.Background(), wikiDir)
	if err != nil {
		t.Fatal(err)
	}
	if broken := wiki.BrokenLinks(graph); len(broken) != 0 {
		t.Errorf("a link to a source file is not a broken cross-reference: %v", broken)
	}
}
