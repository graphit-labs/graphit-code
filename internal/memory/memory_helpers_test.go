package memory

import (
	"strings"
)

func firstLineFromContent(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed != "" && !strings.HasPrefix(trimmed, "#") {
			if len(trimmed) > 100 {
				return trimmed[:100] + "…"
			}
			return trimmed
		}
	}
	return ""
}
