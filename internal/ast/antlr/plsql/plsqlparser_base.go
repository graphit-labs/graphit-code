package plsql

import "github.com/antlr4-go/antlr/v4"

// PlSqlParserBase provides the base class required by PlSqlParser.g4.
// Port from Java: grammars-v4/sql/plsql/Java/PlSqlParserBase.java
//
// Method names match the Java original's casing exactly — the ANTLR4 Go
// target preserves Java method names in generated predicate calls:
//   - isVersion10/11/12 (lowercase 'i' — unexported but same-package)
//   - IsNotNumericFunction (uppercase 'I' — exported)
//   - isNotStartOfJoin (lowercase 'i' — unexported but same-package)
//
// Version flags default to true — accepts all Oracle syntax versions.
type PlSqlParserBase struct {
	*antlr.BaseParser
}

// isVersion12 returns true, accepting Oracle 12c+ syntax.
func (p *PlSqlParserBase) isVersion12() bool { return true }

// isVersion11 returns true, accepting Oracle 11g syntax.
func (p *PlSqlParserBase) isVersion11() bool { return true }

// isVersion10 returns true, accepting Oracle 10g syntax.
func (p *PlSqlParserBase) isVersion10() bool { return true }

// IsNotNumericFunction returns false when the next token is a numeric aggregate
// function (SUM, COUNT, AVG, MIN, MAX, ROUND, LEAST, GREATEST) followed by '('.
func (p *PlSqlParserBase) IsNotNumericFunction() bool {
	lt1 := p.GetTokenStream().LT(1)
	lt2 := p.GetTokenStream().LT(2)
	if lt1 == nil || lt2 == nil {
		return true
	}
	t := lt1.GetTokenType()
	if (t == PlSqlParserSUM ||
		t == PlSqlParserCOUNT ||
		t == PlSqlParserAVG ||
		t == PlSqlParserMIN ||
		t == PlSqlParserMAX ||
		t == PlSqlParserROUND ||
		t == PlSqlParserLEAST ||
		t == PlSqlParserGREATEST) &&
		lt2.GetTokenType() == PlSqlParserLEFT_PAREN {
		return false
	}
	return true
}

// isNotStartOfJoin returns false when the next token starts a join clause.
func (p *PlSqlParserBase) isNotStartOfJoin() bool {
	lt1 := p.GetTokenStream().LT(1)
	if lt1 == nil {
		return true
	}
	t := lt1.GetTokenType()
	return t != PlSqlParserINNER &&
		t != PlSqlParserCROSS &&
		t != PlSqlParserNATURAL &&
		t != PlSqlParserPARTITION &&
		t != PlSqlParserFULL &&
		t != PlSqlParserLEFT &&
		t != PlSqlParserRIGHT &&
		t != PlSqlParserOUTER
}
