package hub

import (
	"strings"
)

func TruncateHash(hash string, n int) string {
	if len(hash) <= n {
		return hash
	}
	return hash[:n]
}

func VerifyHash(path, expectedHash string) (bool, error) {
	if expectedHash == "" {
		return true, nil
	}
	actual, err := HashPath(path)
	if err != nil {
		return false, err
	}
	return strings.EqualFold(actual, expectedHash), nil
}
