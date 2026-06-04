package wiki

import (
	"sort"
	"testing"
)

func TestComputeCohesion(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		adj     map[string][]string
		members []string
		want    float64
	}{
		{
			name:    "empty_members",
			adj:     map[string][]string{},
			members: nil,
			want:    1.0,
		},
		{
			name:    "single_member",
			adj:     map[string][]string{"a": {}},
			members: []string{"a"},
			want:    1.0,
		},
		{
			name: "fully_connected_triangle",
			adj: map[string][]string{
				"a": {"b", "c"},
				"b": {"a", "c"},
				"c": {"a", "b"},
			},
			members: []string{"a", "b", "c"},
			want:    1.0,
		},
		{
			name: "no_internal_edges",
			adj: map[string][]string{
				"a": {"x"},
				"b": {"y"},
			},
			members: []string{"a", "b"},
			want:    0.0,
		},
		{
			name: "partial_connectivity",
			adj: map[string][]string{
				"a": {"b"},
				"b": {"a", "c"},
				"c": {"b"},
			},
			members: []string{"a", "b", "c"},
			want:    0.67, // 2/3
		},
		{
			name: "two_members_connected",
			adj: map[string][]string{
				"a": {"b"},
				"b": {"a"},
			},
			members: []string{"a", "b"},
			want:    1.0,
		},
		{
			name: "two_members_disconnected",
			adj: map[string][]string{
				"a": {},
				"b": {},
			},
			members: []string{"a", "b"},
			want:    0.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := ComputeCohesion(tt.adj, tt.members)
			if got != tt.want {
				t.Errorf("ComputeCohesion() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestLouvain(t *testing.T) {
	t.Parallel()

	t.Run("empty_graph", func(t *testing.T) {
		t.Parallel()
		result := Louvain(map[string][]string{})
		if len(result) != 0 {
			t.Errorf("expected empty map, got %v", result)
		}
	})

	t.Run("single_node", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{"a": {}}
		result := Louvain(adj)
		if len(result) != 1 {
			t.Errorf("expected 1 entry, got %d", len(result))
		}
		if _, ok := result["a"]; !ok {
			t.Error("expected key 'a' in result")
		}
	})

	t.Run("isolated_nodes_get_separate_communities", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {},
			"b": {},
			"c": {},
		}
		result := Louvain(adj)
		if len(result) != 3 {
			t.Errorf("expected 3 entries, got %d", len(result))
		}
		// Each isolated node should get its own community
		seen := make(map[int]bool)
		for _, cid := range result {
			seen[cid] = true
		}
		if len(seen) != 3 {
			t.Errorf("expected 3 distinct communities for isolated nodes, got %d", len(seen))
		}
	})

	t.Run("two_cliques_separate_communities", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {"b", "c"},
			"b": {"a", "c"},
			"c": {"a", "b"},
			"d": {"e", "f"},
			"e": {"d", "f"},
			"f": {"d", "e"},
		}
		result := Louvain(adj)
		if len(result) != 6 {
			t.Fatalf("expected 6 entries, got %d", len(result))
		}
		// Nodes in the same clique should have the same community
		if result["a"] != result["b"] || result["b"] != result["c"] {
			t.Errorf("clique {a,b,c} not in same community: a=%d b=%d c=%d",
				result["a"], result["b"], result["c"])
		}
		if result["d"] != result["e"] || result["e"] != result["f"] {
			t.Errorf("clique {d,e,f} not in same community: d=%d e=%d f=%d",
				result["d"], result["e"], result["f"])
		}
		if result["a"] == result["d"] {
			t.Error("expected different communities for two separate cliques")
		}
	})

	t.Run("fully_connected", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {"b", "c"},
			"b": {"a", "c"},
			"c": {"a", "b"},
		}
		result := Louvain(adj)
		// All should end up in the same community
		if result["a"] != result["b"] || result["b"] != result["c"] {
			t.Errorf("fully connected graph should be one community, got %v", result)
		}
	})

	t.Run("self_loops_and_duplicates_ignored", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {"a", "b", "b", "b"},
			"b": {"a"},
		}
		result := Louvain(adj)
		if len(result) != 2 {
			t.Errorf("expected 2 entries, got %d", len(result))
		}
	})

	t.Run("non_existent_neighbor_ignored", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {"b", "z"},
			"b": {"a"},
		}
		result := Louvain(adj)
		if _, ok := result["z"]; ok {
			t.Error("non-existent neighbor 'z' should not appear in result")
		}
	})

	t.Run("community_ids_are_contiguous", func(t *testing.T) {
		t.Parallel()
		adj := map[string][]string{
			"a": {"b"},
			"b": {"a"},
			"c": {"d"},
			"d": {"c"},
		}
		result := Louvain(adj)
		var ids []int
		for _, cid := range result {
			ids = append(ids, cid)
		}
		sort.Ints(ids)
		// IDs should start at 0 and be contiguous
		if ids[0] != 0 {
			t.Errorf("expected community IDs to start at 0, got %d", ids[0])
		}
	})
}
