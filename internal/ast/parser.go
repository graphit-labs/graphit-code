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

	// mergeIdx locates entities by identity for AddOrMergeEntity. Unexported, so it
	// never reaches the shard cache.
	mergeIdx map[string]*entityIndex
}

type Entity struct {
	Name    string
	Line    int
	EndLine int
	// ModifierExport is the verdict of the modifier-based export strategies
	// ("modifier", "no_modifier", "no_static"), decided when the entity is built
	// and its body text is already in hand.
	//
	// It replaces a Source field that held a full copy of every entity's body —
	// overlapping copies for nested entities — purely so isExported could run a
	// substring check later. That text was never persisted (cachedEntity has no
	// source field), so retaining it was pure overhead.
	ModifierExport bool
	Docstring      string
	Args           []string
	Context        string
	ContextType    string
	GraphLabel     string
	Complexity     int
	Properties     map[string]string
	// Lang is the language that PRODUCED this entity, which is the file's own only
	// until an embedded block is parsed. Empty means "the file's" — see langOr.
	Lang string
}

type CallInfo struct {
	Name         string
	Line         int
	Args         []string
	FullName     string
	SourceName   string
	SourceType   string
	ReceiverType string
	// Lang: see Entity.Lang. Name resolution refuses to cross languages, so a call
	// carrying the host's language instead of its own resolves against the wrong
	// declarations — that is what left embedded SQL pointing at nothing.
	Lang string
}

type ReferenceInfo struct {
	TargetName string
	RelType    string
	Line       int
	SourceName string
	// Lang: see Entity.Lang. It also picks the TargetRule, so a reference from an
	// embedded block is resolved by the rulebook of the grammar that parsed it.
	Lang string
}

func (pf *ParsedFile) AddEntity(dataKey string, e Entity) {
	if pf.Entities == nil {
		pf.Entities = make(map[string][]Entity)
	}
	pf.Entities[dataKey] = append(pf.Entities[dataKey], e)
	// A later AddOrMergeEntity has to see this, or it appends a duplicate of it.
	if idx := pf.mergeIdx[dataKey]; idx != nil {
		id := identityOf(e)
		if _, seen := idx.pos[id]; !seen {
			idx.pos[id] = len(pf.Entities[dataKey]) - 1
		}
		idx.slen = len(pf.Entities[dataKey])
	}
}

// AddOrMergeEntity records an entity, or completes one already recorded.
//
// Two queries legitimately describe the same node: one matches a declaration for
// its own sake, another matches it again to reach the value it declares, because
// no single pattern can say "the name, and the value if there is one". Appending
// both left three Oracle columns as six Column entities and two Table→Column
// edges as four.
//
// Identity is label, name, context and line together. Name and line alone would
// merge the two 1s of `{"a": 1, "b": 1}`, which are values of different keys.
func (pf *ParsedFile) AddOrMergeEntity(dataKey string, e Entity) {
	if pf.Entities == nil {
		pf.Entities = make(map[string][]Entity)
	}
	idx := pf.entityIndexFor(dataKey)
	id := identityOf(e)
	if pos, ok := idx.pos[id]; ok && pos < len(pf.Entities[dataKey]) &&
		identityOf(pf.Entities[dataKey][pos]) == id {
		mergeEntityInto(&pf.Entities[dataKey][pos], e)
		return
	}

	pf.Entities[dataKey] = append(pf.Entities[dataKey], e)
	idx.slen = len(pf.Entities[dataKey])
	if _, seen := idx.pos[id]; !seen {
		idx.pos[id] = idx.slen - 1
	}
}

// mergeEntityInto completes an entity already recorded with what the second match
// brought and the first lacked.
func mergeEntityInto(existing *Entity, e Entity) {
	if existing.Docstring == "" {
		existing.Docstring = e.Docstring
	}
	if len(existing.Args) == 0 {
		existing.Args = e.Args
	}
	if existing.ContextType == "" {
		existing.ContextType = e.ContextType
	}
	for k, v := range e.Properties {
		if existing.Properties == nil {
			existing.Properties = make(map[string]string, len(e.Properties))
		}
		if existing.Properties[k] == "" {
			existing.Properties[k] = v
		}
	}
}

// entityIdentity is what AddOrMergeEntity treats as one node.
type entityIdentity struct {
	label   string
	name    string
	context string
	line    int
}

func identityOf(e Entity) entityIdentity {
	return entityIdentity{e.GraphLabel, e.Name, e.Context, e.Line}
}

// entityIndex locates an identity in ParsedFile.Entities[dataKey].
//
// slen is the slice length the index was built for. This file is not the only
// writer -- adapters take pointers into the slice and rewrite fields in place in
// later passes, and callers build a ParsedFile with Entities already populated --
// so a length that no longer matches means rebuild rather than trust.
type entityIndex struct {
	pos  map[entityIdentity]int
	slen int
}

// entityIndexFor returns the index for dataKey, building it when missing or stale.
//
// It exists because AddOrMergeEntity used to scan every entity already recorded
// under the same key, which is quadratic in entities per file. That stayed
// invisible until value nodes made every literal an entity: on a 114k-line XML
// file one worker sat inside that scan long after every other worker had
// finished, with no disk I/O and no subprocess alive, and an index of 36k files
// looked hung with no way to tell it from idle.
func (pf *ParsedFile) entityIndexFor(dataKey string) *entityIndex {
	if pf.mergeIdx == nil {
		pf.mergeIdx = make(map[string]*entityIndex)
	}
	list := pf.Entities[dataKey]
	idx := pf.mergeIdx[dataKey]
	if idx == nil {
		idx = &entityIndex{pos: make(map[entityIdentity]int, len(list)+8)}
		pf.mergeIdx[dataKey] = idx
	}
	if idx.slen == len(list) {
		return idx
	}
	// Rebuilt whole rather than patched: the first occurrence has to win, and a
	// patch cannot know which that is. O(n) once per divergence, against O(n) on
	// every insert before.
	clear(idx.pos)
	for i := range list {
		id := identityOf(list[i])
		if _, seen := idx.pos[id]; !seen {
			idx.pos[id] = i
		}
	}
	idx.slen = len(list)
	return idx
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
	// IndexSource mirrors PipelineOptions.IndexSource. When false the parsers
	// skip materialising ParsedFile.Source, a full copy of every file that only
	// ConvertToCache consumes — and only when source indexing is on.
	IndexSource bool

	// Cancelled, when set, is polled from inside the parse and reports that the
	// caller has given up. A context cannot be honoured any other way here: the
	// expensive work happens inside cgo, which Go cannot preempt, so the parse
	// has to be asked to stop rather than told. tree-sitter provides exactly
	// this hook on both the parser and the query cursor.
	//
	// Called from a C callback on the parsing goroutine, frequently. Keep it to
	// an atomic load; do not allocate, log or take a lock in it.
	Cancelled func() bool
}
