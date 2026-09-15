package artifactpackage

import (
	"archive/zip"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestWriteAndValidate(t *testing.T) {
	source := filepath.Join(t.TempDir(), "graph.icebug")
	if err := os.MkdirAll(source, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(source, "schema.cypher"), []byte("RETURN 1"), 0o644); err != nil {
		t.Fatal(err)
	}
	out := filepath.Join(t.TempDir(), "code.ast")
	if err := Write(out, "ast", []Member{{SourcePath: source, PackagePath: "graph.icebug", Required: true}}); err != nil {
		t.Fatalf("Write: %v", err)
	}

	extracted := filepath.Join(t.TempDir(), "extracted")
	if err := unzipForTest(out, extracted); err != nil {
		t.Fatal(err)
	}
	manifest, err := Validate(extracted, "ast")
	if err != nil {
		t.Fatalf("Validate: %v", err)
	}
	if manifest.Version != Version || manifest.ArtifactType != "ast" {
		t.Fatalf("manifest = %#v", manifest)
	}
	if _, err := os.Stat(filepath.Join(extracted, "graph.icebug", "schema.cypher")); err != nil {
		t.Fatalf("packaged source missing: %v", err)
	}
}

func TestValidateRejectsWrongTypeAndVersion(t *testing.T) {
	root := t.TempDir()
	if err := os.Mkdir(filepath.Join(root, "index.lance"), 0o755); err != nil {
		t.Fatal(err)
	}
	manifest := Manifest{Format: Format, Version: Version + 1, ArtifactType: "knowledge", Contents: []string{"index.lance"}}
	raw, _ := json.Marshal(manifest)
	if err := os.WriteFile(filepath.Join(root, ManifestName), raw, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Validate(root, "knowledge"); err == nil || !strings.Contains(err.Error(), "version") {
		t.Fatalf("version error = %v", err)
	}
	manifest.Version = Version
	raw, _ = json.Marshal(manifest)
	if err := os.WriteFile(filepath.Join(root, ManifestName), raw, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Validate(root, "ast"); err == nil || !strings.Contains(err.Error(), "does not match") {
		t.Fatalf("type error = %v", err)
	}
}

func unzipForTest(archive, dest string) error {
	r, err := zip.OpenReader(archive)
	if err != nil {
		return err
	}
	defer func() { _ = r.Close() }()
	for _, file := range r.File {
		target := filepath.Join(dest, filepath.FromSlash(file.Name))
		if file.FileInfo().IsDir() {
			if err := os.MkdirAll(target, 0o755); err != nil {
				return err
			}
			continue
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		rc, err := file.Open()
		if err != nil {
			return err
		}
		raw, err := io.ReadAll(rc)
		_ = rc.Close()
		if err != nil {
			return err
		}
		if err := os.WriteFile(target, raw, 0o644); err != nil {
			return err
		}
	}
	return nil
}
