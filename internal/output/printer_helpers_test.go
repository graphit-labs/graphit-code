package output

import (
	"fmt"
	"io"
	"strings"
)

func (p *Printer) WithWriter(w io.Writer) *Printer {
	return &Printer{prefix: p.prefix, w: w}
}

func (p *Printer) StepError(msg string, args ...any) {
	line := fmt.Sprintf(msg, args...)
	_, _ = fmt.Fprintln(p.w, "  "+dim.Sprint(SymbolStep)+" "+red.Sprint(SymbolError+" "+line))
}

func (p *Printer) Divider() {
	_, _ = fmt.Fprintln(p.w, dim.Sprint(strings.Repeat(SymbolDivider, 40)))
}

func IsTTY() bool { return isTTY }
