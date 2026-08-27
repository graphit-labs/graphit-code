package ast

import (
	"strconv"
	"strings"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

// Containment for tree-sitter entities: which ancestor an entity belongs to, and
// what that ancestor is called.
//
// Two problems are solved here, and they pull in opposite directions.
//
// CORRECTNESS. resolveParentContextTS named an ancestor by reading its "name"
// field. That works for programming languages, where a declaration carries its
// identifier in a field, and not at all for data formats: tree-sitter-xml's
// `element`, tree-sitter-json's `pair`, tree-sitter-html's `element` and
// tree-sitter-yaml's `block_mapping_pair` expose no "name" field, so every
// lookup failed and every entity fell back to being contained by the File. A
// context therefore also declares a PATH to its name (context_name_paths),
// resolved segment by segment as either a field name or a child kind.
//
// COST. The walk is one cgo call per ancestor per entity, and sibling entities
// under one ancestor repeat the identical walk. On a 47 MB XML export that was
// 74% of the whole parse — 36.5 s of 49 s, effectively all of it inside
// ts_node_parent — because xml.yaml declared no contexts, so the walk fell back
// to defaultContextTypes (class_declaration, function_definition, …), which
// cannot match an XML tree, and ran to the document root 1.6 million times.
// Results are therefore memoised per node: the answer for a node is the answer
// for its parent unless the parent is itself a context, so one walk serves every
// entity beneath it.

// contextSpec says that a node kind acts as a container, and how to find the
// name it is known by.
type contextSpec struct {
	label string
	// alts are the ways to reach this container's name, tried in order until one
	// resolves. Empty means the "name" field, which is the programming-language default.
	//
	// More than one is needed because a container's name is not always in the same place
	// for every instance of it. toml keys a table with either a bare_key or a
	// dotted_key; hcl writes `resource "type" "name"` with two labels and
	// `variable "name"` with one, so the name is the second string_lit in one case and
	// the first in the other.
	alts []namePath
}

// namePath is one route from a container to the node holding its name, segment by
// segment, each resolved as a field name or as a child kind.
type namePath struct {
	segs []string
	// ids are the segments resolved to node-kind ids where the grammar knows them; 0
	// means "match this segment by string". Field names are not resolvable to a kind id.
	ids []uint16
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

// ctxHit is the container found above some node, or found == false for none.
type ctxHit struct {
	name  string
	label string
	// nameID identifies the node the name was read from. An entity whose own
	// capture IS that node has found itself, not its parent — see resolve.
	nameID  uintptr
	ctxNode *sitter.Node
	found   bool
}

// contextResolver answers "which container is this node in" for one parsed file.
// Not safe for concurrent use: one is built per parse.
type contextResolver struct {
	byID   map[uint16]contextSpec
	byName map[string]contextSpec
	// anon is matched by kind id: Node.Kind() allocates a Go string through
	// C.GoString, and this is tested on every ancestor of every entity.
	anon   kindMatcher
	anonOn bool
	src    []byte
	memo   map[uintptr]ctxHit
	// scratch is reused across calls: from() is invoked once per entity, and a
	// fresh slice per call is a million allocations on a large file.
	scratch []uintptr
	// resolved caches the final answer per capture node. Distinct queries capture
	// the SAME node — xml.yaml reaches an element's Name through both `elements`
	// and `element_text` — so a file with 600k elements asks 1.8M times. The memo
	// above only covers interior nodes; the walk from the capture leaf itself was
	// redone on every ask.
	resolved map[uintptr][2]string
}

func newContextResolver(lang *sitter.Language, langConfig *ExternalQueryFile, src []byte) *contextResolver {
	types := defaultContextTypes
	anon := defaultAnonFuncTypes
	var namePaths map[string]string

	if langConfig != nil {
		// A grammar that declares context_types is describing its own tree, so
		// it REPLACES the defaults rather than adding to them — including when
		// it declares none. The defaults are a fallback for a query file that
		// did not opine, not a floor under one that did.
		if langConfig.ContextTypes != nil {
			types = langConfig.ContextTypes
		}
		// Declared empty means empty, for the same reason as context_types: a
		// grammar that lists no anonymous-function kinds is describing its tree.
		// Falling back to the defaults here cost a Node.Kind() -- a cgo call AND
		// a Go string allocation -- on every ancestor of every entity, to test
		// membership in a set of JavaScript node kinds that a data format cannot
		// contain. Measured at 17.4s of a 50s parse.
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
		// Matching by kind id avoids Node.Kind(), which allocates a Go string
		// through C.GoString on every ancestor of every entity.
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

// specFor reports whether n is a container, and how to name it.
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

// nameNodeOf walks a container's name path. An unresolvable segment yields no
// name, which makes the container transparent: the walk continues above it
// rather than attaching the entity to something unnamed.
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

// parsePathSegment splits a name-path segment into its kind and the occurrence it
// selects: "string_lit[1]" is the second string_lit child. An unindexed segment yields
// -1, meaning the first match, which is what every segment meant before indices existed.
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

// childBySegment resolves one path segment: a field name if the grammar has one,
// otherwise the named child of that kind at the requested occurrence.
//
// An index is only meaningful for the kind lookup. A field holds one node by
// definition, so an indexed segment skips the field branch — otherwise `string_lit[1]`
// would silently return the field's single node and ignore the index.
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

// from returns the nearest container strictly above start.
//
// Memoised on every node passed through, so the ancestors shared by sibling
// entities are walked once for the file rather than once per entity.
func (r *contextResolver) from(start *sitter.Node) ctxHit {
	pending := r.scratch[:0]
	defer func() { r.scratch = pending }()
	cur := SafeParent(start)
	for !SafeIsNull(cur) {
		// Checked before the memo: memo[id] holds the container above that node,
		// which is not the same answer as "that node is itself the container".
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

// anonHit preserves the pre-existing rule that an anonymous function assigned to
// a variable is named after that variable.
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

// resolve names the container of the entity captured at node.
//
// The container found first may be the entity's OWN declaration: an XML element
// captured through `(STag (Name) @name)` sits inside the very `element` that
// names it, and a Go method's name field sits inside the `method_declaration`
// that context_types calls a Method. Naming an entity after itself is not
// containment, so when the name resolves back to the capture, the search
// resumes above that container.
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
