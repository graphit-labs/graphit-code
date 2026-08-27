package store

// Where a context's store is, given only its name and the project asking.
//
// These are the counterpart of the registry: membership says WHICH contexts a project
// has, and these say WHERE each one's compiled data sits. Both live here because the
// answer depends on the origin — a Hub artifact is version-keyed, a link points at a
// store this project does not own, a local import has one of its own — and scattering
// that across ast and knowledge is how the two came to disagree in the first place.
//
// Nothing here records anything. Every path is derived, which is why the lockfile no
// longer stores one: a stored path froze at the moment it was written, and went wrong
// the first time the sibling it pointed at ran `init` and re-keyed its store.

// ASTContextDBPathIn resolves the graph that a context name points at for one project.
func ASTContextDBPathIn(projectDir, name string) string {
	rec, ok := LookupContext(projectDir, KindAST, name)
	if !ok {
		return ASTContextDBPath(name)
	}
	switch {
	case rec.IsHub():
		return ASTHubDBPath(rec.Name, rec.Version)
	case rec.IsLink() && rec.SourcePath != "":
		// The sibling's own project store, read in place. Derived from its path, so a
		// reindex or a re-key over there is picked up rather than frozen here.
		return ASTProjectDBPath(rec.SourcePath)
	default:
		return ASTContextDBPath(rec.Name)
	}
}

// KnowledgeContextDirIn resolves the documentation wiki that a context name points at
// for one project.
func KnowledgeContextDirIn(projectDir, name string) string {
	rec, ok := LookupContext(projectDir, KindKnowledge, name)
	if !ok {
		return KnowledgeContextDir(name)
	}
	switch {
	case rec.IsHub():
		return KnowledgeHubDir(rec.Name, rec.Version)
	case rec.IsLink() && rec.SourcePath != "":
		return KnowledgeProjectDir(rec.SourcePath)
	default:
		return KnowledgeContextDir(rec.Name)
	}
}
