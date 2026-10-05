// Package gitenv strips the Git invocation scope a process can inherit.
//
// A Git hook exports the invocation it belongs to — GIT_DIR, GIT_INDEX_FILE, GIT_PREFIX,
// GIT_AUTHOR_DATE — and every process the hook starts inherits it. GIT_INDEX_FILE=.git/index
// is RELATIVE, so it re-resolves against whatever repository the next `git -C` names; in a
// linked worktree `.git` is a file and the command fails. A git command Graphit runs against
// a repository it chose itself must therefore drop that scope.
package gitenv

import "strings"

var invocationScope = map[string]struct{}{
	"GIT_DIR":                          {},
	"GIT_COMMON_DIR":                   {},
	"GIT_WORK_TREE":                    {},
	"GIT_INDEX_FILE":                   {},
	"GIT_INDEX_VERSION":                {},
	"GIT_PREFIX":                       {},
	"GIT_OBJECT_DIRECTORY":             {},
	"GIT_ALTERNATE_OBJECT_DIRECTORIES": {},
	"GIT_NAMESPACE":                    {},
	"GIT_GRAFT_FILE":                   {},
	"GIT_QUARANTINE_PATH":              {},
	"GIT_REFLOG_ACTION":                {},
	"GIT_AUTHOR_DATE":                  {},
	"GIT_COMMITTER_DATE":               {},
}

// WithoutInvocationScope returns environ without the variables that tie a git command to
// the invocation of an enclosing hook. Everything else, other GIT_* settings included, is
// kept.
func WithoutInvocationScope(environ []string) []string {
	clean := make([]string, 0, len(environ))
	for _, kv := range environ {
		if name, _, ok := strings.Cut(kv, "="); ok {
			if _, drop := invocationScope[name]; drop {
				continue
			}
		}
		clean = append(clean, kv)
	}
	return clean
}
