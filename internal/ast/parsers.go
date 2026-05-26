package ast

import (
	"os"
	"strings"
)

func ReadFileBytes(path string) ([]byte, error) {
	return os.ReadFile(path)
}

func Truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max]
}

func ComputeCyclomaticComplexity(source string) int {
	cc := 1

	branchKeywords := []string{
		" if ", " else ", " elif ", " elsif ", " elseif ",
		" for ", " while ", " foreach ",
		" case ", " when ",
		" catch ", " except ", " rescue ",
		" && ", " || ",
		"? ",
	}

	lower := " " + strings.ToLower(source) + " "
	for _, kw := range branchKeywords {
		cc += strings.Count(lower, kw)
	}

	return cc
}
