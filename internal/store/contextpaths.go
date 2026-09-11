package store

// ASTContextIcebugDirIn resolves the icebug bundle for a context (filesystem on-the-fly, :memory: catalog).
func ASTContextIcebugDirIn(projectDir, name string) string {
	rec, ok := LookupContext(projectDir, KindAST, name)
	if !ok {
		return ASTContextIcebugDir(name)
	}
	switch {
	case rec.IsHub():
		return ASTHubDir(rec.Name, rec.Version)
	case rec.IsLink() && rec.SourcePath != "":
		return ASTProjectIcebugDir(rec.SourcePath)
	default:
		return ASTContextIcebugDir(rec.Name)
	}
}

// ASTContextDirIn resolves the store directory of a context for one project.
func ASTContextDirIn(projectDir, name string) string {
	rec, ok := LookupContext(projectDir, KindAST, name)
	if !ok {
		return ASTContextDir(name)
	}
	switch {
	case rec.IsHub():
		return ASTHubDir(rec.Name, rec.Version)
	case rec.IsLink() && rec.SourcePath != "":
		return ASTProjectDir(rec.SourcePath)
	default:
		return ASTContextDir(rec.Name)
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
