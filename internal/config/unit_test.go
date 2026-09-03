package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The identity is generated once and persisted INTO THE GLOBAL CONFIG, so it survives restarts and
// is visible and editable like every other setting. It needs no other tool present, which is the
// whole point of replacing `git config user.email`.
func TestUnitIDIsGeneratedOnceAndPersistedInTheGlobalConfig(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_UNIT_ID", "")
	resetUnitCache()

	first, err := UnitID()
	if err != nil {
		t.Fatalf("UnitID: %v", err)
	}
	if first == "" {
		t.Fatal("UnitID returned empty")
	}

	path, err := globalConfigPath()
	if err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading the global config: %v", err)
	}
	var cfg map[string]any
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("global config is not valid JSON: %v", err)
	}
	unit, ok := cfg["unit"].(map[string]any)
	if !ok {
		t.Fatalf("global config has no `unit` section: %s", raw)
	}
	if unit["id"] != first {
		t.Errorf("unit.id in the config is %v, want %q", unit["id"], first)
	}

	resetUnitCache()
	second, err := UnitID()
	if err != nil {
		t.Fatalf("second UnitID: %v", err)
	}
	if first != second {
		t.Errorf("UnitID is not stable: %q then %q", first, second)
	}
}

// The override is how two installations become one unit — which is what makes a person's
// user-scope memories follow them across machines.
func TestUnitIDOverrideWins(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_UNIT_ID", "team-shared-identity")
	resetUnitCache()

	got, err := UnitID()
	if err != nil {
		t.Fatalf("UnitID: %v", err)
	}
	if got != "team-shared-identity" {
		t.Errorf("UnitID = %q, want the configured override", got)
	}

	path, err := globalConfigPath()
	if err != nil {
		t.Fatal(err)
	}
	if raw, err := os.ReadFile(path); err == nil && strings.Contains(string(raw), "team-shared-identity") {
		t.Error("an overridden unit id was written into the global config")
	}
}

// ResolveUnitID reports what is set without generating anything, which is what callers that only
// want to know "has an identity been chosen" need.
func TestResolveUnitIDDoesNotGenerate(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_UNIT_ID", "")
	resetUnitCache()

	if got := ResolveUnitID(nil, nil); got != "" {
		t.Errorf("ResolveUnitID = %q on a fresh install, want empty", got)
	}
	if _, err := os.Stat(filepath.Join(home, ".graphit", globalConfigFile)); err == nil {
		t.Error("ResolveUnitID created the global config — it must only read")
	}
}

// It is one identity per installation even under concurrency: two callers racing must not each
// generate one and have the second overwrite the first.
func TestUnitIDIsStableUnderConcurrency(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_UNIT_ID", "")
	resetUnitCache()

	const n = 8
	ids := make([]string, n)
	errs := make([]error, n)
	done := make(chan int, n)
	for i := 0; i < n; i++ {
		go func(i int) {
			ids[i], errs[i] = UnitID()
			done <- i
		}(i)
	}
	for i := 0; i < n; i++ {
		<-done
	}
	for i, err := range errs {
		if err != nil {
			t.Fatalf("goroutine %d: %v", i, err)
		}
	}
	for i := 1; i < n; i++ {
		if ids[i] != ids[0] {
			t.Fatalf("concurrent callers saw different identities: %q and %q", ids[0], ids[i])
		}
	}
}

// Reranking is OFF unless the operator turned it on, and that default is what gates a 1.04 GiB
// model download: absent means false, not "ask the model manager".
func TestSearchRerankDefaultsOff(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv("GRAPHIT_SEARCH_RERANK", "")

	if SearchRerank() {
		t.Error("reranking is on by default — the reranker model would be fetched unasked")
	}
	if ResolveSearchRerank(ConfigMap{"search": map[string]any{"rerank": "true"}}, nil) != true {
		t.Error("an explicit true was not honoured")
	}
	if ResolveSearchRerank(ConfigMap{"search": map[string]any{"rerank": "yes"}}, nil) {
		t.Error(`"yes" was accepted; only "true" enables a module switch here`)
	}
}
