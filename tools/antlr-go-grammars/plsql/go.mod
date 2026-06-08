module github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/plsql

go 1.25.11

require (
	github.com/antlr4-go/antlr/v4 v4.13.1
	github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared v0.0.0
)

require golang.org/x/exp v0.0.0-20240506185415-9bf2ced13842 // indirect

replace github.com/graphit-labs/graphit-code/tools/antlr-go-grammars/shared => ../shared
