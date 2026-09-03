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
	got := TableURI("project-1", cfg)
	if got != "s3://shared/graphit/team/work/tasks/project/project-1" {
		t.Fatalf("TableURI() = %q", got)
	}
}
