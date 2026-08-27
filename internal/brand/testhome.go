package brand

import (
	"os"
	"path/filepath"
	"testing"
)

// TestHomeRoot is the single parent every isolated test home is created under, so a
// full run leaves ONE directory to sweep instead of one per package binary.
//
// It reads Brand, which tests are free to reassign, so the value is only stable at
// init time — which is the only time this package uses it. Anything asserting on the
// home that was actually created must compare against isolatedTestHome instead.
func TestHomeRoot() string {
	return filepath.Join(os.TempDir(), Brand+"-test-homes")
}

// isolatedTestHome is the directory init pointed HOME at, or empty outside a test
// binary. Recorded because Brand is a mutable global: a test that reassigns it moves
// TestHomeRoot() with it, and the home this process is actually using does not move.
var isolatedTestHome string

// init points HOME at a throwaway directory whenever this binary is a test binary.
//
// Why an environment variable, and not a check inside GlobalDir(): the global
// directory is not the only thing a test can write into the operator's home. git reads
// ~/.gitconfig, the macOS scheduler writes ~/Library/LaunchAgents, and the IDE adapters
// expand "~" in paths the user supplied — none of which route through GlobalDir(), so a
// guard there would leave every one of them aimed at the real home. HOME is the variable
// os.UserHomeDir() reads on Unix, so moving it once covers all of them at once,
// including the callers added after this comment was written.
//
// It also covers the two cases no in-process check can reach:
//
//   - A SUBPROCESS. A test that spawns the daemon, or any git command, hands it the
//     environment of this process. The child is not a test binary, so
//     testing.Testing() is false over there and it would resolve the real home no
//     matter what this package returned for us. Inheriting an isolated HOME is what
//     actually stops it.
//   - GIT's own configuration. A temporary repository created by a test picks up
//     ~/.gitconfig, and on a real installation that config names a memory remote —
//     which is how a test run ends up adding a live repository as `origin` and
//     pushing test branches to it. XDG_CONFIG_HOME moves along with HOME for the
//     same reason: git prefers $XDG_CONFIG_HOME/git/config over
//     $HOME/.config/git/config, so leaving it behind would keep the developer's real
//     config reachable through the back door.
//
// A package that isolates HOME itself still wins. This runs at init, before any
// TestMain and before any t.Setenv, so it is the floor and never a ceiling.
//
// The directory is deliberately NOT removed at exit. os.Exit — which the generated
// test main calls, and which every TestMain in this repository calls — does not run
// deferred functions, and a package without a TestMain of its own offers no hook that
// fires after m.Run(). So each home is created under one predictable parent instead,
// and `make test` sweeps that parent before and after the run. Leaking a directory
// under the system temp dir is the cost this trade accepts; writing into the
// operator's real home is what it buys.
func init() {
	if !testing.Testing() {
		return
	}

	root := TestHomeRoot()
	if err := os.MkdirAll(root, 0o755); err != nil {
		// Falling back to the real home is the exact bug this exists to prevent, and
		// it would do it invisibly. A temp dir that cannot be created breaks
		// t.TempDir() and most of the suite anyway, so say so instead of continuing.
		panic("brand: cannot create the test home root " + root + ": " + err.Error())
	}
	home, err := os.MkdirTemp(root, "home-")
	if err != nil {
		panic("brand: cannot create an isolated test home under " + root + ": " + err.Error())
	}

	isolatedTestHome = home
	_ = os.Setenv("HOME", home)
	_ = os.Setenv("USERPROFILE", home) // Windows
	_ = os.Setenv("XDG_CONFIG_HOME", filepath.Join(home, ".config"))

	// NOTE: GlobalDir() honours this before it ever looks at HOME, so an operator who
	// exports it in their shell would hand the whole suite their real store.
	_ = os.Unsetenv(EnvVar("GLOBAL_DIR"))

	// A git identity, because emptying HOME is exactly what hides the developer's
	// ~/.gitconfig, and git refuses to commit without one:
	//
	//	fatal: unable to auto-detect email address (got 'user@host.(none)')
	//
	// That is not a side effect to work around per package — it is the direct
	// consequence of the isolation above, so it is answered here rather than left for
	// each package that happens to commit to rediscover. Exporting it is the right
	// answer anyway: a test that commits should not depend on who is running it.
	//
	// internal/memory and internal/hub still set the same values in their own
	// TestMain. That predates this and is left alone — the values are identical, and
	// those files carry the reasoning for the rest of what they isolate.
	for k, v := range map[string]string{
		"GIT_AUTHOR_NAME": "Test", "GIT_AUTHOR_EMAIL": "test@example.com",
		"GIT_COMMITTER_NAME": "Test", "GIT_COMMITTER_EMAIL": "test@example.com",
	} {
		_ = os.Setenv(k, v)
	}
}
