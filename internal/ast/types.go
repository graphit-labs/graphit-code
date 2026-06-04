package ast

import "context"

type NodeRecord struct {
	ID         string
	Labels     []string
	Properties map[string]any
}

type RelRecord struct {
	Type       string
	SrcID      string
	DstID      string
	Properties map[string]any
}

type QueryRecord map[string]any

type QueryResult struct {
	Records []QueryRecord

	NodesCreated         int
	RelationshipsCreated int
	PropertiesSet        int
}

type GraphDB interface {
	Query(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error)

	Execute(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error)

	ExecuteBatch(ctx context.Context, queries []BatchQuery) error

	Ping(ctx context.Context) error

	BackendType() string

	Close() error
}

type Releaser interface {
	Release()
}

type BatchQuery struct {
	Cypher string
	Params map[string]any
}

const (
	LabelFile       = "File"
	LabelDirectory  = "Directory"
	LabelModule     = "Module"
	LabelFunction   = "Function"
	LabelClass      = "Class"
	LabelVariable   = "Variable"
	LabelTrait      = "Trait"
	LabelInterface  = "Interface"
	LabelMacro      = "Macro"
	LabelStruct     = "Struct"
	LabelEnum       = "Enum"
	LabelUnion      = "Union"
	LabelAnnotation = "Annotation"
	LabelRecord     = "Record"
	LabelProperty   = "Property"
	LabelParameter  = "Parameter"
	LabelField      = "Field"

	LabelTable            = "Table"
	LabelView             = "View"
	LabelProcedure        = "Procedure"
	LabelPackage          = "Package"
	LabelTrigger          = "Trigger"
	LabelIndex            = "Index"
	LabelSequence         = "Sequence"
	LabelType             = "Type"
	LabelSynonym          = "Synonym"
	LabelConstant         = "Constant"
	LabelCursor           = "Cursor"
	LabelException        = "Exception"
	LabelNamespace        = "Namespace"
	LabelExport           = "Export"
	LabelDelegate         = "Delegate"
	LabelComment          = "Comment"
	LabelConstraint       = "Constraint"
	LabelMaterializedView = "MaterializedView"
	LabelDatabaseLink     = "DatabaseLink"
	LabelColumn           = "Column"
)

const (
	RelContains   = "CONTAINS"
	RelCalls      = "CALLS"
	RelImports    = "IMPORTS"
	RelInherits   = "INHERITS"
	RelHasParam   = "HAS_PARAMETER"
	RelIncludes   = "INCLUDES"
	RelImplements = "IMPLEMENTS"

	RelHasField    = "HAS_FIELD"
	RelReadsField  = "READS_FIELD"
	RelWritesField = "WRITES_FIELD"

	RelSelects = "SELECTS"
	RelInserts = "INSERTS"
	RelUpdates = "UPDATES"
	RelDeletes = "DELETES"

	RelCreates = "CREATES"
	RelAlters  = "ALTERS"
	RelDrops   = "DROPS"

	RelReferences = "REFERENCES"
)
