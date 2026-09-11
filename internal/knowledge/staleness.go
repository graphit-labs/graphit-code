package knowledge

import (
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// Manifest tracks source file hashes for staleness detection.
type Manifest struct {
	SourceHashes map[string]string `json:"source_hashes"` // source_path → content hash
	PageSources  map[string]string `json:"page_sources"`  // slug → source_path
}

// StaleInfo describes why a page is considered stale.
type StaleInfo struct {
	Since  string // ISO date
	Reason string // human-readable reason
}

// ManifestFromChunks derives the previous source state from the table itself.
func ManifestFromChunks(chunks []wiki.WikiChunk) *Manifest {
	m := &Manifest{
		SourceHashes: make(map[string]string),
		PageSources:  make(map[string]string),
	}
	for _, chunk := range chunks {
		if chunk.Source == "" {
			continue
		}
		m.SourceHashes[chunk.Source] = chunk.ContentHash
		m.PageSources[chunk.Slug] = chunk.Source
	}
	return m
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
