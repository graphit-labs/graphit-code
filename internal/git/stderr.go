package git

import "strings"

func CleanStderr(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "(no stderr output)"
	}
	var meaningful []string
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		if IsProgressLine(line) {
			continue
		}
		meaningful = append(meaningful, line)
	}
	if len(meaningful) == 0 {

		lines := strings.Split(raw, "\n")
		last := strings.TrimSpace(lines[len(lines)-1])
		if last != "" {
			return last
		}
		return "(git returned no useful error details)"
	}

	if len(meaningful) > 3 {
		meaningful = meaningful[len(meaningful)-3:]
	}
	return strings.Join(meaningful, "; ")
}

func IsProgressLine(line string) bool {
	progressKeywords := []string{
		"Counting objects:",
		"Compressing objects:",
		"Receiving objects:",
		"Resolving deltas:",
		"remote: Counting",
		"remote: Compressing",
		"remote: Total",
	}
	for _, kw := range progressKeywords {
		if strings.Contains(line, kw) {
			return true
		}
	}
	return false
}

func MapToEnv(m map[string]string) []string {
	s := make([]string, 0, len(m))
	for k, v := range m {
		s = append(s, k+"="+v)
	}
	return s
}
