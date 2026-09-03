package postgresql

import (
	"strings"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
	tsPlpgsql "github.com/graphit-labs/graphit-code/internal/ast/treesitter/plpgsql"
	sitter "github.com/tree-sitter/go-tree-sitter"
)

var plpgsqlLang = sitter.NewLanguage(tsPlpgsql.Language())

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
