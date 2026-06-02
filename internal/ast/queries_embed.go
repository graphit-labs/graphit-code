package ast

// AST YAML files (queries, frameworks, ecosystems) are no longer embedded
// in the graphit-core binary. They are embedded in the launcher and
// extracted to ~/.graphit/runtime/<version>/ast/ during binary setup.
// The query_loader reads them from the runtime directory.
