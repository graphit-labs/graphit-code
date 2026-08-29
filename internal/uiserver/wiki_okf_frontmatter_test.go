package uiserver

import "testing"

// The explorer read `tags: [a, b]` and `source:` — the shapes the generator emitted BEFORE
// the OKF move. After it, tags are a block sequence and provenance is `sources:` with a
// required `resource` per entry (OKF §5.1), so every page in the explorer showed no tags
// and no source. Both shapes are on disk at once, so both are read.
func TestExtractPageMetaReadsOKFFrontmatter(t *testing.T) {
	t.Parallel()
	content := "---\n" +
		"type: architecture\n" +
		"title: Storage Layout\n" +
		"generated: { by: process:graphit-knowledge-wiki, at: 2026-08-29 }\n" +
		"sources:\n" +
		"  - resource: docs/architecture/storage_layout.md\n" +
		"description: Where each store lives.\n" +
		"tags:\n" +
		"  - knowledge\n" +
		"  - architecture\n" +
		"confidence: 0.90\n" +
		"---\n\n# Storage Layout\n\nBody.\n"

	meta := extractPageMeta("Storage_Layout.md", content)

	if meta.Type != "architecture" {
		t.Errorf("Type = %q; OKF §4.1 makes `type` the one required field — it should be used", meta.Type)
	}
	if len(meta.Tags) != 2 || meta.Tags[0] != "knowledge" || meta.Tags[1] != "architecture" {
		t.Errorf("Tags = %v; want the block sequence", meta.Tags)
	}
	if meta.Source != "docs/architecture/storage_layout.md" {
		t.Errorf("Source = %q; want the first sources[].resource", meta.Source)
	}
	if meta.Confidence != 0.90 {
		t.Errorf("Confidence = %v", meta.Confidence)
	}
}

// Free-text values are quoted by the generator so the block always parses as YAML
// (OKF §11 conformance criterion 1). The reader has to undo that, or every quoted title,
// tag and source path would carry its quotes into the UI.
func TestExtractPageMetaUnquotesFrontmatterScalars(t *testing.T) {
	t.Parallel()
	content := "---\n" +
		"type: \"decision: storage\"\n" +
		"sources:\n" +
		"  - resource: \"docs/decisions/a: b.md\"\n" +
		"tags:\n" +
		"  - \"knowledge\"\n" +
		"  - decision\n" +
		"---\n\n# D\n"

	meta := extractPageMeta("D.md", content)
	if meta.Type != "decision: storage" {
		t.Errorf("Type = %q; quotes must not survive into the UI", meta.Type)
	}
	if meta.Source != "docs/decisions/a: b.md" {
		t.Errorf("Source = %q", meta.Source)
	}
	if len(meta.Tags) != 2 || meta.Tags[0] != "knowledge" || meta.Tags[1] != "decision" {
		t.Errorf("Tags = %v", meta.Tags)
	}
}

// The reserved filenames get their meaning from the NAME (OKF §3.1), and §8 says an index
// carries no frontmatter to read anyway.
func TestExtractPageMetaReservedFilenamesWinOverFrontmatter(t *testing.T) {
	t.Parallel()
	if got := extractPageMeta("index.md", "---\nokf_version: \"0.2\"\n---\n\n# Index\n").Type; got != "index" {
		t.Errorf("index.md Type = %q; want index", got)
	}
	if got := extractPageMeta("log.md", "# Log\n\n## 2026-08-29\n").Type; got != "log" {
		t.Errorf("log.md Type = %q; want log", got)
	}
}

// A `type:` line inside the body — a quoted example, a code block — is not metadata.
func TestExtractPageMetaIgnoresTypeInTheBody(t *testing.T) {
	t.Parallel()
	content := "---\ntype: decision\n---\n\n# D\n\n```yaml\ntype: not-this-one\n```\n"
	if got := extractPageMeta("D.md", content).Type; got != "decision" {
		t.Errorf("Type = %q; want decision", got)
	}
}
