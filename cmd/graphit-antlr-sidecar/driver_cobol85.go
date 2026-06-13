//go:build grammar_cobol85

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/cobol85"
)

func init() {
	drivers["cobol85"] = &cobol85.Driver{}
}
