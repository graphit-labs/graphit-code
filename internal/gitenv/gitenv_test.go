package gitenv

import (
	"strings"
	"testing"
)

func TestWithoutInvocationScopeDropsOnlyTheHookScope(t *testing.T) {
	in := []string{
		"PATH=/usr/bin",
		"GIT_DIR=/elsewhere/.git",
		"GIT_INDEX_FILE=.git/index",
		"GIT_WORK_TREE=/elsewhere",
		"GIT_PREFIX=",
		"GIT_AUTHOR_DATE=@1000000000 +0000",
		"GIT_AUTHOR_NAME=Alguém",
		"GIT_SSH_COMMAND=ssh -v",
		"GIT_CONFIG_GLOBAL=/dev/null",
	}
	got := "\n" + strings.Join(WithoutInvocationScope(in), "\n") + "\n"
	for _, dropped := range []string{"GIT_DIR=", "GIT_INDEX_FILE=", "GIT_WORK_TREE=", "GIT_PREFIX=", "GIT_AUTHOR_DATE="} {
		if strings.Contains(got, "\n"+dropped) {
			t.Errorf("%s survived the filter", dropped)
		}
	}
	for _, kept := range []string{"PATH=/usr/bin", "GIT_AUTHOR_NAME=Alguém", "GIT_SSH_COMMAND=ssh -v", "GIT_CONFIG_GLOBAL=/dev/null"} {
		if !strings.Contains(got, "\n"+kept+"\n") {
			t.Errorf("%s was dropped", kept)
		}
	}
}
