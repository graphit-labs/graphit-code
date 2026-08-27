package chat

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type AIClient interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

type ChatEngine struct {
	client  AIClient
	session *ChatSession
}

func NewChatEngine(client AIClient, session *ChatSession) *ChatEngine {
	return &ChatEngine{
		client:  client,
		session: session,
	}
}

func (e *ChatEngine) Session() *ChatSession {
	return e.session
}

func (e *ChatEngine) Send(ctx context.Context, userMessage string) (string, error) {

	if err := e.session.Append(ChatMessage{
		Role:    "user",
		Content: userMessage,
	}); err != nil {
		return "", fmt.Errorf("saving user message: %w", err)
	}

	historyCtx, err := e.session.BuildContext(20)
	if err != nil {
		return "", fmt.Errorf("building context: %w", err)
	}

	sourceCtx := buildSourceContext(e.session.Sources)

	systemPrompt := buildChatSystemPrompt(e.session.Sources)

	var userPrompt strings.Builder
	if sourceCtx != "" {
		userPrompt.WriteString(sourceCtx)
		userPrompt.WriteString("\n\n")
	}
	userPrompt.WriteString(historyCtx)
	userPrompt.WriteString("\n[user] ")
	userPrompt.WriteString(userMessage)

	response, err := e.client.Complete(ctx, systemPrompt, userPrompt.String())
	if err != nil {
		return "", fmt.Errorf("AI error: %w", err)
	}

	response = strings.TrimSpace(response)

	if err := e.session.Append(ChatMessage{
		Role:    "assistant",
		Content: response,
	}); err != nil {
		return "", fmt.Errorf("saving AI response: %w", err)
	}

	return response, nil
}

func (e *ChatEngine) SendWithSearch(ctx context.Context, userMessage string, searchResult *wiki.SearchResult) (string, error) {

	if searchResult != nil && searchResult.Answer != "" {
		if err := e.session.Append(ChatMessage{
			Role:    "system",
			Content: fmt.Sprintf("Wiki search result for query %q:\n\n%s", e.session.InitialQuery, searchResult.Answer),
		}); err != nil {
			return "", fmt.Errorf("saving search context: %w", err)
		}
	}

	return e.Send(ctx, userMessage)
}

// splitSources separates the two kinds so the prompts can describe each honestly.
func splitSources(sources []Source) (wikis, graphs []Source) {
	for _, s := range sources {
		if s.IsGraph() {
			graphs = append(graphs, s)
		} else {
			wikis = append(wikis, s)
		}
	}
	return wikis, graphs
}

func buildSourceContext(sources []Source) string {
	if len(sources) == 0 {
		return ""
	}
	wikis, graphs := splitSources(sources)

	var b strings.Builder
	if len(wikis) > 0 {
		b.WriteString("=== Active Wiki Sources ===\n")
		for _, s := range wikis {
			_, _ = fmt.Fprintf(&b, "- [%s] %s\n", s.ID, s.Label)
		}
	}
	if len(graphs) > 0 {
		if b.Len() > 0 {
			b.WriteString("\n")
		}
		b.WriteString("=== Code Graphs Consulted (searched during the original query) ===\n")
		for _, s := range graphs {
			_, _ = fmt.Fprintf(&b, "- [%s] %s\n", s.ID, s.Label)
		}
	}
	return b.String()
}

// buildChatSystemPrompt describes the follow-up turn's actual grounding.
//
// The distinction between the two kinds is not cosmetic. A follow-up turn does NOT
// re-open anything: it answers from the transcript, which already contains whatever
// graph results the original search pulled. Telling the model it "has access to
// wiki sources" therefore mislabelled half of what it was reading — and, worse,
// implied it could reach the code again. It cannot, and saying so is what stops it
// inventing a call graph it has no way to check.
func buildChatSystemPrompt(sources []Source) string {
	wikis, graphs := splitSources(sources)

	describe := func(list []Source) string {
		names := make([]string, len(list))
		for i, s := range list {
			names[i] = fmt.Sprintf("%s (%s)", s.Label, s.ID)
		}
		return strings.Join(names, ", ")
	}

	var access strings.Builder
	if len(wikis) > 0 {
		_, _ = fmt.Fprintf(&access, "%d wiki source(s): %s", len(wikis), describe(wikis))
	}
	if len(graphs) > 0 {
		if access.Len() > 0 {
			access.WriteString("; and ")
		}
		_, _ = fmt.Fprintf(&access,
			"%d indexed code graph(s) searched during the original query: %s",
			len(graphs), describe(graphs))
	}
	if access.Len() == 0 {
		access.WriteString("no sources")
	}

	graphRules := ""
	if len(graphs) > 0 {
		graphRules = `
CODE GRAPH RULES:
- Code graph findings already in the conversation are yours to reason about and cite.
- You CANNOT run new graph queries in this conversation — the graphs were searched
  once, during the original query. If answering needs code you have not been shown,
  say which query or file would settle it instead of guessing at it.
- Cite code as [ast:<source-id>] <path>:<entity>, matching how it appears above.
- Never describe a call, an implementation or a dependency that is not in the context.
  An unverified claim about code reads exactly like a verified one, which is what
  makes it costly.`
	}

	return fmt.Sprintf(`You are a knowledge exploration assistant with access to %s.

You help developers understand project documentation, architecture, decisions, specifications, and — where code graphs were consulted — how the code actually behaves.

PROTOCOL:
- Answer based ONLY on information from the context and conversation history.
- Reference specific wiki pages using [[Page_Name]] syntax when citing sources.
- If the information is not in the provided context, say so clearly.
- Be thorough but focused. Use Markdown formatting for clarity.
- Maintain conversation continuity — refer to previous messages when relevant.
- When multiple wikis are loaded, prefix page references with the wiki source ID: [wiki-id]/[[Page_Name]]%s`,
		access.String(), graphRules)
}
