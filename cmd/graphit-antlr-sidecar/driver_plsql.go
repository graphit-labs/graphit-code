//go:build grammar_plsql

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/plsql"
)

func init() {
	drivers["plsql"] = &plsql.Driver{}
}
