package knowledge

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// ExportResult reports what a textual Knowledge export produced.
type ExportResult = wiki.RenderResult

// ExportPackage writes a validated native Knowledge index to a portable .knowledge archive.
func ExportPackage(ctx context.Context, wikiDir, outputPath string) error {
	staging, err := os.MkdirTemp("", "graphit-knowledge-package-")
	if err != nil {
		return fmt.Errorf("create knowledge package staging: %w", err)
	}
	defer func() { _ = os.RemoveAll(staging) }()
	if _, err := wiki.StagePublishedIndex(ctx, wikiDir, staging); err != nil {
		return err
	}
	return artifactpackage.Write(outputPath, "knowledge", []artifactpackage.Member{{
		SourcePath:  filepath.Join(staging, wiki.WikiIndexDirName),
		PackagePath: wiki.WikiIndexDirName,
		Required:    true,
	}})
}

// ExportOKF emits the Open Knowledge Format Markdown representation.
func ExportOKF(ctx context.Context, wikiDir, outDir, moduleTag string) (*ExportResult, error) {
	return wiki.RenderMarkdown(ctx, wikiDir, outDir, moduleTag)
}

// ExportObsidian emits a navigable Obsidian vault. OKF frontmatter is retained as valid YAML.
func ExportObsidian(ctx context.Context, wikiDir, outDir, moduleTag string) (*ExportResult, error) {
	return wiki.RenderMarkdown(ctx, wikiDir, outDir, moduleTag)
}
