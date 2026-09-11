package ast

import (
	"sort"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// relationshipTypeStat is the public, logical view of a relationship group.
// Physical member tables are an icebug storage constraint and must not cross the
// API boundary into the explorer.
type relationshipTypeStat struct {
	Type  string
	Count int64
}

type nodeTypeStat struct {
	Label string
	Count int64
}

type canonicalStats struct {
	NodeCount         int64
	EdgeCount         int64
	Nodes             []nodeTypeStat
	Relationships     []relationshipTypeStat
	Langs             []SchemaLangGroup
	LangStatsComplete bool
}

type relationshipTypeNamer interface {
	logicalRelationshipType(physical string) string
}

type canonicalStatsProvider interface {
	canonicalGraphStats() (canonicalStats, bool)
}

func canonicalLogicalRelationshipType(man *ladybug.CanonicalManifest, physical string) string {
	if man == nil || physical == "" {
		return physical
	}
	for _, group := range man.RelGroups {
		if strings.EqualFold(group.Type, physical) {
			return group.Type
		}
		for _, member := range group.Members {
			if strings.EqualFold(member.Table, physical) {
				return group.Type
			}
		}
		for _, member := range group.ReverseMembers {
			if strings.EqualFold(member.Table, physical) {
				return group.Type
			}
		}
	}
	return physical
}

func (k *LadybugBackend) logicalRelationshipType(physical string) string {
	k.mu.Lock()
	defer k.mu.Unlock()
	return canonicalLogicalRelationshipType(k.canonical, physical)
}

// IsCanonicalGraph reports whether the backend has a canonical manifest whose
// metadata can answer schema and naming questions without scanning the graph.
func (k *LadybugBackend) IsCanonicalGraph() bool {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.canonical == nil {
		_ = k.loadCanonicalManifestLocked()
	}
	return k.canonical != nil
}

type canonicalGraphMarker interface {
	IsCanonicalGraph() bool
}

// IsCanonicalGraph lets consumers choose metadata-backed operations instead of
// generic graph scans. Backends without this optional capability return false.
func IsCanonicalGraph(db GraphDB) bool {
	marker, ok := db.(canonicalGraphMarker)
	return ok && marker.IsCanonicalGraph()
}

type canonicalManifestProvider interface {
	canonicalManifestSnapshot() (*ladybug.CanonicalManifest, bool)
}

func (k *LadybugBackend) canonicalManifestSnapshot() (*ladybug.CanonicalManifest, bool) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.canonical == nil {
		if err := k.loadCanonicalManifestLocked(); err != nil || k.canonical == nil {
			return nil, false
		}
	}
	return k.canonical, true
}

type publicCypherNormalizer interface {
	publicCypher(string) string
}

// PublicCypher replaces canonical storage-table relationship names with the
// logical relationship aliases exposed by the manifest.
func PublicCypher(db GraphDB, cypher string) string {
	if normalizer, ok := db.(publicCypherNormalizer); ok {
		return normalizer.publicCypher(cypher)
	}
	return cypher
}

func (k *LadybugBackend) publicCypher(cypher string) string {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.canonical == nil {
		_ = k.loadCanonicalManifestLocked()
	}
	return canonicalPublicCypher(k.canonical, cypher)
}

