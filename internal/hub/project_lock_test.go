package hub

import (
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestProjectClusterMutationUpdatesLockAndLocalProjection(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	projectDir := t.TempDir()
	projectID := "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	lockPath := filepath.Join(projectDir, brand.LockFileName())
	if err := SaveLockfile(lockPath, &Lockfile{Project: ProjectIdentity{ID: projectID, Name: "demo"}}); err != nil {
		t.Fatal(err)
	}
	if err := SetProjectClusterLabel(projectDir, projectID, " team ", " backend "); err != nil {
		t.Fatal(err)
	}
	lf, err := LoadLockfile(lockPath)
	if err != nil {
		t.Fatal(err)
	}
	if got := lf.Project.Cluster["team"]; len(got) != 1 || got[0] != "backend" {
		t.Fatalf("project lock cluster = %#v", lf.Project.Cluster)
	}
	mgr, err := NewGlobalLockManager()
	if err != nil {
		t.Fatal(err)
	}
	projected, err := mgr.GetAllClusterLabels(projectID, projectDir)
	if err != nil {
		t.Fatal(err)
	}
	if got := projected["team"]; len(got) != 1 || got[0] != "backend" {
		t.Fatalf("global projection = %#v", projected)
	}
	if err := UnsetProjectClusterLabel(projectDir, projectID, "team"); err != nil {
		t.Fatal(err)
	}
	lf, _ = LoadLockfile(lockPath)
	if lf.Project.Cluster != nil {
		t.Fatalf("project lock cluster after unset = %#v", lf.Project.Cluster)
	}
}

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
