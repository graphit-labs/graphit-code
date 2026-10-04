//go:build lancedb

package main

import (
	"bytes"
	"context"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

// The daemon's store maintenance runs to completion here, without the core.
func TestStoreMaintenanceRunsWithoutTheCore(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	dir := t.TempDir()
	if _, err := projectlock.EnsureIdentity(filepath.Join(dir, brand.LockFileName())); err != nil {
		t.Fatal(err)
	}
	t.Chdir(dir)
	called := useFakeCore(t)
	old := usesScopedStorage
	usesScopedStorage = func(context.Context) bool { return false }
	t.Cleanup(func() { usesScopedStorage = old })

	for _, args := range [][]string{{"memory", "index"}, {"memory", "maintain"}, {"task", "maintain"}} {
		var out, errOut bytes.Buffer
		if code := run(args, &out, &errOut); code != 0 || *called {
			t.Fatalf("%v: code=%d coreCalled=%v err=%q", args, code, *called, errOut.String())
		}
	}
}