func canonicalPublicCypher(man *ladybug.CanonicalManifest, cypher string) string {
	if man == nil || cypher == "" {
		return cypher
	}
	aliases := make(map[string]string)
	for _, group := range man.RelGroups {
		aliases[strings.ToLower(group.Type)] = group.Type
		for _, member := range group.Members {
			aliases[strings.ToLower(member.Table)] = group.Type
		}
		for _, member := range group.ReverseMembers {
			aliases[strings.ToLower(member.Table)] = group.Type
		}
	}

	var out strings.Builder
	out.Grow(len(cypher))
	inString := byte(0)
	inRelationship := false
	for i := 0; i < len(cypher); {
		ch := cypher[i]
		if inString != 0 {
			out.WriteByte(ch)
			if ch == inString && (i == 0 || cypher[i-1] != '\\') {
				inString = 0
			}
			i++
			continue
		}
		if ch == '\'' || ch == '"' {
			inString = ch
			out.WriteByte(ch)
			i++
			continue
		}
		switch ch {
		case '[':
			inRelationship = true
		case ']':
			inRelationship = false
		}
		out.WriteByte(ch)
		i++
		if !inRelationship || ch != ':' {
			continue
		}
		for i < len(cypher) && (cypher[i] == ' ' || cypher[i] == '\t') {
			out.WriteByte(cypher[i])
			i++
		}
		quoted := i < len(cypher) && cypher[i] == '`'
		if quoted {
			i++
		}
		start := i
		for i < len(cypher) && ((cypher[i] >= 'A' && cypher[i] <= 'Z') || (cypher[i] >= 'a' && cypher[i] <= 'z') || (cypher[i] >= '0' && cypher[i] <= '9') || cypher[i] == '_') {
			i++
		}
		if start == i {
			if quoted {
				out.WriteByte('`')
			}
			continue
		}
		name := cypher[start:i]
		if logical, ok := aliases[strings.ToLower(name)]; ok {
			name = logical
		}
		if quoted {
			out.WriteByte('`')
		}
		out.WriteString(name)
		if quoted && i < len(cypher) && cypher[i] == '`' {
			out.WriteByte('`')
			i++
		}
	}
	return out.String()
}

func (k *LadybugBackend) logicalRelationshipStats() ([]relationshipTypeStat, bool) {
	stats, ok := k.canonicalGraphStats()
	return stats.Relationships, ok
}

func (k *LadybugBackend) canonicalGraphStats() (canonicalStats, bool) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.canonical == nil {
		if err := k.loadCanonicalManifestLocked(); err != nil || k.canonical == nil {
			return canonicalStats{}, false
		}
	}
	return canonicalStatsFromManifest(k.canonical), true
}

func canonicalStatsFromManifest(man *ladybug.CanonicalManifest) canonicalStats {
	stats := canonicalStats{LangStatsComplete: true}
	langLabels := map[string][]SchemaLabelCount{}
	langTotals := map[string]int{}

	for _, table := range man.NodeTables {
		stats.NodeCount += table.Rows
		stats.Nodes = append(stats.Nodes, nodeTypeStat{Label: table.Label, Count: table.Rows})

		var accounted int64
		for _, lc := range table.LangCounts {
			accounted += lc.Rows
			count := int(lc.Rows)
			langTotals[lc.Lang] += count
			langLabels[lc.Lang] = append(langLabels[lc.Lang], SchemaLabelCount{Label: table.Label, Count: count})
		}
		if accounted != table.Rows {
			stats.LangStatsComplete = false
		}
	}
	sort.Slice(stats.Nodes, func(i, j int) bool {
		if stats.Nodes[i].Count == stats.Nodes[j].Count {
			return stats.Nodes[i].Label < stats.Nodes[j].Label
		}
		return stats.Nodes[i].Count > stats.Nodes[j].Count
	})

	for _, group := range man.RelGroups {
		var count int64
		for _, member := range group.Members {
			count += member.Rows
		}
		stats.Relationships = append(stats.Relationships, relationshipTypeStat{Type: group.Type, Count: count})
	}
	stats.EdgeCount = man.EdgeCount
	sort.Slice(stats.Relationships, func(i, j int) bool {
		if stats.Relationships[i].Count == stats.Relationships[j].Count {
			return stats.Relationships[i].Type < stats.Relationships[j].Type
		}
		return stats.Relationships[i].Count > stats.Relationships[j].Count
	})

	if stats.LangStatsComplete {
		langs := make([]string, 0, len(langTotals))
		for lang := range langTotals {
			langs = append(langs, lang)
			sort.Slice(langLabels[lang], func(i, j int) bool {
				if langLabels[lang][i].Count == langLabels[lang][j].Count {
					return langLabels[lang][i].Label < langLabels[lang][j].Label
				}
				return langLabels[lang][i].Count > langLabels[lang][j].Count
			})
		}
		sort.Slice(langs, func(i, j int) bool {
			if (langs[i] == "") != (langs[j] == "") {
				return langs[j] == ""
			}
			if langTotals[langs[i]] == langTotals[langs[j]] {
				return langs[i] < langs[j]
			}
			return langTotals[langs[i]] > langTotals[langs[j]]
		})
		for _, lang := range langs {
			stats.Langs = append(stats.Langs, SchemaLangGroup{
				Lang: lang, Count: langTotals[lang], Labels: langLabels[lang],
			})
		}
	}
	return stats
}
