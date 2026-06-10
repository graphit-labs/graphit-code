package main

import (
	"context"
	"fmt"

	sitter "github.com/smacker/go-tree-sitter"
	"github.com/smacker/go-tree-sitter/swift"
	tree_sitter_dart "github.com/graphit-labs/graphit-code/internal/ast/treesitter/dart"
)

func main() {
	// Swift
	{
		p := sitter.NewParser()
		p.SetLanguage(swift.GetLanguage())
		src := []byte(`
struct User { let name: String }
enum Direction { case north }
`)
		tree, _ := p.ParseCtx(context.Background(), nil, src)
		if tree != nil {
			fmt.Printf("Swift struct & enum: %s\n\n", tree.RootNode().String())
		}
	}

	// Dart
	{
		lang := tree_sitter_dart.GetLanguage()
		fmt.Printf("Sitter language wrapper: %v\n", lang)
		if lang == nil {
			fmt.Println("Sitter language is nil!")
			return
		}

		p := sitter.NewParser()
		p.SetLanguage(lang)
		src := []byte("void main() { print('hello'); }")
		tree, err := p.ParseCtx(context.Background(), nil, src)
		fmt.Printf("Dart parse err: %v, tree: %v\n", err, tree)
		if tree != nil && tree.RootNode() != nil {
			fmt.Printf("Dart AST: %s\n", tree.RootNode().String())
		} else {
			fmt.Println("Dart root node is nil!")
		}
	}
}
