package task

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
)

func TestTableURIPlacesTaskPrefixUnderHubPrefix(t *testing.T) {
	t.Setenv("GRAPHIT_HUB_BUCKET", "shared")
	t.Setenv("GRAPHIT_HUB_PREFIX", "graphit/team")
	t.Setenv("GRAPHIT_TASK_PREFIX", "/work/tasks/")
	cfg := config.ConfigMap{
		"hub":  map[string]any{"bucket": "shared", "prefix": "graphit/team"},
		"task": map[string]any{"prefix": "/work/tasks/"},
	}
	got := TableURI("01ARZ3NDEKTSV4RRFFQ69G5FAV", cfg)
	if got != "s3://shared/graphit/team/v2/projects/01ARZ3NDEKTSV4RRFFQ69G5FAV/work/tasks" {
		t.Fatalf("TableURI() = %q", got)
	}
}

func TestTableURIRemoteRejectsNonULIDProject(t *testing.T) {
	cfg := config.ConfigMap{"hub": map[string]any{"bucket": "shared"}}
	if got := TableURI("project", cfg); got != "" {
		t.Fatalf("TableURI() = %q", got)
	}
}
