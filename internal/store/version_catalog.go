package store

import (
	"encoding/base64"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/projectversion"
)

// VersionFromPathSegment reverses VersionPathSegment: the version a store directory
// under `versions/` belongs to.
func VersionFromPathSegment(segment string) string {
	switch {
	case segment == "unversioned":
		return ""
	case strings.HasPrefix(segment, "~"):
		if decoded, err := base64.RawURLEncoding.DecodeString(segment[1:]); err == nil {
			return string(decoded)
		}
		return segment
	case strings.HasSuffix(segment, "_"):
		if trimmed := strings.TrimSuffix(segment, "_"); DefuseReservedName(trimmed) == segment {
			return trimmed
		}
	}
	return segment
}

// LocalVersions lists the project versions whose local AST graph or wiki exists for
// projectDir's project, with the modules ("ast", "knowledge") that hold each. A store
// written before versions existed belongs to the default version. Nothing is created.
func LocalVersions(projectDir string) map[string][]string {
	out := map[string][]string{}
	add := func(version, module string) {
		for _, existing := range out[version] {
			if existing == module {
				return
			}
		}
		out[version] = append(out[version], module)
	}
	for module, root := range map[string]string{"ast": ASTProjectRoot(projectDir), "knowledge": KnowledgeProjectRoot(projectDir)} {
		if root == "" {
			continue
		}
		if hasLegacyStore(root) {
			add(projectversion.Current(projectDir).Default, module)
		}
		entries, err := os.ReadDir(filepath.Join(root, versionsNamespace))
		if err != nil {
			continue
		}
		for _, entry := range entries {
			if !entry.IsDir() || strings.HasPrefix(entry.Name(), ".") {
				continue
			}
			if version := VersionFromPathSegment(entry.Name()); version != "" {
				add(version, module)
			}
		}
	}
	for version := range out {
		sort.Strings(out[version])
	}
	return out
}
