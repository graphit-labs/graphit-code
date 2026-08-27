package ast

import (
	"archive/zip"
	"context"
	"encoding/json"
	"io"
	"path/filepath"
	"strings"
	"testing"
)

// bundleMembers reads a bundle back: the manifest, and the text of every sources/
// member keyed by its path inside the archive.
func bundleMembers(t *testing.T, bundlePath string) (BundleManifest, map[string]string) {
	t.Helper()

	zr, err := zip.OpenReader(bundlePath)
	if err != nil {
		t.Fatalf("open bundle: %v", err)
	}
	defer func() { _ = zr.Close() }()

	var manifest BundleManifest
	sources := map[string]string{}

	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			t.Fatalf("open member %s: %v", f.Name, err)
		}
		body, err := io.ReadAll(rc)
		_ = rc.Close()
		if err != nil {
			t.Fatalf("read member %s: %v", f.Name, err)
		}

		switch {
		case f.Name == "manifest.json":
			if err := json.Unmarshal(body, &manifest); err != nil {
				t.Fatalf("decode manifest: %v", err)
			}
		case strings.HasPrefix(f.Name, bundleSourceDir):
			sources[strings.TrimPrefix(f.Name, bundleSourceDir)] = string(body)
		}
	}
	return manifest, sources
}

// TestBundleCarriesSourcesFromTheSearchIndex is why --no-sources can mean something
// again.
//
// The flag existed on the CLI and on the MCP tool and was accepted and dropped:
// runASTExport took the parameter and never read it, and the success message claimed
// "(with sources)" either way. It was also unimplementable as written, because it
// described omitting a node property that file text no longer lives in. The text comes
// from the search index now, so including it is a real action and omitting it is a real
// choice.
func TestBundleCarriesSourcesFromTheSearchIndex(t *testing.T) {
	const rel = "svc/handler.go"
	const body = "package svc\n\nfunc HandlePayment() {}\n"
	dbPath := writeSearchIndexSource(t, rel, body)

	out := filepath.Join(t.TempDir(), "graph.ast")
	err := ExportBundle(context.Background(), &emptyGraphDB{}, t.TempDir(), out,
		BundleOptions{StorePath: dbPath}, nil)
	if err != nil {
		t.Fatalf("ExportBundle: %v", err)
	}

	manifest, sources := bundleMembers(t, out)
	if manifest.SourceCount != 1 {
		t.Errorf("manifest source_count = %d, want 1", manifest.SourceCount)
	}
	if got := sources[rel]; got != body {
		t.Errorf("bundled source for %s:\n got %q\nwant %q", rel, got, body)
	}
}

// The point of the flag: same export, no text.
func TestBundleNoSourcesOmitsText(t *testing.T) {
	const rel = "svc/handler.go"
	dbPath := writeSearchIndexSource(t, rel, "package svc\n")

	out := filepath.Join(t.TempDir(), "graph.ast")
	err := ExportBundle(context.Background(), &emptyGraphDB{}, t.TempDir(), out,
		BundleOptions{StorePath: dbPath, NoSources: true}, nil)
	if err != nil {
		t.Fatalf("ExportBundle: %v", err)
	}

	manifest, sources := bundleMembers(t, out)
	if manifest.SourceCount != 0 {
		t.Errorf("manifest source_count = %d, want 0 with NoSources", manifest.SourceCount)
	}
	if len(sources) != 0 {
		t.Errorf("bundle carries %d source member(s) despite NoSources", len(sources))
	}
}

// Without a store there is no index to read, so the bundle is structure-only — and the
// manifest says so rather than leaving a consumer to guess whether it was truncated.
func TestBundleWithoutStoreIsStructureOnly(t *testing.T) {
	out := filepath.Join(t.TempDir(), "graph.ast")
	err := ExportBundle(context.Background(), &emptyGraphDB{}, t.TempDir(), out,
		BundleOptions{}, nil)
	if err != nil {
		t.Fatalf("ExportBundle: %v", err)
	}

	manifest, sources := bundleMembers(t, out)
	if manifest.SourceCount != 0 || len(sources) != 0 {
		t.Errorf("expected a structure-only bundle, got source_count=%d members=%d",
			manifest.SourceCount, len(sources))
	}
}

// A missing index is not a silent structure-only export: asking for sources and not
// getting them has to be an error, or a bundle nobody can use looks like a success.
func TestBundleFailsWhenSourcesAreAskedForButUnavailable(t *testing.T) {
	out := filepath.Join(t.TempDir(), "graph.ast")
	err := ExportBundle(context.Background(), &emptyGraphDB{}, t.TempDir(), out,
		BundleOptions{StorePath: filepath.Join(t.TempDir(), "absent-store")}, nil)
	if err == nil {
		t.Fatal("expected an error when the search index is missing")
	}
	if !strings.Contains(err.Error(), "sources") {
		t.Errorf("error should name the sources step, got: %v", err)
	}
}

func TestEachFileSourceSkipsEmptyAndReportsMissingIndex(t *testing.T) {
	dbPath := writeSearchIndexSource(t, "has/text.go", "package a\n")

	idx, err := OpenSearchIndex(context.Background(), dbPath)
	if err != nil {
		t.Fatalf("open index: %v", err)
	}
	putFileRow(t, idx, "no/text.go", "")
	_ = idx.Close()

	seen := map[string]string{}
	if err := EachFileSource(context.Background(), dbPath, func(p, s string) error {
		seen[p] = s
		return nil
	}); err != nil {
		t.Fatalf("EachFileSource: %v", err)
	}
	if _, ok := seen["no/text.go"]; ok {
		t.Error("a row with empty source must not be walked")
	}
	if seen["has/text.go"] != "package a\n" {
		t.Errorf("walked source = %q", seen["has/text.go"])
	}

	if err := EachFileSource(context.Background(), filepath.Join(t.TempDir(), "absent-store"), func(string, string) error {
		return nil
	}); err == nil {
		t.Error("a missing index must be reported, not silently walked as empty")
	}
}
