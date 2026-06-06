package ast

type ParsedFile struct {
	Path     string
	RepoPath string
	Language string
	Parser   string // "tree-sitter" or "antlr4"
	IsDepend bool
	HasError bool

	Source string

	Entities map[string][]Entity

	CallSites []CallInfo

	References []ReferenceInfo
}

type Entity struct {
	Name        string
	Line        int
	EndLine     int
	Source      string
	Docstring   string
	Args        []string
	Context     string
	ContextType string
	GraphLabel  string
	Complexity  int
	Properties  map[string]string
}

type CallInfo struct {
	Name         string
	Line         int
	Args         []string
	FullName     string
	SourceName   string
	SourceType   string
	ReceiverType string
}

type ReferenceInfo struct {
	TargetName string
	RelType    string
	Line       int
	SourceName string
}

func (pf *ParsedFile) AddEntity(dataKey string, e Entity) {
	if pf.Entities == nil {
		pf.Entities = make(map[string][]Entity)
	}
	pf.Entities[dataKey] = append(pf.Entities[dataKey], e)
}

func (pf *ParsedFile) GetEntities(dataKey string) []Entity {
	if pf.Entities == nil {
		return nil
	}
	return pf.Entities[dataKey]
}

func (pf *ParsedFile) AllEntities() []Entity {
	var all []Entity
	for _, entities := range pf.Entities {
		all = append(all, entities...)
	}
	return all
}

func (pf *ParsedFile) EntityCount() int {
	n := 0
	for _, entities := range pf.Entities {
		n += len(entities)
	}
	return n
}

type LanguageParser interface {
	Parse(path string, isDepend bool, opts ParseOptions) (*ParsedFile, error)
}

type ParseOptions struct {
}

type BatchParser interface {
	ParseBatch(files []BatchFileInput, opts ParseOptions, resultCh chan<- BatchResult)
}

type BatchFileInput struct {
	Path     string
	Content  string
	IsDepend bool
}

type BatchResult struct {
	File *ParsedFile
	Err  error
}
