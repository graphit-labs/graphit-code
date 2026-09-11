package hub

import (
	"testing"
)

func TestIsClusterSibling(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		current   *InstanceEntry
		candidate *InstanceEntry
		want      bool
	}{
		{
			name:      "both no labels",
			current:   &InstanceEntry{},
			candidate: &InstanceEntry{},
			want:      true,
		},
		{
			name:      "current no labels, candidate has",
			current:   &InstanceEntry{},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}},
			want:      false,
		},
		{
			name:      "current has labels, candidate no labels",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}},
			candidate: &InstanceEntry{},
			want:      false,
		},
		{
			name:      "matching label",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"backend"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"backend"}}},
			want:      true,
		},
		{
			name:      "non-matching labels",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"backend"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"frontend"}}},
			want:      false,
		},
		{
			name:      "different keys",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"backend"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"env": {"prod"}}},
			want:      false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := isClusterSibling(tt.current, tt.candidate)
			if got != tt.want {
				t.Errorf("isClusterSibling() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSharesLabel(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		current   *InstanceEntry
		candidate *InstanceEntry
		key       string
		want      bool
	}{
		{
			name:      "current lacks key",
			current:   &InstanceEntry{Cluster: ClusterMap{}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}},
			key:       "team",
			want:      false,
		},
		{
			name:      "candidate lacks key",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{}},
			key:       "team",
			want:      false,
		},
		{
			name:      "shared value",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"a", "b"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"b", "c"}}},
			key:       "team",
			want:      true,
		},
		{
			name:      "no shared value",
			current:   &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}},
			candidate: &InstanceEntry{Cluster: ClusterMap{"team": {"b"}}},
			key:       "team",
			want:      false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := sharesLabel(tt.current, tt.candidate, tt.key)
			if got != tt.want {
				t.Errorf("sharesLabel() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestHasLabelKey(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		inst *InstanceEntry
		key  string
		want bool
	}{
		{name: "has key", inst: &InstanceEntry{Cluster: ClusterMap{"team": {"a"}}}, key: "team", want: true},
		{name: "missing key", inst: &InstanceEntry{Cluster: ClusterMap{}}, key: "team", want: false},
		{name: "empty values", inst: &InstanceEntry{Cluster: ClusterMap{"team": {}}}, key: "team", want: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := hasLabelKey(tt.inst, tt.key)
			if got != tt.want {
				t.Errorf("hasLabelKey() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSameDir(t *testing.T) {
	t.Parallel()

	if !sameDir("/tmp", "/tmp") {
		t.Error("expected same dir")
	}
	if sameDir("/tmp/a", "/tmp/b") {
		t.Error("expected different dirs")
	}
}

func TestResolveCurrentProject(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()

	lock := &GlobalHubLock{
		Projects: map[string]*ProjectEntry{
			"proj1": {
				Instances: []InstanceEntry{
					{Dir: dir, Name: "test"},
				},
			},
		},
	}

	id, inst := resolveCurrentProject(dir, lock)
	if id != "proj1" {
		t.Errorf("id = %q, want %q", id, "proj1")
	}
	if inst == nil {
		t.Fatal("expected non-nil instance")
	}
	if inst.Name != "test" {
		t.Errorf("Name = %q, want %q", inst.Name, "test")
	}

	id, inst = resolveCurrentProject("/nonexistent", lock)
	if id != "" {
		t.Errorf("expected empty id, got %q", id)
	}
	if inst != nil {
		t.Error("expected nil instance")
	}
}
