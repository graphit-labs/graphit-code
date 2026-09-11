//go:build lancedb

package memory

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/testsupport"
)

func TestRemoteAuthoritativeTableSupportsDirectSearchAndRead(t *testing.T) {
	ctx := context.Background()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	_, endpoint := testsupport.StartFakeS3(t, "memory-direct")
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), filepath.Join(home, brand.DotDir()))
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := authStore.AddProvider(auth.Provider{Name: "test", Type: auth.ProviderLocal, Local: &auth.LocalConfig{}, S3: auth.S3Config{Bucket: "memory-direct", Region: "us-east-1", Endpoint: endpoint}}); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{Name: "alice", Provider: "test", Username: "alice", S3: auth.S3Credentials{AccessKeyID: "test-key", SecretAccessKey: "test-secret"}}); err != nil {
		t.Fatal(err)
	}

	svc := NewMemoryService(MemoryScopeUser, "alice", nil).WithContext(ctx)
	table, err := svc.openTable(ctx)
	if err != nil {
		t.Fatalf("opening remote authoritative table: %v", err)
	}
	const id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	if err := table.Put(ctx, MemoryRecord{
		ID: id, Title: "Remote memory", Body: "sideralium remote marker", Type: "fact", Revision: 1,
	}); err != nil {
		t.Fatalf("writing remote memory: %v", err)
	}
	if err := table.Close(); err != nil {
		t.Fatal(err)
	}

	results, err := svc.SearchMemories(ctx, "sideralium", 5, SearchOptions{})
	if err != nil {
		t.Fatalf("searching remote table: %v", err)
	}
	if len(results) != 1 || results[0].MemoryID != id {
		t.Fatalf("remote search = %+v", results)
	}
	content, found, err := svc.ReadMemory(ctx, id)
	if err != nil || !found || !strings.Contains(content, "sideralium remote marker") {
		t.Fatalf("remote read found=%v err=%v content=%q", found, err, content)
	}

	if _, err := os.Stat(filepath.Join(home, ".graphit", "wiki", "memory")); !os.IsNotExist(err) {
		t.Fatalf("direct remote retrieval created a local memory projection: %v", err)
	}
}
