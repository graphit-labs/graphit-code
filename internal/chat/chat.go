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

	wikiCtx := buildWikiSourceContext(e.session.WikiSources)

	systemPrompt := buildChatSystemPrompt(e.session.WikiSources)

	var userPrompt strings.Builder
	if wikiCtx != "" {
		userPrompt.WriteString(wikiCtx)
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

func buildWikiSourceContext(sources []WikiSource) string {
	if len(sources) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("=== Active Wiki Sources ===\n")
	for _, s := range sources {
		_, _ = fmt.Fprintf(&b, "- [%s] %s\n", s.ID, s.Label)
	}
	return b.String()
}

func buildChatSystemPrompt(sources []WikiSource) string {
	sourceList := make([]string, len(sources))
	for i, s := range sources {
		sourceList[i] = fmt.Sprintf("%s (%s)", s.Label, s.ID)
	}

	return fmt.Sprintf(`You are a knowledge exploration assistant with access to %d wiki sources: %s.

You help developers understand project documentation, architecture, decisions, and specifications by answering questions based on the wiki content provided in context.

PROTOCOL:
- Answer based ONLY on information from the wiki context and conversation history.
- Reference specific wiki pages using [[Page_Name]] syntax when citing sources.
- If the information is not in the provided context, say so clearly.
- Be thorough but focused. Use Markdown formatting for clarity.
- Maintain conversation continuity — refer to previous messages when relevant.
- When multiple wikis are loaded, prefix page references with the wiki source ID: [wiki-id]/[[Page_Name]]`,
		len(sources), strings.Join(sourceList, ", "))
}
