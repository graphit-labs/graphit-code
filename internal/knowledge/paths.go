package knowledge

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/store"
)

// A knowledge wiki lives once, in the global brand directory, keyed by whose it is:
//
//	<global>/wiki/knowledge/project/<project-id>/   the project's own documentation
//	<global>/wiki/knowledge/context/<name>/         an imported documentation set
//
// It used to be compiled into `<project>/.graphit/knowledge/project` and every
// imported context was COPIED into the project beside it. That cost a copy of every
// page per project, and the copy had to be resynchronised whenever the source moved
// — logic whose only purpose was to stop two copies of the same wiki from
// disagreeing. An agent never reads these pages as files anyway: it reads them
// through the wiki MCP tools, which take the project as a parameter and can
// therefore serve a directory the agent could not open itself.
//
// Which contexts a project has imported is recorded in its context registry; see
// internal/store.

// WikiDirFor is the compiled documentation wiki of one project.
func WikiDirFor(projectDir string) string {
	return store.KnowledgeProjectDir(projectDir)
}

// WikiDir is WikiDirFor the working directory. Prefer WikiDirFor wherever the
// project is known: a server or an MCP process serving one project while sitting in
// another resolves the wrong project's wiki here, and silently.
func WikiDir() string {
	wd, _ := os.Getwd()
	return WikiDirFor(wd)
}

// WikiDirForContextIn resolves the wiki directory of a context — or of the project
// itself, for an empty name.
//
// There is one shape now. Every install path — a hub branch, a published artifact, a
// link to a sibling — places the compiled wiki AT the context directory, so there is
// nothing to probe for. This used to have to try a `wiki/` subdirectory as well,
// because the two install paths disagreed about where the wiki went, and reconciling
// them on the reading side meant every reader could be handed either.
func WikiDirForContextIn(projectDir, name string) string {
	if name == "" || name == "__project__" {
		return WikiDirFor(projectDir)
	}
	// The store decides where a context's wiki sits, because the answer depends on the
	// origin: a Hub artifact is version-keyed, a link points at a sibling's own wiki,
	// a local import has one of its own.
	return store.KnowledgeContextDirIn(projectDir, name)
}

// WikiDirForContext is WikiDirForContextIn against the working directory.
func WikiDirForContext(name string) string {
	wd, _ := os.Getwd()
	return WikiDirForContextIn(wd, name)
}

// InstalledContextsIn lists the knowledge contexts projectDir has imported.
//
// It reads the project's registry, not a directory. Listing the global wiki root
// would report every context anybody on this machine ever installed, which is not
// what "installed here" means.
func InstalledContextsIn(projectDir string) []string {
	var names []string
	for _, name := range store.ContextNames(projectDir, store.KindKnowledge) {
		if rec, ok := store.LookupContext(projectDir, store.KindKnowledge, name); ok && rec.IsHub() {
			// Published knowledge is mounted from its immutable object-store URI. There
			// is deliberately no local directory whose existence could prove the claim.
			names = append(names, name)
			continue
		}
		if _, err := os.Stat(WikiDirForContextIn(projectDir, name)); err == nil {
			names = append(names, name)
		}
	}
	return names
}

// InstalledContexts is InstalledContextsIn against the working directory.
func InstalledContexts() []string {
	wd, _ := os.Getwd()
	return InstalledContextsIn(wd)
}

// ReadDirIn returns the wiki directory a knowledge read over projectDir covers, or ""
// when there is none.
//
// Naming a context returns that context's directory, for any project. Otherwise a
// normal project returns its own wiki.
//
// An ephemeral workspace returns nothing. It has no documentation of its own and will
// not live long enough to acquire any, so an unqualified read over it has no answer —
// the sets it can search are the ones it selected, and reaching them means naming one.
// This is the same shape the code graph has: a session queries contexts, never a store
// of its own.
func ReadDirIn(projectDir, contextName string) string {
	if contextName != "" {
		return WikiDirForContextIn(projectDir, contextName)
	}
	if store.IsEphemeralProject(projectDir) {
		return ""
	}
	return WikiDirForContextIn(projectDir, "")
}
