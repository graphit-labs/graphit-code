package main

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

const fakeCoreEnv = "GRAPHIT_STORE_TEST_FAKE_CORE"

// TestMain doubles as the core binary for the fallback path.
func TestMain(m *testing.M) {
	if os.Getenv(fakeCoreEnv) == "1" {
		in, _ := io.ReadAll(os.Stdin)
		fmt.Printf("core:%s:%s", strings.Join(os.Args[1:], " "), in)
		os.Exit(7)
	}
	os.Exit(testenv.Run(m))
}

func useFakeCore(t *testing.T) *bool {
	t.Helper()
	called := new(bool)
	old := coreBinary
	coreBinary = func() (string, error) {
		*called = true
		return os.Executable()
	}
	t.Setenv(fakeCoreEnv, "1")
	t.Cleanup(func() { coreBinary = old })
	return called
}

func TestParseServesOnlyTheStoreMaintenanceForms(t *testing.T) {
	for _, args := range [][]string{
		{"task", "maintain"}, {"memory", "maintain"}, {"memory", "maintain", "--user"},
		{"memory", "embed", "--user"}, {"memory", "index", "--reset", "--user"},
	} {
		if _, ok := parse(args); !ok {
			t.Errorf("%v is not served", args)
		}
	}
	for _, args := range [][]string{
		nil, {"task"}, {"task", "maintain", "--user"}, {"memory", "embed", "--reset"},
		{"memory", "index", "--user", "--user"}, {"memory", "query"}, {"sync"},
	} {
		if _, ok := parse(args); ok {
			t.Errorf("%v is served, want the core", args)
		}
	}
}

// The core runs any form this binary does not know, with the same arguments and exit code.
func TestUnknownFormsRunInTheCore(t *testing.T) {
	called := useFakeCore(t)
	var out, errOut bytes.Buffer
	code := run([]string{"memory", "search", "x"}, &out, &errOut)
	if !*called || code != 7 || out.String() != "core:memory search x:" {
		t.Fatalf("code=%d coreCalled=%v out=%q err=%q", code, *called, out.String(), errOut.String())
	}
}

// Broker-scoped storage is authorized through the Hub registry, which only the core links.
func TestScopedStorageRunsInTheCore(t *testing.T) {
	called := useFakeCore(t)
	old := usesScopedStorage
	usesScopedStorage = func(context.Context) bool { return true }
	t.Cleanup(func() { usesScopedStorage = old })
	var out, errOut bytes.Buffer
	code := run([]string{"memory", "maintain", "--user"}, &out, &errOut)
	if !*called || code != 7 || out.String() != "core:memory maintain --user:" {
		t.Fatalf("code=%d coreCalled=%v out=%q err=%q", code, *called, out.String(), errOut.String())
	}
}
