package wiki

import (
	"strings"
)

func isPageRefOnlyAnswer(answer string) bool {
	answer = strings.TrimSpace(answer)
	if answer == "" {
		return true
	}

	lines := strings.Split(answer, "\n")
	totalLines := 0
	refLines := 0

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		line = strings.TrimPrefix(line, "- ")
		line = strings.TrimPrefix(line, "* ")
		for i := 1; i <= 9; i++ {
			stripped := strings.TrimPrefix(line, string(rune('0'+i))+". ")
			if stripped != line {
				line = stripped
				break
			}
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		totalLines++

		if isPageRefLine(line) {
			refLines++
		}
	}

	if totalLines == 0 {
		return true
	}

	return float64(refLines)/float64(totalLines) >= 0.8
}

func isPageRefLine(line string) bool {

	if strings.HasPrefix(line, "[") && strings.Contains(line, "]/") {
		return true
	}

	if strings.Contains(line, "/") && !strings.Contains(line, " ") {
		return true
	}

	if strings.HasPrefix(line, "[[") && strings.HasSuffix(line, "]]") {
		return true
	}

	if !strings.Contains(line, " ") && strings.Count(line, "_") >= 2 {
		parts := strings.SplitN(line, "_", 2)
		if len(parts) == 2 && len(line) > 10 {
			return true
		}
	}

	return false
}

const synthesisSystemPrompt = `You are a technical documentation expert. You receive wiki content as context and answer questions based exclusively on that content.

RULES:
- Write detailed, comprehensive answers in Markdown format.
- Use ## headings, bullet lists, **bold**, and code blocks for clarity.
- Write at least several paragraphs for complex topics.
- Reference source pages using [[Page_Name]] notation as inline citations.
- Never fabricate information — answer only from the provided context.
- Never output lists of page names as your answer.
- Answer directly without any prefix or signal words.`

func buildSynthesisRetryPrompt(query, accumulatedContext string) string {
	return "Based on the following wiki content, answer the question thoroughly.\n\n" +
		"=== WIKI CONTENT ===\n" + accumulatedContext + "\n=== END WIKI CONTENT ===\n\n" +
		"QUESTION: " + query + "\n\n" +
		"Write a comprehensive, detailed answer in Markdown format:"
}
