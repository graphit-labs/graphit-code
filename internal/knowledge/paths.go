package knowledge

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/store"
)

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

func WikiDirForContextIn(projectDir, name string) string {
	if name == "" || name == "__project__" {
		return WikiDirFor(projectDir)
	}
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
