package ast

import (
	"os"
)

func ReadFileBytes(path string) ([]byte, error) {
	return os.ReadFile(path)
}
