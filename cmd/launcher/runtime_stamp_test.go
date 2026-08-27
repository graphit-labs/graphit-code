package main

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/version"
)

// stampFixture lays out the two stamps the decision reads: one inside the runtime
// directory, one global. Empty content means "no such file".
type stampFixture struct {
	runtime string
	global  string
}

func (f stampFixture) write(t *testing.T, appDir, runtimeDir string) {
	t.Helper()
	if err := os.MkdirAll(runtimeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if f.runtime != "" {
		if err := os.WriteFile(runtimeStampPath(runtimeDir), []byte(f.runtime+"\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if f.global != "" {
		if err := os.MkdirAll(filepath.Dir(launcherStampPath(appDir)), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(launcherStampPath(appDir), []byte(f.global+"\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

// The decision must not consult the version string or the mere presence of the
// core binary. VERSION is pinned in the Makefile, so a rebuilt launcher carries
// the same v0.1.x as the runtime already on disk while the code differs; BuildID
// is what tells them apart.
func TestShouldExtractRuntime(t *testing.T) {
	version.BuildID = "build-under-test"
	t.Cleanup(func() { version.BuildID = "" })
	mine := computeBuildIDStamp()
	const other = "0000000000000000000000000000000000000000000000000000000000000000"

	for _, tc := range []struct {
		name string
		have stampFixture
		want bool
	}{
		{"first install, neither stamp exists", stampFixture{}, true},
		{"both stamps are this build", stampFixture{runtime: mine, global: mine}, false},
		{"runtime stamp missing: extraction never finished", stampFixture{global: mine}, true},
		{"global stamp missing: the daemon was never told", stampFixture{runtime: mine}, true},
		{"runtime stamp is another build: make install", stampFixture{runtime: other, global: mine}, true},
		{"global stamp is another build", stampFixture{runtime: mine, global: other}, true},
		{"both stamps are another build", stampFixture{runtime: other, global: other}, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			appDir := t.TempDir()
			runtimeDir := filepath.Join(appDir, "runtime", "v0.1.27")
			tc.have.write(t, appDir, runtimeDir)

			if got := shouldExtractRuntime(appDir, runtimeDir); got != tc.want {
				t.Errorf("shouldExtractRuntime = %v, want %v", got, tc.want)
			}
		})
	}
}

// A core binary sitting in the runtime directory used to be the whole condition,
// which made an interrupted extraction permanent for a versioned build.
func TestCoreBinaryAloneDoesNotSatisfyTheCheck(t *testing.T) {
	version.BuildID = "build-under-test"
	t.Cleanup(func() { version.BuildID = "" })

	appDir := t.TempDir()
	runtimeDir := filepath.Join(appDir, "runtime", "v0.1.27")
	if err := os.MkdirAll(runtimeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(runtimeDir, "graphit-core"), []byte("elf"), 0o755); err != nil {
		t.Fatal(err)
	}
	writeLauncherStamp(appDir)

	if !shouldExtractRuntime(appDir, runtimeDir) {
		t.Error("a runtime with a core binary but no stamp must be extracted again")
	}
}

// The regression this whole change exists for: after an extraction the launcher
// must be satisfied, and after a REBUILD under the same version it must not be.
func TestStampsRoundTripAcrossARebuild(t *testing.T) {
	t.Cleanup(func() { version.BuildID = "" })

	appDir := t.TempDir()
	runtimeDir := filepath.Join(appDir, "runtime", "v0.1.27")
	if err := os.MkdirAll(runtimeDir, 0o755); err != nil {
		t.Fatal(err)
	}

	version.BuildID = "first-build"
	writeRuntimeStamp(runtimeDir)
	writeLauncherStamp(appDir)
	if shouldExtractRuntime(appDir, runtimeDir) {
		t.Fatal("right after extracting, the runtime must be considered current")
	}

	// Same VERSION, new binary — what `make install` produces.
	version.BuildID = "second-build"
	if !shouldExtractRuntime(appDir, runtimeDir) {
		t.Error("a rebuilt launcher must re-extract; this is the bug that kept the old core running")
	}

	writeRuntimeStamp(runtimeDir)
	writeLauncherStamp(appDir)
	if shouldExtractRuntime(appDir, runtimeDir) {
		t.Error("after re-extracting, both stamps must agree again")
	}
}
