package knowledge

import (
	"sort"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// KnowledgeCommunity represents a cluster of related knowledge pages.
type KnowledgeCommunity struct {
	ID       int
	Label    string   // hub node title (most connected page in cluster)
	Members  []string // page slugs
	Cohesion float64
}

// DetectKnowledgeCommunities runs Louvain community detection on the cross-ref graph.
// Returns clusters sorted by size (largest first). Singleton clusters are excluded.
func DetectKnowledgeCommunities(graph *wiki.CrossRefGraph) []KnowledgeCommunity {
	if graph == nil || len(graph.AllPages) < 2 {
		return nil
	}

	// Build undirected adjacency from Outbound refs
	adj := make(map[string][]string)
	for slug := range graph.AllPages {
		adj[slug] = nil // ensure every page appears
	}
	for src, targets := range graph.Outbound {
		for _, dst := range targets {
			if graph.AllPages[dst] { // only include valid targets
				adj[src] = append(adj[src], dst)
				adj[dst] = append(adj[dst], src)
			}
		}
	}

	// Run Louvain
	assignment := wiki.Louvain(adj)

	// Group by community
	byComm := make(map[int][]string)
	for slug, cid := range assignment {
		byComm[cid] = append(byComm[cid], slug)
	}

	// Sort communities by size (largest first)
	type cidMembers struct {
		cid     int
		members []string
	}
	var sorted []cidMembers
	for cid, members := range byComm {
		if len(members) < 2 {
			continue // exclude singletons
		}
		sort.Strings(members)
		sorted = append(sorted, cidMembers{cid, members})
	}
	sort.Slice(sorted, func(i, j int) bool {
		return len(sorted[i].members) > len(sorted[j].members)
	})

	var result []KnowledgeCommunity
	for newID, cm := range sorted {
		// Name cluster by the hub node (most connected page)
		label := cm.members[0]
		bestDeg := 0
		for _, slug := range cm.members {
			deg := len(adj[slug])
			if deg > bestDeg {
				bestDeg = deg
				if title, ok := graph.Titles[slug]; ok && title != "" {
					label = title
				} else {
					label = slug
				}
			}
		}

		cohesion := wiki.ComputeCohesion(adj, cm.members)
		result = append(result, KnowledgeCommunity{
			ID:       newID,
			Label:    label,
			Members:  cm.members,
			Cohesion: cohesion,
		})
	}

	return result
}

// AssignCommunities maps each slug to its community info.
func AssignCommunities(communities []KnowledgeCommunity) (slugToCluster map[string]int, slugToClusterName map[string]string) {
	slugToCluster = make(map[string]int)
	slugToClusterName = make(map[string]string)
	for _, comm := range communities {
		for _, slug := range comm.Members {
			slugToCluster[slug] = comm.ID
			slugToClusterName[slug] = comm.Label
		}
	}
	return
}
