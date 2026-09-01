package ide

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The mandate block is assembled from a shared preamble plus one block per module,
// but only the module blocks were ever hashed. A release that changed nothing but the
// preamble therefore reached no project: every fast path reported the file current,
// and the agent kept reading instructions the binary had stopped generating.
func TestUpsertMandateTriggerRewritesWhenOnlyThePreambleChanged(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, GlobalRulesFile("claude"))

	if err := UpsertMandateTrigger(dir, "claude", "mem_rule", "MEM"); err != nil {
		t.Fatal(err)
	}

	// Simulate the binary's preamble moving on while every trigger stays identical.
	stale := strings.Replace(mandatePreamble(),
		"You are the", "You are the STALE", 1)
	data, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	rewritten := strings.Replace(string(data), strings.TrimSpace(mandatePreamble()),
		strings.TrimSpace(stale), 1)
	if rewritten == string(data) {
		t.Fatal("could not plant a stale preamble: the generated file did not contain the current one")
	}
	if err := os.WriteFile(target, []byte(rewritten), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := UpsertMandateTrigger(dir, "claude", "mem_rule", "MEM"); err != nil {
		t.Fatal(err)
	}

	got, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(got), "You are the STALE") {
		t.Error("a stale preamble survived a sync: the trigger hash matched, so the preamble was never compared")
	}
	if !strings.Contains(string(got), strings.TrimSpace(mandatePreamble())) {
		t.Error("the rewrite did not restore the preamble the running binary generates")
	}
	if !strings.Contains(string(got), "<mem_rule>MEM</mem_rule>") {
		t.Error("the rewrite dropped the module trigger it was supposed to preserve")
	}
}

// The mtime fast path returns before reading the file at all, so a preamble change has
// to be caught by the cache itself rather than by re-parsing what is on disk.
func TestUpsertMandateTriggerIgnoresTheMtimeFastPathAfterAPreambleChange(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, GlobalRulesFile("claude"))

	if err := UpsertMandateTrigger(dir, "claude", "mem_rule", "MEM"); err != nil {
		t.Fatal(err)
	}

	cache := loadMandateHashCache(brand.ProjectRuntimePath(dir))
	if cache.Preamble != mandatePreambleHash() {
		t.Fatalf("the write did not record the preamble hash: got %q", cache.Preamble)
	}

	// A cache that predates preamble hashing has the field empty, which must not be
	// read as "the preamble matches".
	cache.Preamble = ""
	saveMandateHashCache(brand.ProjectRuntimePath(dir), cache)

	before, _ := os.ReadFile(target)
	if err := UpsertMandateTrigger(dir, "claude", "mem_rule", "MEM"); err != nil {
		t.Fatal(err)
	}
	after, _ := os.ReadFile(target)

	if string(before) != string(after) {
		t.Error("the file content changed, which is unexpected here — only the cache should be repaired")
	}
	repaired := loadMandateHashCache(brand.ProjectRuntimePath(dir))
	if repaired.Preamble != mandatePreambleHash() {
		t.Error("an empty preamble hash was left empty, so every later sync keeps re-checking the file")
	}
}

// Removing a module rewrites the block from the triggers that remain. It used to reuse
// the inner content verbatim, which carried whatever preamble was already on disk and
// made removal a second source of drift.
func TestRemoveMandateTriggerRegeneratesThePreamble(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, GlobalRulesFile("claude"))

	for _, tag := range []string{"mem_rule", "ast_rule"} {
		if err := UpsertMandateTrigger(dir, "claude", tag, strings.ToUpper(tag)); err != nil {
			t.Fatal(err)
		}
	}

	stale := strings.Replace(mandatePreamble(), "You are the", "You are the STALE", 1)
	data, _ := os.ReadFile(target)
	planted := strings.Replace(string(data), strings.TrimSpace(mandatePreamble()),
		strings.TrimSpace(stale), 1)
	if err := os.WriteFile(target, []byte(planted), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := RemoveMandateTrigger(dir, "claude", "ast_rule"); err != nil {
		t.Fatal(err)
	}

	got, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(got), "You are the STALE") {
		t.Error("removal carried the stale preamble forward instead of regenerating it")
	}
	if strings.Contains(string(got), "<ast_rule>") {
		t.Error("the removed trigger is still in the file")
	}
	if !strings.Contains(string(got), "<mem_rule>MEM_RULE</mem_rule>") {
		t.Error("removal dropped a trigger that should have survived")
	}

	cache := loadMandateHashCache(brand.ProjectRuntimePath(dir))
	if _, ok := cache.Hashes["ast_rule"]; ok {
		t.Error("the removed trigger is still in the hash cache, so a later sync compares against a module that is gone")
	}
}
