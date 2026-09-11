//go:build !grammar_plsql && !grammar_postgresql && !grammar_tsql && !grammar_db2 && !grammar_cobol85

package main

import (
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/cobol85"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/db2"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/plsql"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/postgresql"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/tsql"
)

func init() {
	drivers["plsql"] = &plsql.Driver{}
	drivers["postgresql"] = &postgresql.Driver{}
	drivers["tsql"] = &tsql.Driver{}
	drivers["db2"] = &db2.Driver{}
	drivers["cobol85"] = &cobol85.Driver{}
}
