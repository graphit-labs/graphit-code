//go:build lancedb

package hub

import (
	"context"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func TestPublishBranchLanceRecordsCommitTagsInOneBranchLineage(t *testing.T) {
	ctx := context.Background()
	_, endpoint := testsupport.StartFakeS3(t, "graphit-hub")
	cfg := config.S3Config{
		Bucket:          "graphit-hub",
		Region:          "us-east-1",
		Endpoint:        endpoint,
		AccessKeyID:     "test-key",
		SecretAccessKey: "test-secret",
	}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	remote := &S3Store{objects: objects, cfg: cfg}
	manager := &RegistryManager{store: remote}
	meta := &Entry{Type: TypeKnowledge, ProjectID: "project"}
	branchVersion := "branch/feature/hub-sync"

	stagedRoot := t.TempDir()
	indexPath := wiki.WikiIndexPath(stagedRoot)
	local, err := lancestore.Open(ctx, lancestore.Config{URI: indexPath})
	if err != nil {
		t.Fatal(err)
	}
	table, err := local.CreateTable(ctx, "meta", lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString},
		{Name: "value", Type: lancestore.FieldString, Nullable: true},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Append(ctx, []lancestore.Row{{"key": "state", "value": "one"}}); err != nil {
		t.Fatal(err)
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}

	first, err := manager.publishBranchLance(ctx, "ignored", branchVersion, meta, stagedRoot, gitstate.Snapshot{Branch: "feature/hub-sync", Commit: "c1", Detached: true})
	if err != nil {
		t.Fatal(err)
	}
	if err := remote.writeBranchHistory(ctx, TypeKnowledge, "ignored", branchVersion, "project", first); err != nil {
		t.Fatal(err)
	}

	local, err = lancestore.Open(ctx, lancestore.Config{URI: indexPath})
	if err != nil {
		t.Fatal(err)
	}
	table, err = local.OpenTable(ctx, "meta")
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Upsert(ctx, "key", []lancestore.Row{{"key": "state", "value": "two"}}); err != nil {
		t.Fatal(err)
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}

	second, err := manager.publishBranchLance(ctx, "ignored", branchVersion, meta, stagedRoot, gitstate.Snapshot{Branch: "feature/hub-sync", Commit: "c2"})
	if err != nil {
		t.Fatal(err)
	}
	if len(second.Commits) != 2 || second.Commits[0].Commit != "c2" || second.Commits[1].Commit != "c1" {
		t.Fatalf("branch history = %#v", second.Commits)
	}
	for _, commit := range second.Commits {
		if got := commit.Tables["meta"].Tag; got != lanceCommitTag(commit.Commit) {
			t.Fatalf("commit %s tag = %q", commit.Commit, got)
		}
	}

	remoteURI := remote.ArtifactURI(TypeKnowledge, "ignored", branchVersion, "project", wiki.WikiIndexDirName)
	published, err := lancestore.Open(ctx, remote.lanceConfig(remoteURI, false))
	if err != nil {
		t.Fatal(err)
	}
	defer published.Close()
	publishedTable, err := published.OpenTable(ctx, "meta")
	if err != nil {
		t.Fatal(err)
	}
	rows, err := publishedTable.Search(ctx, lancestore.Query{Filter: `"key" = 'state'`, Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 1 || rows[0].Row["value"] != "two" {
		t.Fatalf("published rows = %#v", rows)
	}
}
