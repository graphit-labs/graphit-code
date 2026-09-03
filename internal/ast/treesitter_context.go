package ast

import (
	"strconv"
	"strings"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

type contextSpec struct {
	label string
	alts  []namePath
}

type namePath struct {
	segs []string
	ids  []uint16
	// idx is the occurrence a segment selects among same-kind children, written
	// `kind[n]` and zero-based. -1 means "the first", which is what an unindexed
	// segment means.
	//
	// Needed because a name is not always the first child of its kind: in hcl's
	// `resource "aws_s3_bucket" "logs"` the NAME is the second string_lit — the first is
	// the type, which is deliberately not an entity, so a context named after it pointed
	// every CONTAINS edge at a node that never gets created.
	idx []int
}

type ctxHit struct {
	name    string
	label   string
	nameID  uintptr
	ctxNode *sitter.Node
	found   bool
}

type contextResolver struct {
	byID     map[uint16]contextSpec
	byName   map[string]contextSpec
	anon     kindMatcher
	anonOn   bool
	src      []byte
	memo     map[uintptr]ctxHit
	scratch  []uintptr
	resolved map[uintptr][2]string
}

func newContextResolver(lang *sitter.Language, langConfig *ExternalQueryFile, src []byte) *contextResolver {
	types := defaultContextTypes
	anon := defaultAnonFuncTypes
	var namePaths map[string]string

	if langConfig != nil {
		if langConfig.ContextTypes != nil {
			types = langConfig.ContextTypes
		}
		if langConfig.AnonFuncTypes != nil {
			anon = make(map[string]bool, len(langConfig.AnonFuncTypes))
			for _, t := range langConfig.AnonFuncTypes {
				anon[t] = true
			}
		}
		namePaths = langConfig.ContextNamePaths
	}

	r := &contextResolver{
		src:      src,
		memo:     make(map[uintptr]ctxHit),
		resolved: make(map[uintptr][2]string),
	}
	if len(anon) > 0 {
		r.anon = newKindMatcher(lang, anon)
		r.anonOn = true
	}
	for kind, label := range types {
		spec := contextSpec{label: label}
		for _, alt := range strings.Split(namePaths[kind], "|") {
			alt = strings.TrimSpace(alt)
			if alt == "" {
				continue
			}
			raw := strings.Split(alt, "/")
			np := namePath{
				segs: make([]string, len(raw)),
				ids:  make([]uint16, len(raw)),
				idx:  make([]int, len(raw)),
			}
			for i, seg := range raw {
				np.segs[i], np.idx[i] = parsePathSegment(seg)
				if lang != nil {
					np.ids[i] = lang.IdForNodeKind(np.segs[i], true)
				}
			}
			spec.alts = append(spec.alts, np)
		}
		if lang != nil {
			if id := lang.IdForNodeKind(kind, true); id != 0 {
				if r.byID == nil {
					r.byID = make(map[uint16]contextSpec, len(types))
				}
				r.byID[id] = spec
				continue
			}
		}
		if r.byName == nil {
			r.byName = make(map[string]contextSpec, len(types))
		}
		r.byName[kind] = spec
	}
	return r
}

func (r *contextResolver) specFor(n *sitter.Node) (contextSpec, bool) {
	if r.byID != nil {
		if spec, ok := r.byID[n.KindId()]; ok {
			return spec, true
		}
	}
	if r.byName == nil {
		return contextSpec{}, false
	}
	spec, ok := r.byName[n.Kind()]
	return spec, ok
}

func (r *contextResolver) nameNodeOf(n *sitter.Node, spec contextSpec) *sitter.Node {
	if len(spec.alts) == 0 {
		return SafeChildByFieldName(n, "name")
	}
	for _, np := range spec.alts {
		if node := resolveNamePath(n, np); !SafeIsNull(node) {
			return node
		}
	}
	return nil
}

func resolveNamePath(n *sitter.Node, np namePath) *sitter.Node {
	cur := n
	for i, seg := range np.segs {
		next := childBySegment(cur, seg, np.ids[i], np.idx[i])
		if SafeIsNull(next) {
			return nil
		}
		cur = next
	}
	return cur
}

func parsePathSegment(seg string) (string, int) {
	open := strings.IndexByte(seg, '[')
	if open <= 0 || !strings.HasSuffix(seg, "]") {
		return seg, -1
	}
	idx, err := strconv.Atoi(seg[open+1 : len(seg)-1])
	if err != nil || idx < 0 {
		return seg, -1
	}
	return seg[:open], idx
}

func childBySegment(n *sitter.Node, seg string, segID uint16, idx int) *sitter.Node {
	if idx < 0 {
		if c := SafeChildByFieldName(n, seg); !SafeIsNull(c) {
			return c
		}
	}
	want := idx
	if want < 0 {
		want = 0
	}
	seen := 0
	count := n.NamedChildCount()
	for i := uint(0); i < count; i++ {
		c := n.NamedChild(i)
		if SafeIsNull(c) {
			continue
		}
		if segID != 0 {
			if c.KindId() != segID {
				continue
			}
		} else if c.Kind() != seg {
			continue
		}
		if seen == want {
			return c
		}
		seen++
	}
	return nil
}

func (r *contextResolver) from(start *sitter.Node) ctxHit {
	pending := r.scratch[:0]
	defer func() { r.scratch = pending }()
	cur := SafeParent(start)
	for !SafeIsNull(cur) {
		if spec, ok := r.specFor(cur); ok {
			if nameNode := r.nameNodeOf(cur, spec); !SafeIsNull(nameNode) {
				hit := ctxHit{
					name:    nameNode.Utf8Text(r.src),
					label:   spec.label,
					nameID:  nameNode.Id(),
					ctxNode: cur,
					found:   true,
				}
				r.fill(pending, hit)
				return hit
			}
		}
		if hit, ok := r.anonHit(cur); ok {
			r.fill(pending, hit)
			return hit
		}

		id := cur.Id()
		if hit, ok := r.memo[id]; ok {
			r.fill(pending, hit)
			return hit
		}
		pending = append(pending, id)
		cur = SafeParent(cur)
	}
	r.fill(pending, ctxHit{})
	return ctxHit{}
}

func (r *contextResolver) anonHit(cur *sitter.Node) (ctxHit, bool) {
	if !r.anonOn || !r.anon.match(cur) {
		return ctxHit{}, false
	}
	grandparent := SafeParent(cur)
	if SafeIsNull(grandparent) || SafeType(grandparent) != "variable_declarator" {
		return ctxHit{}, false
	}
	nameNode := SafeChildByFieldName(grandparent, "name")
	if SafeIsNull(nameNode) {
		return ctxHit{}, false
	}
	return ctxHit{
		name:    nameNode.Utf8Text(r.src),
		label:   "Function",
		nameID:  nameNode.Id(),
		ctxNode: grandparent,
		found:   true,
	}, true
}

func (r *contextResolver) fill(ids []uintptr, hit ctxHit) {
	for _, id := range ids {
		r.memo[id] = hit
	}
}

func (r *contextResolver) resolve(node *sitter.Node) (string, string) {
	if node == nil {
		return "", ""
	}
	id := node.Id()
	if got, ok := r.resolved[id]; ok {
		return got[0], got[1]
	}
	hit := r.from(node)
	if hit.found && hit.nameID == node.Id() && hit.ctxNode != nil {
		hit = r.from(hit.ctxNode)
	}
	if !hit.found {
		r.resolved[id] = [2]string{}
		return "", ""
	}
	r.resolved[id] = [2]string{hit.name, hit.label}
	return hit.name, hit.label
}
