package ast

const shardInternLimit = 1 << 20

const shardLocalInternLimit = 1 << 16

type shardInterner struct {
	values map[string]string
	limit  int
}

func newShardInterner(limit int) *shardInterner {
	return &shardInterner{values: make(map[string]string), limit: limit}
}

func (si *shardInterner) of(s string) string {
	if si == nil || s == "" {
		return s
	}
	if v, ok := si.values[s]; ok {
		return v
	}
	if len(si.values) >= si.limit {
		return s
	}
	si.values[s] = s
	return s
}

func clip[T any](s []T) []T {
	if len(s) == cap(s) {
		return s
	}
	if len(s) == 0 {
		return nil
	}
	out := make([]T, len(s))
	copy(out, s)
	return out
}

func (n *shardNodes) compact(shared, local *shardInterner) {
	if n == nil {
		return
	}
	n.Lang = shared.of(n.Lang)
	n.Cluster = shared.of(n.Cluster)
	n.DirPaths = clip(n.DirPaths)
	for i := range n.DirPaths {
		n.DirPaths[i] = shared.of(n.DirPaths[i])
	}

	n.Entities = clip(n.Entities)
	for i := range n.Entities {
		e := &n.Entities[i]
		e.Label = shared.of(e.Label)
		e.Path = shared.of(e.Path)
		e.Lang = shared.of(e.Lang)
		e.Context = shared.of(e.Context)
		e.ContextType = shared.of(e.ContextType)
		e.UID = local.of(e.UID)
		for d := range e.Decorators {
			e.Decorators[d] = shared.of(e.Decorators[d])
		}
	}

	n.Params = clip(n.Params)
	for i := range n.Params {
		p := &n.Params[i]
		p.Path = shared.of(p.Path)
		p.Lang = shared.of(p.Lang)
		p.FuncUID = local.of(p.FuncUID)
	}

	n.Fields = clip(n.Fields)
	for i := range n.Fields {
		f := &n.Fields[i]
		f.Path = shared.of(f.Path)
		f.Lang = shared.of(f.Lang)
		f.ParentType = shared.of(f.ParentType)
		f.ParentUID = local.of(f.ParentUID)
	}
}

func (e *shardEdges) compact(shared, local *shardInterner) {
	if e == nil {
		return
	}
	e.Calls = clip(e.Calls)
	for i := range e.Calls {
		c := &e.Calls[i]
		c.Path = shared.of(c.Path)
		c.Lang = shared.of(c.Lang)
		c.SourceType = shared.of(c.SourceType)
		c.ReceiverType = shared.of(c.ReceiverType)
		c.CallerUID = local.of(c.CallerUID)
		c.CalleeUID = shared.of(c.CalleeUID)
	}

	e.Imports = clip(e.Imports)
	for i := range e.Imports {
		im := &e.Imports[i]
		im.Lang = shared.of(im.Lang)
		im.SourceFile = shared.of(im.SourceFile)
		im.ModuleName = shared.of(im.ModuleName)
		im.ModuleUID = shared.of(im.ModuleUID)
		im.RawImport = shared.of(im.RawImport)
		im.FileUID = local.of(im.FileUID)
	}

	e.Inheritance = clip(e.Inheritance)
	for i := range e.Inheritance {
		in := &e.Inheritance[i]
		in.Path = shared.of(in.Path)
		in.RelType = shared.of(in.RelType)
		in.ChildUID = local.of(in.ChildUID)
		in.ParentUID = shared.of(in.ParentUID)
	}

	e.FieldAccess = clip(e.FieldAccess)
	for i := range e.FieldAccess {
		fa := &e.FieldAccess[i]
		fa.Path = shared.of(fa.Path)
		fa.SourceUID = local.of(fa.SourceUID)
		fa.FieldUID = shared.of(fa.FieldUID)
	}

	e.References = clip(e.References)
	for i := range e.References {
		r := &e.References[i]
		r.Path = shared.of(r.Path)
		r.Lang = shared.of(r.Lang)
		r.RelType = shared.of(r.RelType)
		r.TargetUID = shared.of(r.TargetUID)
		r.SourceUID = local.of(r.SourceUID)
	}

	e.Contains = clip(e.Contains)
	for i := range e.Contains {
		c := &e.Contains[i]
		c.ParentLabel = shared.of(c.ParentLabel)
		c.ChildLabel = shared.of(c.ChildLabel)
		c.ParentUID = local.of(c.ParentUID)
		c.ChildUID = local.of(c.ChildUID)
	}
}
