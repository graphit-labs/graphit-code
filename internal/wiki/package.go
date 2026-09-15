package wiki

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
)

// ExportPackage writes a validated native Knowledge index to a portable .knowledge archive.
func ExportPackage(ctx context.Context, wikiDir, outputPath string) error {
	staging, err := os.MkdirTemp("", "graphit-knowledge-package-")
	if err != nil {
		return fmt.Errorf("create knowledge package staging: %w", err)
	}
	defer func() { _ = os.RemoveAll(staging) }()
	if _, err := StagePublishedIndex(ctx, wikiDir, staging); err != nil {
		return err
	}
	return artifactpackage.Write(outputPath, "knowledge", []artifactpackage.Member{{
		SourcePath:  filepath.Join(staging, WikiIndexDirName),
		PackagePath: WikiIndexDirName,
		Required:    true,
	}})
}

// ExportOKF emits the Open Knowledge Format Markdown representation.
func ExportOKF(ctx context.Context, wikiDir, outDir, moduleTag string) (*ExportResult, error) {
	return ExportMarkdown(ctx, wikiDir, outDir, moduleTag)
}

// ExportObsidian emits a navigable Obsidian vault. OKF frontmatter is retained as valid YAML.
func ExportObsidian(ctx context.Context, wikiDir, outDir, moduleTag string) (*ExportResult, error) {
	return ExportMarkdown(ctx, wikiDir, outDir, moduleTag)
}
