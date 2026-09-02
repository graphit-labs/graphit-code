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

// memoryEntityPageWithHash renders the page of a LIVE memory.
//
// It is the old signature of memoryEntityPage, kept for the tests that predate the revision
// chain. Production code passes a memDoc, because a page now also has to be able to describe a
// superseded revision — which needs the chain fields this helper deliberately leaves empty.
