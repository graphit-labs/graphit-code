//go:build grammar_tsql

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/tsql"
)

func init() {
	drivers["tsql"] = &tsql.Driver{}
}
