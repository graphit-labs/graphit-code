//go:build lancedb

package lancestore

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

type minioSTSCredentials struct {
	AccessKeyID     string    `json:"access_key_id"`
	SecretAccessKey string    `json:"secret_access_key"`
	SessionToken    string    `json:"session_token"`
	ExpiresAt       time.Time `json:"expires_at"`
	Bucket          string    `json:"bucket"`
	Region          string    `json:"region"`
	Endpoint        string    `json:"endpoint"`
}

func TestMinIOSTSLanceRemoteAndShallowClone(t *testing.T) {
	credentialFile := os.Getenv("GRAPHIT_MINIO_STS_CREDENTIAL_FILE")
	if credentialFile == "" {
		t.Skip("GRAPHIT_MINIO_STS_CREDENTIAL_FILE is not set")
	}
	raw, err := os.ReadFile(credentialFile)
	if err != nil {
		t.Fatal(err)
	}
	var temporary minioSTSCredentials
	if err := json.Unmarshal(raw, &temporary); err != nil {
		t.Fatal(err)
	}
	if temporary.SessionToken == "" || !temporary.ExpiresAt.After(time.Now()) {
		t.Fatal("credential file does not contain a live STS session")
	}
	cfg := config.S3Config{
		Bucket: temporary.Bucket, Region: temporary.Region, Endpoint: temporary.Endpoint,
		AccessKeyID: temporary.AccessKeyID, SecretAccessKey: temporary.SecretAccessKey,
		SessionToken: temporary.SessionToken, ExpiresAt: temporary.ExpiresAt,
	}
	ctx := context.Background()
	prefix := "graphit/v2/projects/project-a/integration/lance-sts"
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = objects.DeletePrefix(context.Background(), prefix) })
	uri := "s3://" + temporary.Bucket + "/" + prefix

	remote, err := Open(ctx, Config{URI: uri, S3: cfg, Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	table, err := remote.CreateTable(ctx, "items", Schema{Fields: []Field{
		{Name: "id", Type: FieldString}, {Name: "value", Type: FieldString},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := table.Append(ctx, []Row{{"id": "base", "value": "remote"}}); err != nil {
		t.Fatal(err)
	}
	version, err := table.CurrentVersion(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := table.PutTag(ctx, "git-base", version); err != nil {
		t.Fatal(err)
	}
	if err := remote.Close(); err != nil {
		t.Fatal(err)
	}

	remote, err = Open(ctx, Config{URI: uri, S3: cfg})
	if err != nil {
		t.Fatal(err)
	}
	table, err = remote.OpenTable(ctx, "items")
	if err != nil {
		t.Fatal(err)
	}
	if count, countErr := table.Count(ctx); countErr != nil || count != 1 {
		t.Fatalf("remote count = %d, %v", count, countErr)
	}
	if err := remote.Close(); err != nil {
		t.Fatal(err)
	}

	targetDir := t.TempDir()
	target, err := Open(ctx, Config{URI: targetDir, S3: cfg, Writable: true})
	if err != nil {
		t.Fatal(err)
	}
	clone, err := target.CloneTable(ctx, "items", uri+"/items.lance", CloneOptions{SourceTag: "git-base"})
	if err != nil {
		t.Fatal(err)
	}
	if files := regularFilesUnder(filepath.Join(targetDir, "items.lance", "data")); files != 0 {
		t.Fatalf("shallow clone copied %d inherited data files", files)
	}
	if err := clone.Append(ctx, []Row{{"id": "local", "value": "overlay"}}); err != nil {
		t.Fatal(err)
	}
	if count, countErr := clone.Count(ctx); countErr != nil || count != 2 {
		t.Fatalf("clone count = %d, %v", count, countErr)
	}
	if err := target.Close(); err != nil {
		t.Fatal(err)
	}
}
