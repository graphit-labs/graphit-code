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

type relationshipTypeNamer interface {
	logicalRelationshipType(physical string) string
}

type relationshipStatsProvider interface {
	logicalRelationshipStats() ([]relationshipTypeStat, bool)
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

func (k *LadybugBackend) logicalRelationshipStats() ([]relationshipTypeStat, bool) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.canonical == nil {
		return nil, false
	}

	stats := make([]relationshipTypeStat, 0, len(k.canonical.RelGroups))
	for _, group := range k.canonical.RelGroups {
		var count int64
		for _, member := range group.Members {
			count += member.Rows
		}
		stats = append(stats, relationshipTypeStat{Type: group.Type, Count: count})
	}
	sort.Slice(stats, func(i, j int) bool {
		if stats[i].Count == stats[j].Count {
			return stats[i].Type < stats[j].Type
		}
		return stats[i].Count > stats[j].Count
	})
	return stats, true
}
