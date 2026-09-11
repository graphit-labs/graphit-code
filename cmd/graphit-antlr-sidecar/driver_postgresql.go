//go:build grammar_postgresql

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/postgresql"
)

func init() {
	drivers["postgresql"] = &postgresql.Driver{}
}
