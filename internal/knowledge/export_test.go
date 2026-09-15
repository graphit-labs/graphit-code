//go:build lancedb

package knowledge

import (
	"archive/zip"
	"context"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func indexedKnowledgeForExport(t *testing.T) string {
	t.Helper()
	wikiDir := t.TempDir()
	chunk := wiki.WikiChunk{
		Slug: "alpha", Title: "Alpha", Body: "Body of Alpha.", Summary: "Summary of Alpha.",
		DocType: "document", Source: "docs/alpha.md", Updated: "2026-08-29", WordCount: 4,
		ClusterID: -1,
	}
	if err := wiki.SyncDB(context.Background(), wikiDir, []wiki.WikiChunk{chunk}, nil, nil); err != nil {
		t.Fatalf("building Knowledge index: %v", err)
	}
	return wikiDir
}

func TestExportPackageCarriesValidatedKnowledgeIndex(t *testing.T) {
	wikiDir := indexedKnowledgeForExport(t)
	out := filepath.Join(t.TempDir(), "docs.knowledge")
	if err := ExportPackage(context.Background(), wikiDir, out); err != nil {
		t.Fatalf("ExportPackage: %v", err)
	}

	zr, err := zip.OpenReader(out)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = zr.Close() }()
	var manifest artifactpackage.Manifest
	hasIndexFile := false
	for _, member := range zr.File {
		if member.Name == artifactpackage.ManifestName {
			rc, err := member.Open()
			if err != nil {
				t.Fatal(err)
			}
			raw, err := io.ReadAll(rc)
			_ = rc.Close()
			if err != nil {
				t.Fatal(err)
			}
			if err := json.Unmarshal(raw, &manifest); err != nil {
				t.Fatal(err)
			}
		}
		if strings.HasPrefix(member.Name, wiki.WikiIndexDirName+"/") && !member.FileInfo().IsDir() {
			hasIndexFile = true
		}
	}
	if manifest.ArtifactType != "knowledge" || manifest.Version != artifactpackage.Version {
		t.Fatalf("manifest = %#v", manifest)
	}
	if !hasIndexFile {
		t.Fatal("package contains no native index file")
	}
}

func TestExportFormatsKeepExplicitOKFAndObsidianEntrypoints(t *testing.T) {
	wikiDir := indexedKnowledgeForExport(t)
	okfDir := filepath.Join(t.TempDir(), "okf")
	obsidianDir := filepath.Join(t.TempDir(), "obsidian")
	if _, err := ExportOKF(context.Background(), wikiDir, okfDir, "knowledge"); err != nil {
		t.Fatalf("ExportOKF: %v", err)
	}
	if _, err := ExportObsidian(context.Background(), wikiDir, obsidianDir, "knowledge"); err != nil {
		t.Fatalf("ExportObsidian: %v", err)
	}
	for _, dir := range []string{okfDir, obsidianDir} {
		data, err := os.ReadFile(filepath.Join(dir, "alpha.md"))
		if err != nil {
			t.Fatal(err)
		}
		page := string(data)
		if !strings.Contains(page, "generated:") || !strings.Contains(page, "# Alpha") {
			t.Fatalf("export at %s is neither OKF nor a navigable vault:\n%s", dir, page)
		}
	}
}
