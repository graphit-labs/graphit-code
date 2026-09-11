package ai

import (
	"os"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/testsupport/testenv"
)

// The model-cache override is CLEARED for this package, and that is the opposite of what
// `make test` wants everywhere else.
//
// `make test` sets <BRAND>_MODEL_CACHE to one shared directory so that the packages which need a
// real embedder do not each download ~132 MB into their own throwaway HOME. This package is the
// one that must not see it: nearly every test here isolates HOME and then asserts something about
// model PATHS — that a fresh home has no model, that constructing a manager creates nothing, that
// a seeded cache is the one that gets read. An override pointing at a shared directory silently
// defeats all of that, and it defeats it in the direction that makes a test pass while measuring
// the wrong filesystem.
//
// Cleared here rather than per test, for the reason internal/brand/testhome.go gives for doing the
// HOME isolation in init: a floor is reliable and a per-test opt-in is something a new test
// forgets. A test that genuinely wants the shared cache can still set it with t.Setenv.
func TestMain(m *testing.M) {
	_ = os.Unsetenv(brand.EnvVar("MODEL_CACHE"))
	os.Exit(testenv.Run(m))
}
