/*
PostgreSQL grammar.
The MIT License (MIT).
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
*/

package postgresql

import (
	"strings"

	"github.com/antlr4-go/antlr/v4"
)

type PostgreSQLParserBase struct {
	*antlr.BaseParser
}

func NewPostgreSQLParserBase(input antlr.TokenStream) *PostgreSQLParserBase {
	return &PostgreSQLParserBase{
		BaseParser: antlr.NewBaseParser(input),
	}
}

func (receiver *PostgreSQLParserBase) ParseRoutineBody() {
}

func TrimQuotes(s string) string {
	if s == "" {
		return s
	}
	return s[1 : len(s)-2]
}

func unquote(s string) string {
	result := strings.Builder{}
	length := len(s)
	index := 0
	for index < length {
		c := s[index]
		result.WriteByte(c)
		if c == '\'' && index < length-1 && (s[index+1] == '\'') {
			index++
		}
		index++
	}
	return result.String()
}

func GetRoutineBodyString(rule *SconstContext) string {
	if rule.Anysconst() == nil {
		return ""
	}
	anySConstContext := rule.Anysconst().(*AnysconstContext)

	stringConstant := anySConstContext.StringConstant()
	if stringConstant != nil {
		return unquote(TrimQuotes(stringConstant.GetText()))
	}

	unicodeEscapeStringConstant := anySConstContext.UnicodeEscapeStringConstant()
	if unicodeEscapeStringConstant != nil {
		return TrimQuotes(unicodeEscapeStringConstant.GetText())
	}

	escapeStringConstant := anySConstContext.EscapeStringConstant()
	if escapeStringConstant != nil {
		return TrimQuotes(escapeStringConstant.GetText())
	}

	result := strings.Builder{}
	for _, node := range anySConstContext.AllDollarText() {
		result.WriteString(node.GetText())
	}
	return result.String()
}

func (p *PostgreSQLParserBase) OnlyAcceptableOps() bool {
	stream := p.GetTokenStream()
	c := stream.LT(1)
	text := c.GetText()
	return text == "!" || text == "!!" || text == "!=-"
}
