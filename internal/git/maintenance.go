package git

import (
	"os"
	"strconv"
)

// DisableAutoMaintenance stops git from starting background maintenance in every
// repository this process touches, for as long as the process lives.
//
// It exists for tests, and for one specific failure they cannot otherwise avoid. A
// commit triggers `gc --auto`, which may leave a maintenance process writing inside
// `.git` AFTER the test that made the commit has returned. The test's temporary
// directory is then removed while that writer is still there, and the removal fails:
//
//	TempDir RemoveAll cleanup: unlinkat …/repo/.git: directory not empty
//
// The race is the test's own, not the code under test, and it only opens under load —
// which is why it appears in the full parallel suite and never when the test is run
// alone.
//
// Through the ENVIRONMENT rather than `git config` in each repository, because that is
// what makes it complete: it reaches every repository the process creates, including
// worktrees and clones made later, without any call site having to remember. Every git
// invocation here inherits os.Environ(), so exporting it once covers all of them.
//
// Appends to whatever GIT_CONFIG_* the caller already exported instead of replacing it,
// so this can never silently drop someone else's configuration.
func DisableAutoMaintenance() {
	settings := [][2]string{
		{"gc.auto", "0"},
		{"maintenance.auto", "false"},
	}

	start := 0
	if n, err := strconv.Atoi(os.Getenv("GIT_CONFIG_COUNT")); err == nil && n > 0 {
		start = n
	}

	for i, kv := range settings {
		idx := strconv.Itoa(start + i)
		_ = os.Setenv("GIT_CONFIG_KEY_"+idx, kv[0])
		_ = os.Setenv("GIT_CONFIG_VALUE_"+idx, kv[1])
	}
	_ = os.Setenv("GIT_CONFIG_COUNT", strconv.Itoa(start+len(settings)))
}
