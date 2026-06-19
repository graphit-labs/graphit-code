package knowledge

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

const manifestFileName = ".manifest.json"

// Manifest tracks source file hashes for staleness detection.
type Manifest struct {
	SourceHashes  map[string]string `json:"source_hashes"`   // source_path → content hash
	PageSources   map[string]string `json:"page_sources"`    // slug → source_path
	DocsModTime   int64             `json:"docs_mod_time"`   // Unix timestamp of docs dir mtime
	DocsFileCount int               `json:"docs_file_count"` // number of source files at last index
}

// StaleInfo describes why a page is considered stale.
type StaleInfo struct {
	Since  string // ISO date
	Reason string // human-readable reason
}

// LoadManifest reads the manifest from the wiki directory.
// Returns an empty manifest if the file doesn't exist.
func LoadManifest(wikiDir string) *Manifest {
	m := &Manifest{
		SourceHashes: make(map[string]string),
		PageSources:  make(map[string]string),
	}
	data, err := os.ReadFile(filepath.Join(wikiDir, manifestFileName))
	if err != nil {
		return m
	}
	_ = json.Unmarshal(data, m)
	if m.SourceHashes == nil {
		m.SourceHashes = make(map[string]string)
	}
	if m.PageSources == nil {
		m.PageSources = make(map[string]string)
	}
	return m
}

func SaveManifest(wikiDir string, m *Manifest) {
	data, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return
	}
	_ = os.WriteFile(filepath.Join(wikiDir, manifestFileName), data, 0o644)
}

// DetectStalePages compares old and new manifests and uses the cross-ref graph
// to find pages that are stale (their source changed) or transitively stale
// (a page they reference changed).
func DetectStalePages(old, current *Manifest, graph *wiki.CrossRefGraph) map[string]StaleInfo {
	stale := make(map[string]StaleInfo)
	now := time.Now().UTC().Format("2006-01-02")

	if old == nil || current == nil {
		return stale
	}

	// Find slugs whose source content changed
	changedSlugs := make(map[string]bool)
	for slug, sourcePath := range current.PageSources {
		newHash, hasNew := current.SourceHashes[sourcePath]
		oldHash, hasOld := old.SourceHashes[sourcePath]
		if hasNew && hasOld && newHash != oldHash {
			changedSlugs[slug] = true
			stale[slug] = StaleInfo{
				Since:  now,
				Reason: "source " + sourcePath + " changed",
			}
		}
	}

	// Propagate staleness to pages that reference changed pages
	if graph != nil {
		for changedSlug := range changedSlugs {
			for _, dependent := range graph.Inbound[changedSlug] {
				if _, already := stale[dependent]; !already {
					stale[dependent] = StaleInfo{
						Since:  now,
						Reason: "dependency " + changedSlug + " changed",
					}
				}
			}
		}
	}

	return stale
}
