package ast

type parseCacheEntry struct {
	RelPath  string
	Language string
	IsDepend bool
	Source   string

	FileRow       []string
	DirPaths      []string
	Entities      []cachedEntity
	Calls         []cachedCall
	Imports       []cachedImport
	Inheritance   []cachedInheritance
	FieldAccess   []cachedFieldAccess
	References    []cachedReference
	Parameters    []cachedParameter
	Fields        []cachedField
	ContainsEdges []cachedContainsEdge
}

type cachedEntity struct {
	Label       string
	UID         string
	Name        string
	Path        string
	Line        int
	EndLine     int
	Docstring   string
	Lang        string
	Complexity  int
	Context     string
	ContextType string
	IsDep       bool
	IsExported  bool
	Decorators  []string
	Args        []string
	Value       string
}

type cachedCall struct {
	CallerUID    string
	CalleeUID    string
	SourceType   string
	Line         int
	Path         string
	ReceiverType string
}

type cachedImport struct {
	FileUID      string
	ModuleUID    string
	ModuleName   string
	RawImport    string
	Alias        string
	ImportedName string
	Line         int
	Lang         string
	SourceFile   string
}

type cachedInheritance struct {
	ChildUID  string
	ParentUID string
	RelType   string
	Path      string
	Line      int
}

type cachedFieldAccess struct {
	SourceUID string
	FieldUID  string
	IsWrite   bool
	Path      string
	Line      int
}

type cachedReference struct {
	SourceUID string
	TargetUID string
	RelType   string
	Path      string
	Line      int
}

type cachedParameter struct {
	UID     string
	Name    string
	FuncUID string
	Path    string
	Line    int
	Lang    string
}

type cachedField struct {
	UID        string
	Name       string
	ParentUID  string
	ParentType string
	Path       string
	Line       int
	Lang       string
}

type cachedContainsEdge struct {
	ParentUID   string
	ChildUID    string
	ParentLabel string
	ChildLabel  string
}
