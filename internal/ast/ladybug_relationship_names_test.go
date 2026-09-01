package ast

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

func canonicalRelationshipManifest() *ladybug.CanonicalManifest {
	return &ladybug.CanonicalManifest{RelGroups: []ladybug.CanonicalRelGroup{
		{
			Type: "CALLS",
			Members: []ladybug.CanonicalMember{
				{Table: "calls_file_function", Rows: 7},
				{Table: "calls_method_function", Rows: 5},
			},
			ReverseMembers: []ladybug.CanonicalMember{
				{Table: "calls_file_function_reverse", Rows: 7},
				{Table: "calls_method_function_reverse", Rows: 5},
			},
		},
		{
			Type:    "CONTAINS",
			Members: []ladybug.CanonicalMember{{Table: "contains_file_function", Rows: 19}},
		},
	}}
}

func TestRelationshipNamesComeFromEachProjectsIcebugManifest(t *testing.T) {
	t.Parallel()

	for _, tc := range []struct {
		project string
		logical string
	}{
		{project: "project-a", logical: "CALLS"},
		{project: "project-b", logical: "DEPENDS_ON"},
	} {
		t.Run(tc.project, func(t *testing.T) {
			t.Parallel()

			icebugDir := filepath.Join(t.TempDir(), tc.project, "graph.icebug")
			if err := os.MkdirAll(icebugDir, 0o755); err != nil {
				t.Fatalf("create graph.icebug: %v", err)
			}
			man := &ladybug.CanonicalManifest{
				Version:  ladybug.CanonicalManifestVersion,
				Format:   "icebug-canonical",
				Finished: true,
				RelGroups: []ladybug.CanonicalRelGroup{{
					Type:    tc.logical,
					Members: []ladybug.CanonicalMember{{Table: "same_physical_table", Rows: 3}},
				}},
			}
			raw, err := json.Marshal(man)
			if err != nil {
				t.Fatalf("marshal manifest: %v", err)
			}
			if err := os.WriteFile(filepath.Join(icebugDir, ladybug.IcebugManifestFile), raw, 0o644); err != nil {
				t.Fatalf("write graph.icebug/icebug.json: %v", err)
			}

			db := &LadybugBackend{cfg: LadybugConfig{IcebugDir: icebugDir}}
			if err := db.loadCanonicalManifestLocked(); err != nil {
				t.Fatalf("load project manifest: %v", err)
			}
			if got := db.logicalRelationshipType("same_physical_table"); got != tc.logical {
				t.Fatalf("resolved %q, want %q from this project's manifest", got, tc.logical)
			}
		})
	}
}

func TestCanonicalRelationshipNamesHidePhysicalMemberTables(t *testing.T) {
	man := canonicalRelationshipManifest()
	for _, tc := range []struct{ input, want string }{
		{"calls_file_function", "CALLS"},
		{"CALLS_METHOD_FUNCTION_REVERSE", "CALLS"},
		{"CALLS", "CALLS"},
		{"unmapped_storage_table", "unmapped_storage_table"},
	} {
		if got := canonicalLogicalRelationshipType(man, tc.input); got != tc.want {
			t.Errorf("canonicalLogicalRelationshipType(%q) = %q, want %q", tc.input, got, tc.want)
		}
	}
}

func TestCanonicalRelationshipStatsAggregateLogicalEdgesOnly(t *testing.T) {
	db := &LadybugBackend{canonical: canonicalRelationshipManifest()}
	stats, ok := db.logicalRelationshipStats()
	if !ok {
		t.Fatal("canonical manifest was not recognized")
	}
	if len(stats) != 2 {
		t.Fatalf("got %d relationship groups, want 2: %v", len(stats), stats)
	}
	if stats[0] != (relationshipTypeStat{Type: "CONTAINS", Count: 19}) {
		t.Errorf("first stat = %+v, want CONTAINS x19", stats[0])
	}
	if stats[1] != (relationshipTypeStat{Type: "CALLS", Count: 12}) {
		t.Errorf("second stat = %+v, want CALLS x12 without reverse mirrors", stats[1])
	}
}

func TestSchemaEdgeStatsUseLogicalManifestGroups(t *testing.T) {
	db := &LadybugBackend{canonical: canonicalRelationshipManifest()}
	stats := schemaEdgeStats(context.Background(), db)
	if len(stats) != 2 {
		t.Fatalf("schema published %d edge types, want 2 logical groups: %v", len(stats), stats)
	}
	if stats[0]["type"] != "CONTAINS" || stats[0]["count"] != int64(19) {
		t.Errorf("first schema edge = %v, want CONTAINS x19", stats[0])
	}
	if stats[1]["type"] != "CALLS" || stats[1]["count"] != int64(12) {
		t.Errorf("second schema edge = %v, want CALLS x12", stats[1])
	}
}

func TestGraphLinksUseLogicalRelationshipNames(t *testing.T) {
	db := &LadybugBackend{canonical: canonicalRelationshipManifest()}
	edges := []map[string]any{
		{"type": "CALLS_FILE_FUNCTION"},
		{"type": "contains_file_function"},
	}
	normalizeGraphEdgeTypes(db, edges)
	if edges[0]["type"] != "CALLS" || edges[1]["type"] != "CONTAINS" {
		t.Fatalf("physical relationship names crossed the API boundary: %v", edges)
	}
}
