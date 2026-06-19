package knowledge

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// clusterCacheFile is stored alongside the wiki pages and holds the last
// known community-detection result.  When cross-references have not changed,
// the previous assignments can be reused without re-building the graph.
const clusterCacheFile = ".cluster_cache.json"

type clusterCache struct {
	SlugToCluster     map[string]int    `json:"slug_to_cluster"`
	SlugToClusterName map[string]string `json:"slug_to_cluster_name"`
}

func saveClusterCache(wikiDir string, slugToCluster map[string]int, slugToClusterName map[string]string) {
	cc := clusterCache{
		SlugToCluster:     slugToCluster,
		SlugToClusterName: slugToClusterName,
	}
	data, err := json.Marshal(cc)
	if err != nil {
		return
	}
	_ = os.WriteFile(filepath.Join(wikiDir, clusterCacheFile), data, 0o644)
}

// loadClusterCache returns (nil, nil, false) when no cache exists or the file is unreadable.
func loadClusterCache(wikiDir string) (slugToCluster map[string]int, slugToClusterName map[string]string, ok bool) {
	data, err := os.ReadFile(filepath.Join(wikiDir, clusterCacheFile))
	if err != nil {
		return nil, nil, false
	}
	var cc clusterCache
	if err := json.Unmarshal(data, &cc); err != nil {
		return nil, nil, false
	}
	return cc.SlugToCluster, cc.SlugToClusterName, true
}
