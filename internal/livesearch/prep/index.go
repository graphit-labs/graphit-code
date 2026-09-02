package prep

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/memory"
)

// An ephemeral project compiles nothing, and owns nothing.
//
// This used to be the daemon's work done inline: a real project has a daemon that
// compiles its documentation wiki, and a session has no daemon and no time for one,
// so preparation built one itself. That is no longer necessary, and it was never
// harmless. It is unnecessary because an installed knowledge context now arrives
// already compiled — a Hub knowledge artifact is a versioned LanceDB index mounted from S3. It was
// harmful because
// the only place it could compile to was the wiki keyed by this workspace's project
// ID, and a throwaway search does not get a documentation wiki of its own.
//
// So all three kinds of material are read where they already live:
//
//   - the chosen documentation wikis, each already compiled in its context store;
//   - the chosen code graphs, already built, which only need to resolve;
//   - the user's own memory, which belongs to the user and applies everywhere.
//
// What remains here is reporting. A session that resolved nothing and a session that
// had nothing chosen look identical otherwise, and the difference matters to whoever
// is waiting for the search.

// prepareIndexes reports what the session can reach. Nothing is built.
func prepareIndexes(ctx context.Context, ws string, progress func(string)) error {
	reportKnowledge(ws, progress)
	if err := ctx.Err(); err != nil {
		return err
	}

	reportGraphs(ws, progress)

	prepareUserMemory(ws, progress)
	return ctx.Err()
}

// reportKnowledge names the documentation sets the session can search.
//
// Each one is searched in its own context store, so a session covering several is a
// fan-out at query time rather than a merge at preparation time. That is what keeps
// the workspace from needing a wiki of its own — see knowledge.ReadSourcesIn.
func reportKnowledge(ws string, progress func(string)) {
	names := knowledge.InstalledContextsIn(ws)
	if len(names) == 0 {
		return
	}
	sort.Strings(names)
	progress(fmt.Sprintf("%s ready to search: %s",
		countNoun(len(names), "documentation set"), strings.Join(names, ", ")))
}

// reportGraphs says which code graphs the agent can reach.
//
// Nothing is built here. An AST artifact is installed into a versioned store shared
// by every project pinned to that version, and a project reaches it through its own
// lockfile entry — which the install already wrote. So the graph is ready the moment
// the artifact is installed, and the only useful thing preparation can add is to say
// so, by name, so that a session that quietly resolved nothing is distinguishable
// from one that had nothing chosen.
func reportGraphs(ws string, progress func(string)) {
	contexts := ast.ListImportedContextsIn(ws)
	if len(contexts) == 0 {
		return
	}
	names := make([]string, 0, len(contexts))
	for id := range contexts {
		names = append(names, id)
	}
	sort.Strings(names)
	progress(fmt.Sprintf("%s ready to query: %s",
		countNoun(len(names), "code graph"), strings.Join(names, ", ")))
}

// prepareUserMemory brings the user's own memory into the workspace.
//
// Only the user's. The ephemeral project has no memory of its own — it has never
// been worked in, and it will not exist long enough to learn anything — but the
// user's memory is about the user, applies everywhere, and is often the only place a
// constraint was ever written down.
//
// Nothing is copied into the workspace: the user memory wiki has one location, in
// the global brand directory, and this session opens it there like every other
// reader. What is left here is the check that it exists and is not empty.
//
// The lesson from the version that did copy is worth keeping, because it applies to
// anything this package writes: a destination must not be derived from the process
// working directory. A server runs several sessions at once, so `git rev-parse
// --show-toplevel` or the cwd names one arbitrary project — whichever the server was
// started in — and that is not imprecision, it is somebody else's directory.
//
// Failure here is reported and survivable. A machine with no git identity has no
// user memory to find, which is a normal state and not a broken one.
func prepareUserMemory(ws string, progress func(string)) {
	hash, err := memory.UserScopeID()
	if err != nil {
		progress("no user memory available: " + err.Error())
		return
	}
	store, err := memory.NewMemoryStore()
	if err != nil {
		progress("no user memory available: " + err.Error())
		return
	}

	svc := memory.NewMemoryService(memory.MemoryScopeUser, hash, store)
	if err := svc.EnsureInitialised(); err != nil {
		progress("the user memory could not be opened: " + err.Error())
		return
	}

	source := svc.WikiDir()
	if source == "" {
		progress("no user memory available: the memory wiki has not been built")
		return
	}
	// Nothing is copied. The user memory wiki has one location, in the global brand
	// directory, and every reader — including this session's — opens it there. The
	// copy that used to happen here existed only because the memory search read a
	// project-local replica, and its destination had to be chosen by hand precisely
	// because the service derived one from a process-global working directory.
	if entries, err := os.ReadDir(source); err != nil || len(entries) == 0 {
		progress("the user memory is empty, so there is nothing of yours to recall here")
		return
	}
	progress("your memory is available to this search")
}
