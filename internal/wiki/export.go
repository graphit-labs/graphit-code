package wiki

import (
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"time"
)

type ExportResult struct {
	WikiDir   string
	ExportDir string
	WikiFiles int
	Duration  time.Duration
}

type ImportResult struct {
	WikiDir   string
	ExportDir string
	WikiFiles int
	Duration  time.Duration
}

type ExportManifest struct {
	Version    string `json:"version"`
	ModuleTag  string `json:"module_tag"`
	ExportedAt string `json:"exported_at"`
}

type ExportConfig struct {
	ModuleTag string
}

type ImportConfig struct{}

const exportManifestVersion = "1"

func Export(wikiDir, exportDir string, cfg ExportConfig) (*ExportResult, error) {
	start := time.Now()
	result := &ExportResult{WikiDir: wikiDir, ExportDir: exportDir}

	wikiExportDir := filepath.Join(exportDir, "wiki")
	if err := os.MkdirAll(wikiExportDir, 0o755); err != nil {
		return nil, fmt.Errorf("export: create wiki dir: %w", err)
	}

	if _, err := os.Stat(wikiDir); err == nil {
		n, err := copyWikiTree(wikiDir, wikiExportDir)
		if err != nil {
			return nil, fmt.Errorf("export: copy wiki: %w", err)
		}
		result.WikiFiles = n
	}

	manifest := ExportManifest{
		Version:    exportManifestVersion,
		ModuleTag:  cfg.ModuleTag,
		ExportedAt: time.Now().UTC().Format(time.RFC3339),
	}
	if data, err := json.MarshalIndent(manifest, "", "  "); err == nil {
		_ = os.WriteFile(filepath.Join(exportDir, "manifest.json"), data, 0o644)
	}

	result.Duration = time.Since(start)
	return result, nil
}

func Import(wikiDir, exportDir string, _ ImportConfig) (*ImportResult, error) {
	start := time.Now()
	result := &ImportResult{WikiDir: wikiDir, ExportDir: exportDir}

	wikiExportDir := filepath.Join(exportDir, "wiki")
	if _, err := os.Stat(wikiExportDir); err == nil {
		if err := os.MkdirAll(wikiDir, 0o755); err != nil {
			return nil, fmt.Errorf("import: create wiki dir: %w", err)
		}
		n, err := copyWikiTree(wikiExportDir, wikiDir)
		if err != nil {
			return nil, fmt.Errorf("import: restore wiki: %w", err)
		}
		result.WikiFiles = n
	}

	result.Duration = time.Since(start)
	return result, nil
}

func ReadManifest(exportDir string) (ExportManifest, error) {
	var m ExportManifest
	data, err := os.ReadFile(filepath.Join(exportDir, "manifest.json"))
	if err != nil {
		if os.IsNotExist(err) {
			return m, nil
		}
		return m, err
	}
	return m, json.Unmarshal(data, &m)
}

func CopyWiki(src, dst string) (int, error) {
	return copyWikiTree(src, dst)
}

func copyWikiTree(src, dst string) (int, error) {
	count := 0
	err := filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		rel, relErr := filepath.Rel(src, path)
		if relErr != nil {
			return nil
		}
		destPath := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(destPath, 0o755)
		}
		if cpErr := copyRawFile(path, destPath); cpErr != nil {
			return cpErr
		}
		count++
		return nil
	})
	return count, err
}

func copyRawFile(src, dst string) error {
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, in)
	return err
}
