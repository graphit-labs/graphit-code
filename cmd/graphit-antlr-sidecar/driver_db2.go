//go:build grammar_db2

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/db2"
)

func init() {
	drivers["db2"] = &db2.Driver{}
}
