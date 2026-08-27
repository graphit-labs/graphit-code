package postgresql

import (
	"strings"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	tsPlpgsql "github.com/graphit-labs/graphit-code/internal/ast/treesitter/plpgsql"
	sitter "github.com/tree-sitter/go-tree-sitter"
)

// plpgsqlLang is the real PL/pgSQL tree-sitter grammar (github.com/gmr/tree-sitter-postgres,
// plpgsql/), vendored under internal/ast/treesitter/plpgsql. Its SQL grammar is
// code-generated from PostgreSQL's own Bison source; the plpgsql/ grammar is
// hand-written on top of it, and was verified here (see
// docs/tasks/postgres-plpgsql-embedding.md) against PERFORM, RAISE, RETURN
// QUERY and FOREACH — the constructs that broke the earlier attempt to reuse
// the PL/SQL parser for this.
var plpgsqlLang = sitter.NewLanguage(tsPlpgsql.Language())

// spliceCreateFunctionBodies finds every createfunc_opt_list this grammar
// parsed — the AS/LANGUAGE/... option list of a CREATE FUNCTION — and, when
// its LANGUAGE option says plpgsql, re-parses the dollar-quoted body with the
// real PL/pgSQL grammar instead of leaving it as one opaque string constant.
//
// This grammar is PostgreSQL's own SQL dialect, not PL/pgSQL — a dollar-
// quoted body is opaque to it by design, the same way it is opaque to
// PostgreSQL's own SQL parser: the body's language is a run-time property
// (LANGUAGE plpgsql / sql / plpython3u / ...), so the grammar that only knows
// SQL syntax cannot and should not try to parse it. Only `plpgsql` is
// resolved here; other LANGUAGE values are left as the opaque string they
// already were, same as before.
func spliceCreateFunctionBodies(root *antlrcommon.TreeNode) {
	if root == nil {
		return
	}
	var walk func(n *antlrcommon.TreeNode)
	walk = func(n *antlrcommon.TreeNode) {
		if n == nil {
			return
		}
		if n.Rule == "createfunc_opt_list" {
			spliceOptList(n)
		}
		for _, c := range n.Children {
			walk(c)
		}
	}
	walk(root)
}

// spliceOptList reads one CREATE FUNCTION's option list — AS $$...$$ and
// LANGUAGE xxx are siblings here, both createfunc_opt_item children — and
// splices the PL/pgSQL parse into the anysconst node if the language matches.
func spliceOptList(optList *antlrcommon.TreeNode) {
	var anysconst *antlrcommon.TreeNode
	var body, language string
	for _, item := range optList.Children {
		if item.Rule != "createfunc_opt_item" || len(item.Children) == 0 {
			continue
		}
		head := item.Children[0]
		switch head.Text {
		case "AS":
			if a := findRule(item, "anysconst"); a != nil {
				if b := dollarQuotedBody(a); b != "" {
					anysconst, body = a, b
				}
			}
		case "LANGUAGE":
			if len(item.Children) > 1 {
				language = strings.ToLower(strings.TrimSpace(leafText(item.Children[1])))
			}
		}
	}
	if anysconst == nil || body == "" || language != "plpgsql" {
		return
	}
	if sub := parsePlpgsql(body); sub != nil {
		anysconst.Children = append(anysconst.Children, sub)
	}
}

// findRule returns the first descendant (including n itself) with the given
// rule name, depth-first.
func findRule(n *antlrcommon.TreeNode, rule string) *antlrcommon.TreeNode {
	if n == nil {
		return nil
	}
	if n.Rule == rule {
		return n
	}
	for _, c := range n.Children {
		if r := findRule(c, rule); r != nil {
			return r
		}
	}
	return nil
}

// leafText concatenates every terminal's text under n, in order — used to
// read a value (e.g. the LANGUAGE clause's identifier) without caring how
// many rule layers wrap it.
func leafText(n *antlrcommon.TreeNode) string {
	if n == nil {
		return ""
	}
	if n.IsTerminal() {
		return n.Text
	}
	var b strings.Builder
	for _, c := range n.Children {
		b.WriteString(leafText(c))
	}
	return b.String()
}

// dollarQuotedBody returns the text between BeginDollarStringConstant and
// EndDollarStringConstant, or "" if n is not that alternative of anysconst
// (an ordinary '...' string, for instance).
func dollarQuotedBody(n *antlrcommon.TreeNode) string {
	if len(n.Children) == 0 || n.Children[0].Token != "BeginDollarStringConstant" {
		return ""
	}
	var b strings.Builder
	for _, c := range n.Children {
		if c.Token == "DollarText" {
			b.WriteString(c.Text)
		}
	}
	return b.String()
}

// parsePlpgsql parses body with the real PL/pgSQL tree-sitter grammar and
// converts the result into the shared antlrcommon.TreeNode shape, so the
// same complexityMatcher logic that walks a native ANTLR tree also walks
// this spliced-in subtree unchanged.
func parsePlpgsql(body string) *antlrcommon.TreeNode {
	p := sitter.NewParser()
	defer p.Close()
	if err := p.SetLanguage(plpgsqlLang); err != nil {
		return nil
	}
	src := []byte(body)
	tree := p.Parse(src, nil)
	defer tree.Close()
	root := tree.RootNode()
	if root == nil {
		return nil
	}
	return sitterToTreeNode(root, src)
}

// sitterToTreeNode converts a tree-sitter node into antlrcommon.TreeNode:
// a node with children becomes a Rule (its Kind() is the rule name), a leaf
// becomes a Token+Text terminal — the same distinction ANTLR's own
// TreeNode makes, which is what lets antlrComplexityMatcher.score walk
// either kind of tree without knowing which parser produced it.
func sitterToTreeNode(n *sitter.Node, src []byte) *antlrcommon.TreeNode {
	if n == nil {
		return nil
	}
	start := [2]int{int(n.StartPosition().Row) + 1, int(n.StartPosition().Column)}
	end := [2]int{int(n.EndPosition().Row) + 1, int(n.EndPosition().Column)}
	if n.ChildCount() == 0 {
		return &antlrcommon.TreeNode{Token: n.Kind(), Text: n.Utf8Text(src), Start: start, End: end}
	}
	tn := &antlrcommon.TreeNode{Rule: n.Kind(), Start: start, End: end}
	for i := 0; i < int(n.ChildCount()); i++ {
		if c := sitterToTreeNode(n.Child(uint(i)), src); c != nil {
			tn.Children = append(tn.Children, c)
		}
	}
	return tn
}
