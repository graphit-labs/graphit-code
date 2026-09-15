//go:build lancedb

package ast

import (
	"archive/zip"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

func TestExportPackageCarriesNativeStores(t *testing.T) {
	storeDir := t.TempDir()
	graphDir := filepath.Join(storeDir, IcebugBundleDir)
	searchDir := filepath.Join(storeDir, SearchBundleDir)
	if err := os.MkdirAll(graphDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(searchDir, 0o755); err != nil {
		t.Fatal(err)
	}
	manifest, _ := json.Marshal(ladybugstore.IcebugManifest{Schema: IcebugSchemaFile})
	if err := os.WriteFile(filepath.Join(graphDir, ladybugstore.IcebugManifestFile), manifest, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(graphDir, IcebugSchemaFile), []byte("RETURN 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(searchDir, "index.idx"), []byte("search"), 0o644); err != nil {
		t.Fatal(err)
	}

	out := filepath.Join(t.TempDir(), "project.ast")
	if err := ExportPackage(storeDir, out); err != nil {
		t.Fatalf("ExportPackage: %v", err)
	}
	members := zipMembers(t, out)
	for _, want := range []string{
		artifactpackage.ManifestName,
		IcebugBundleDir + "/" + ladybugstore.IcebugManifestFile,
		IcebugBundleDir + "/" + IcebugSchemaFile,
		SearchBundleDir + "/index.idx",
	} {
		if _, ok := members[want]; !ok {
			t.Errorf("package member %q missing", want)
		}
	}
}

func TestExportPackageRequiresFinishedGraph(t *testing.T) {
	err := ExportPackage(t.TempDir(), filepath.Join(t.TempDir(), "project.ast"))
	if err == nil {
		t.Fatal("expected an unfinished graph to be rejected")
	}
}

func zipMembers(t *testing.T, packagePath string) map[string][]byte {
	t.Helper()
	zr, err := zip.OpenReader(packagePath)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = zr.Close() }()
	out := make(map[string][]byte)
	for _, member := range zr.File {
		if member.FileInfo().IsDir() {
			continue
		}
		rc, err := member.Open()
		if err != nil {
			t.Fatal(err)
		}
		raw, err := io.ReadAll(rc)
		_ = rc.Close()
		if err != nil {
			t.Fatal(err)
		}
		out[member.Name] = raw
	}
	return out
}
