package hub

import (
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

// EnsureGlobalLanguageArtifacts ensures all language artifacts tracked in the
// project lockfile are materialized in the global directories:
//   - YAMLs → ~/.graphit/ast/queries/
//   - Grammars → ~/.graphit/grammars/{treesitter,antlr}/
//
// This is called during sync to ensure the global language files are up to date.
// Project-level files (if any) still take precedence via the resolution chain.
func EnsureGlobalLanguageArtifacts(lockfilePath string) error {
	globalDir := brand.GlobalDir()
	if globalDir == "" {
		return nil
	}

	lf, err := LoadLockfile(lockfilePath)
	if err != nil || lf == nil {
		return nil
	}

	langArts := lf.Artifacts[TypeLanguage]
	if len(langArts) == 0 {
		return nil
	}

	projectDir := filepath.Dir(lockfilePath)
	pp := paths.GetPathsForProject("", projectDir)

	queriesDir := filepath.Join(globalDir, "ast", "queries")

	for artID, meta := range langArts {
		if !meta.IsHubInstalled() {
			continue
		}

		cloneDir := resolveArtifactPath(meta, TypeLanguage, artID, pp)
		if cloneDir == "" {
			continue
		}

		cloneEntries, err := os.ReadDir(cloneDir)
		if err != nil {
			continue
		}

		for _, ce := range cloneEntries {
			if ce.IsDir() {
				continue
			}
			name := ce.Name()
			src := filepath.Join(cloneDir, name)

			// YAML query definitions → global queries dir.
			if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") {
				if err := os.MkdirAll(queriesDir, 0o755); err != nil {
					slog.Warn("ensure global lang: create queries dir", "error", err)
					continue
				}
				if err := copyFile(src, filepath.Join(queriesDir, name)); err != nil {
					slog.Warn("ensure global lang: copy yaml", "file", name, "error", err)
				}
				continue
			}

			// Grammar archives → global grammars dir.
			if strings.HasSuffix(name, ".grammar") {
				if err := installGrammarArchive(src, globalDir, ""); err != nil {
					slog.Warn("ensure global lang: install grammar", "file", name, "error", err)
				}
			}
		}
	}

	return nil
}
