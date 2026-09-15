package ast

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/artifactpackage"
)

// ExportPackage writes the native queryable AST stores to a portable .ast archive.
func ExportPackage(storeDir, outputPath string) error {
	graphDir := filepath.Join(storeDir, IcebugBundleDir)
	if !HasIcebugBundle(storeDir) {
		return fmt.Errorf("AST package: no finished graph at %s", graphDir)
	}
	if info, err := os.Stat(filepath.Join(graphDir, IcebugSchemaFile)); err != nil || info.IsDir() {
		return fmt.Errorf("AST package: no schema at %s", filepath.Join(graphDir, IcebugSchemaFile))
	}
	return artifactpackage.Write(outputPath, "ast", []artifactpackage.Member{
		{SourcePath: graphDir, PackagePath: IcebugBundleDir, Required: true},
		{SourcePath: filepath.Join(storeDir, SearchBundleDir), PackagePath: SearchBundleDir},
	})
}
