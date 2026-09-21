//go:build lancedb

package uiserver

import (
	"context"
	"encoding/json"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/task"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestActivityUsesRealProjectRecordsWithoutTokens(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, brand.LockFileName()), []byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAT"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	svc, err := task.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	s, err := svc.SessionCreate(ctx, task.SessionCreateInput{Title: "Working session", Description: "Observe current ownership and durable progress", Strategy: "Inspect and verify", Actor: "coordinator"})
	if err != nil {
		t.Fatal(err)
	}
	s, err = svc.SessionClaim(ctx, s.ID, "coordinator", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	v, err := svc.Create(ctx, task.CreateInput{Title: "Working task", Description: "private long body", SessionID: s.ID, Actor: "author", AcceptanceCriteria: []string{"Must be observable"}, Tests: []string{"Check current metadata"}})
	if err != nil {
		t.Fatal(err)
	}
	v, err = svc.Claim(ctx, v.ID, "unit:worker", time.Hour)
	if err != nil {
		t.Fatal(err)
	}
	ms, err := memory.NewMemoryStore()
	if err != nil {
		t.Fatal(err)
	}
	mem := memory.NewMemoryService(memory.MemoryScopeProject, store.ProjectID(dir), ms).WithContext(ctx)
	id, err := mem.AddMemory("Recent decision", "secret memory body", memory.MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := indexChunk(t, knowledge.ReadDirIn(dir, ""), wiki.WikiChunk{Slug: "Contract", Title: "Maintained contract", Body: "secret wiki body", Updated: "2026-09-21T12:00:01Z"}); err != nil {
		t.Fatal(err)
	}
	result := readWorkspaceNow(ctx, dir, time.Now())
	for _, key := range []string{"tasks", "sessions", "memories", "knowledge"} {
		if x := result.Sections[key]; x.Error != "" || len(x.Items) != 1 {
			t.Fatalf("%s: %+v", key, x)
		}
	}
	if result.Sections["tasks"].Items[0].Owner != "unit:worker" || result.Sections["memories"].Items[0].ID != id {
		t.Fatal(result)
	}
	bytes, _ := json.Marshal(result)
	body := string(bytes)
	for _, secret := range []string{v.ClaimToken, s.ClaimToken, "snapshot_json", "secret memory body", "secret wiki body", "private long body"} {
		if secret != "" && strings.Contains(body, secret) {
			t.Fatalf("private data escaped: %s", secret)
		}
	}
	other := t.TempDir()
	if err := os.WriteFile(filepath.Join(other, brand.LockFileName()), []byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAS"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	foreign := readWorkspaceNow(ctx, other, time.Now())
	for key, section := range foreign.Sections {
		if section.Error != "" || len(section.Items) != 0 {
			t.Fatalf("empty project %s: %+v", key, section)
		}
	}

	future := readWorkspaceNow(ctx, dir, time.Now().Add(2*time.Hour))
	if len(future.Sections["tasks"].Items) != 0 || len(future.Sections["sessions"].Items) != 0 {
		t.Fatal("expired ownership reported active")
	}
}
