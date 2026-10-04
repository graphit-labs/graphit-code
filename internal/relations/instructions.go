package relations

import "github.com/graphit-labs/graphit-code/internal/relationtext"

// The reference instructions live in relationtext so lightweight binaries can render
// them without the stores; these names keep the module skills unchanged.
const (
	AgentContract     = relationtext.AgentContract
	AgentExample      = relationtext.AgentExample
	KnowledgeContract = relationtext.KnowledgeContract
	AgentMandate      = relationtext.AgentMandate
)

func QueryContract(queryTool, reconcileTool string) string {
	return relationtext.QueryContract(queryTool, reconcileTool)
}
