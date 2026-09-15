//go:build lancedb

package wiki

import (
	"archive/zip"
	"context"
	"encoding/json"
	"io"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
)

func TestExportPackageCarriesValidatedKnowledgeIndex(t *testing.T) {
	wikiDir := indexedWiki(t, []WikiChunk{exportChunk("alpha", "Alpha")})
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
		if strings.HasPrefix(member.Name, WikiIndexDirName+"/") && !member.FileInfo().IsDir() {
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
	wikiDir := indexedWiki(t, []WikiChunk{exportChunk("alpha", "Alpha")})
	okfDir := filepath.Join(t.TempDir(), "okf")
	obsidianDir := filepath.Join(t.TempDir(), "obsidian")
	if _, err := ExportOKF(context.Background(), wikiDir, okfDir, "knowledge"); err != nil {
		t.Fatalf("ExportOKF: %v", err)
	}
	if _, err := ExportObsidian(context.Background(), wikiDir, obsidianDir, "knowledge"); err != nil {
		t.Fatalf("ExportObsidian: %v", err)
	}
	for _, dir := range []string{okfDir, obsidianDir} {
		page := readExported(t, dir, "alpha.md")
		if !strings.Contains(page, "generated:") || !strings.Contains(page, "# Alpha") {
			t.Fatalf("export at %s is neither OKF nor a navigable vault:\n%s", dir, page)
		}
	}
}
