package artifactpackage

import (
	"archive/zip"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strings"
)

const (
	Format       = "graphit-artifact-package"
	Version      = 1
	ManifestName = "graphit-package.json"
)

type Manifest struct {
	Format       string   `json:"format"`
	Version      int      `json:"version"`
	ArtifactType string   `json:"artifact_type"`
	Contents     []string `json:"contents"`
}

type Member struct {
	SourcePath  string
	PackagePath string
	Required    bool
}

// EnsureExtension adds the package extension, or chooses export<extension> inside an existing directory.
func EnsureExtension(outputPath, extension string) string {
	if strings.EqualFold(filepath.Ext(outputPath), extension) {
		return outputPath
	}
	if info, err := os.Stat(outputPath); err == nil && info.IsDir() {
		return filepath.Join(outputPath, "export"+extension)
	}
	return outputPath + extension
}

// Write creates an atomic, versioned ZIP package from native artifact directories.
func Write(outputPath, artifactType string, members []Member) error {
	if strings.TrimSpace(outputPath) == "" {
		return fmt.Errorf("package output path is required")
	}
	if strings.TrimSpace(artifactType) == "" {
		return fmt.Errorf("package artifact type is required")
	}
	if err := os.MkdirAll(filepath.Dir(outputPath), 0o755); err != nil {
		return fmt.Errorf("create package output directory: %w", err)
	}

	tmp, err := os.CreateTemp(filepath.Dir(outputPath), ".graphit-package-*")
	if err != nil {
		return fmt.Errorf("create package: %w", err)
	}
	tmpPath := tmp.Name()
	ok := false
	defer func() {
		_ = tmp.Close()
		if !ok {
			_ = os.Remove(tmpPath)
		}
	}()

	zw := zip.NewWriter(tmp)
	manifest := Manifest{Format: Format, Version: Version, ArtifactType: artifactType}
	for _, member := range members {
		cleanName, err := cleanPackagePath(member.PackagePath)
		if err != nil {
			_ = zw.Close()
			return err
		}
		info, statErr := os.Stat(member.SourcePath)
		if statErr != nil {
			if os.IsNotExist(statErr) && !member.Required {
				continue
			}
			_ = zw.Close()
			return fmt.Errorf("package member %s: %w", cleanName, statErr)
		}
		if !info.IsDir() {
			_ = zw.Close()
			return fmt.Errorf("package member %s must be a directory", cleanName)
		}
		if err := writeDirectory(zw, member.SourcePath, cleanName); err != nil {
			_ = zw.Close()
			return err
		}
		manifest.Contents = append(manifest.Contents, cleanName)
	}

	if len(manifest.Contents) == 0 {
		_ = zw.Close()
		return fmt.Errorf("package has no contents")
	}
	manifestWriter, err := zw.Create(ManifestName)
	if err != nil {
		_ = zw.Close()
		return fmt.Errorf("create package manifest: %w", err)
	}
	enc := json.NewEncoder(manifestWriter)
	enc.SetIndent("", "  ")
	if err := enc.Encode(manifest); err != nil {
		_ = zw.Close()
		return fmt.Errorf("write package manifest: %w", err)
	}
	if err := zw.Close(); err != nil {
		return fmt.Errorf("finish package: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("close package: %w", err)
	}
	if err := os.Rename(tmpPath, outputPath); err != nil {
		return fmt.Errorf("publish package: %w", err)
	}
	ok = true
	return nil
}

// Validate checks the extracted package envelope before a type-specific importer reads it.
func Validate(root, expectedType string) (*Manifest, error) {
	raw, err := os.ReadFile(filepath.Join(root, ManifestName))
	if err != nil {
		return nil, fmt.Errorf("read package manifest: %w", err)
	}
	var manifest Manifest
	if err := json.Unmarshal(raw, &manifest); err != nil {
		return nil, fmt.Errorf("parse package manifest: %w", err)
	}
	if manifest.Format != Format {
		return nil, fmt.Errorf("unsupported package format %q", manifest.Format)
	}
	if manifest.Version != Version {
		return nil, fmt.Errorf("unsupported package version %d (supported: %d)", manifest.Version, Version)
	}
	if manifest.ArtifactType != expectedType {
		return nil, fmt.Errorf("package type %q does not match selected type %q", manifest.ArtifactType, expectedType)
	}
	if len(manifest.Contents) == 0 {
		return nil, fmt.Errorf("package manifest has no contents")
	}
	for _, name := range manifest.Contents {
		clean, err := cleanPackagePath(name)
		if err != nil {
			return nil, err
		}
		info, err := os.Stat(filepath.Join(root, filepath.FromSlash(clean)))
		if err != nil || !info.IsDir() {
			return nil, fmt.Errorf("package content %q is missing", clean)
		}
	}
	return &manifest, nil
}

func writeDirectory(zw *zip.Writer, sourceRoot, packageRoot string) error {
	return filepath.WalkDir(sourceRoot, func(filePath string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		rel, err := filepath.Rel(sourceRoot, filePath)
		if err != nil {
			return err
		}
		name := packageRoot
		if rel != "." {
			name = path.Join(packageRoot, filepath.ToSlash(rel))
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("package member %s is a symlink", name)
		}
		if entry.IsDir() {
			header, err := zip.FileInfoHeader(info)
			if err != nil {
				return err
			}
			header.Name = name + "/"
			_, err = zw.CreateHeader(header)
			return err
		}
		if !info.Mode().IsRegular() {
			return fmt.Errorf("package member %s is not a regular file", name)
		}
		header, err := zip.FileInfoHeader(info)
		if err != nil {
			return err
		}
		header.Name = name
		header.Method = zip.Deflate
		writer, err := zw.CreateHeader(header)
		if err != nil {
			return err
		}
		reader, err := os.Open(filePath)
		if err != nil {
			return err
		}
		_, copyErr := io.Copy(writer, reader)
		closeErr := reader.Close()
		if copyErr != nil {
			return copyErr
		}
		return closeErr
	})
}

func cleanPackagePath(name string) (string, error) {
	clean := path.Clean(strings.TrimSpace(filepath.ToSlash(name)))
	if clean == "." || clean == ".." || strings.HasPrefix(clean, "../") || path.IsAbs(clean) {
		return "", fmt.Errorf("invalid package path %q", name)
	}
	return clean, nil
}
