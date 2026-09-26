package git

import "sort"

// SupportedHookEvents are the event names documented by Git's githooks manual.
// Keeping this list explicit prevents a lockfile key from becoming a path under
// .git/hooks or a Git configuration key without validation.
var SupportedHookEvents = []string{
	"applypatch-msg", "commit-msg", "fsmonitor-watchman", "p4-changelist",
	"p4-post-changelist", "p4-pre-submit", "p4-prepare-changelist",
	"post-applypatch", "post-checkout", "post-commit", "post-index-change",
	"post-merge", "post-receive", "post-rewrite", "post-update",
	"pre-applypatch", "pre-auto-gc", "pre-commit", "pre-merge-commit",
	"pre-push", "pre-rebase", "pre-receive", "prepare-commit-msg",
	"proc-receive", "push-to-checkout", "reference-transaction",
	"sendemail-validate", "update",
}

func IsSupportedHookEvent(event string) bool {
	index := sort.SearchStrings(SupportedHookEvents, event)
	return index < len(SupportedHookEvents) && SupportedHookEvents[index] == event
}
