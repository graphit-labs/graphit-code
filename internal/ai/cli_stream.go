package ai

import (
	"bufio"
	"context"
	"crypto/rand"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"
)

type streamSpec struct {
	args            []string
	capturesSession bool
	// parse turns one line of structured output into events. It returns the events
	// to emit, the assistant text to accumulate, and any session ID learned.
	//
	// A line it does not recognise must be ignored rather than reported: these
	// formats gain event types between releases, and a stream that errors on an
	// unknown line breaks on upgrade.
	parse func(line []byte) (events []Event, text string, sessionID string)
}

var structuredStreams = map[string]streamSpec{
	"claude": {
		args:            []string{"--output-format", "stream-json", "--verbose", "--include-partial-messages"},
		capturesSession: true,
		parse:           parseClaudeStreamLine,
	},
	"gemini": {
		args:            []string{"--output-format", "stream-json"},
		capturesSession: true,
		parse:           parseGeminiStreamLine,
	},
	"agy": {
		args:            []string{"--output-format", "stream-json"},
		capturesSession: true,
		parse:           parseAgyStreamLine,
	},
	"codex": {
		args:            []string{"--json"},
		capturesSession: true,
		parse:           parseCodexStreamLine,
	},
	"opencode": {
		args:            []string{"--format", "json"},
		capturesSession: true,
		parse:           parseOpenCodeStreamLine,
	},
	"qwen": {
		args:            []string{"--output-format", "stream-json"},
		capturesSession: true,
		parse:           parseQwenStreamLine,
	},
	"kimi": {
		args:            []string{"--output-format", "stream-json"},
		capturesSession: true,
		parse:           parseKimiStreamLine,
	},
}

func streamSpecFor(name string) (streamSpec, bool) {
	spec, ok := structuredStreams[name]
	return spec, ok
}

func (c *cliClient) SupportsStructuredStream() bool {
	_, ok := streamSpecFor(c.binaryName)
	return ok
}

// CompleteStream runs one turn, reporting progress as it happens.
//
// The contract holds for every CLI: text arrives incrementally, EventDone is always
// the last event, and StreamResult.Text is the whole answer whether or not the caller
// looked at a single event. What varies is only how much detail the middle has.
func (c *cliClient) CompleteStream(ctx context.Context, req StreamRequest, emit EventFunc) (*StreamResult, error) {
	if emit == nil {
		emit = func(Event) {}
	}
	var emitMu sync.Mutex
	send := func(ev Event) {
		ev.At = time.Now().UTC()
		emitMu.Lock()
		defer emitMu.Unlock()
		emit(ev)
	}

	spec := specForBinary(c.binaryName)
	stream, structured := streamSpecFor(c.binaryName)

	var promptBuilder strings.Builder
	promptBuilder.WriteString(preambleFor(req.AllowTools))
	if req.SystemPrompt != "" {
		promptBuilder.WriteString(req.SystemPrompt)
		promptBuilder.WriteString("\n\n")
	}
	promptBuilder.WriteString(req.UserPrompt)
	prompt := promptBuilder.String()

	effectiveSessionID := req.SessionID
	createdSession := false
	if effectiveSessionID == "" && req.PersistSession && spec.newSessionFlag != "" {
		var err error
		effectiveSessionID, err = newSessionUUID()
		if err != nil {
			return nil, fmt.Errorf("creating session ID for %q: %w", c.binaryName, err)
		}
		createdSession = true
	}

	var args []string
	if c.binaryName == "codex" {
		args = append(args, "exec")
		if req.SessionID != "" {
			args = append(args, "resume", req.SessionID)
		}
		if structured {
			args = append(args, stream.args...)
		}
		if req.AllowTools {
			args = append(args, c.agentArgs...)
		}
		args = append(args, "-")
	} else if isOpenCodeFamily(c.binaryName) {
		args = append(args, spec.argArgs...)
		if req.SessionID != "" {
			args = append(args, spec.sessionFlag, req.SessionID)
		}
		if structured {
			args = append(args, stream.args...)
		}
		if req.AllowTools {
			args = append(args, c.agentArgs...)
		}
		args = append(args, prompt)
	} else {
		if req.SessionID != "" && spec.sessionFlag != "" {
			args = append(args, spec.sessionFlag, req.SessionID)
		} else if createdSession {
			args = append(args, spec.newSessionFlag, effectiveSessionID)
		}
		if structured {
			args = append(args, stream.args...)
		}
		if req.AllowTools {
			args = append(args, c.agentArgs...)
		}
	}

	switch {
	case c.binaryName == "codex" || isOpenCodeFamily(c.binaryName):
	case spec.mode == inputStdin:
		args = append(args, spec.stdinArgs...)
	case spec.mode == inputArg:
		args = append(args, spec.argArgs...)
		args = append(args, prompt)
	}

	cmd := exec.CommandContext(ctx, c.executablePath, args...)

	cmd.Dir = req.WorkDir

	cmd.Env = append(os.Environ(), "NO_COLOR=1", "TERM=dumb")
	for k, v := range req.Env {
		cmd.Env = append(cmd.Env, k+"="+v)
	}

	switch spec.mode {
	case inputStdin:
		cmd.Stdin = strings.NewReader(prompt)
	case inputArg:
		// No stdin needed; an empty reader prevents a deadlock on a CLI that reads
		// stdin anyway.
		cmd.Stdin = strings.NewReader("")
	}

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("stdout pipe for %q: %w", c.binaryName, err)
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return nil, fmt.Errorf("stderr pipe for %q: %w", c.binaryName, err)
	}

	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("starting %q: %w", c.binaryName, err)
	}

	result := &StreamResult{
		SessionID:           effectiveSessionID,
		Structured:          structured,
		Binary:              c.binaryName,
		AgentArgsConfigured: len(c.agentArgs) > 0,
	}

	var wg sync.WaitGroup
	var textBuf strings.Builder
	var stderrBuf strings.Builder

	wg.Add(1)
	go func() {
		defer wg.Done()
		if structured {
			if !readStructured(stdout, stream, send, &textBuf, &result.SessionID) {
				result.Structured = false
			}
			return
		}
		readIncrementalText(stdout, send, &textBuf)
	}()

	wg.Add(1)
	go func() {
		defer wg.Done()
		scanner := bufio.NewScanner(stderr)
		scanner.Buffer(make([]byte, 0, 64*1024), maxStreamLine)
		for scanner.Scan() {
			line := scanner.Text()
			stderrBuf.WriteString(line)
			stderrBuf.WriteString("\n")
			if strings.TrimSpace(line) != "" {
				send(Event{Kind: EventStderr, Text: line})
			}
		}
	}()

	wg.Wait()
	runErr := cmd.Wait()

	result.Text = strings.TrimSpace(textBuf.String())

	if runErr != nil {
		err := fmt.Errorf("CLI %q failed: %w (stderr: %s)",
			c.binaryName, runErr, strings.TrimSpace(stderrBuf.String()))
		send(Event{Kind: EventError, Text: err.Error()})
		send(Event{Kind: EventDone})
		return result, err
	}

	if result.SessionID != req.SessionID && result.SessionID != "" {
		send(Event{Kind: EventSession, SessionID: result.SessionID})
	}
	send(Event{Kind: EventDone})
	return result, nil
}

func isOpenCodeFamily(binary string) bool {
	return binary == "opencode"
}

func newSessionUUID() (string, error) {
	var raw [16]byte
	if _, err := rand.Read(raw[:]); err != nil {
		return "", err
	}
	raw[6] = (raw[6] & 0x0f) | 0x40
	raw[8] = (raw[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		raw[0:4], raw[4:6], raw[6:8], raw[8:10], raw[10:16]), nil
}

const maxStreamLine = 8 * 1024 * 1024

func readIncrementalText(r io.Reader, send EventFunc, textBuf *strings.Builder) {
	buf := make([]byte, 4096)
	for {
		n, err := r.Read(buf)
		if n > 0 {
			chunk := string(buf[:n])
			textBuf.WriteString(chunk)
			send(Event{Kind: EventText, Text: chunk})
		}
		if err != nil {
			return
		}
	}
}

func readStructured(
	r io.Reader,
	spec streamSpec,
	send EventFunc,
	textBuf *strings.Builder,
	sessionID *string,
) (wasStructured bool) {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 64*1024), maxStreamLine)

	parsedAny := false
	var unparsed []string

	for scanner.Scan() {
		line := scanner.Bytes()
		if strings.TrimSpace(string(line)) == "" {
			continue
		}
		events, text, sid := spec.parse(line)
		if events == nil && text == "" && sid == "" {
			unparsed = append(unparsed, string(line))
			continue
		}
		parsedAny = true
		if text != "" {
			textBuf.WriteString(text)
		}
		if sid != "" {
			*sessionID = sid
		}
		for _, ev := range events {
			send(ev)
		}
	}

	if parsedAny {
		return true
	}
	for _, line := range unparsed {
		textBuf.WriteString(line)
		textBuf.WriteString("\n")
		send(Event{Kind: EventText, Text: line + "\n"})
	}
	return false
}

type claudeStreamLine struct {
	Type      string `json:"type"`
	Subtype   string `json:"subtype"`
	SessionID string `json:"session_id"`
	Message   struct {
		Content []struct {
			Type  string          `json:"type"`
			Text  string          `json:"text"`
			Name  string          `json:"name"`
			Input json.RawMessage `json:"input"`
			// tool_result blocks carry content that is either a string or an array
			// of blocks, so it stays raw and is rendered by renderToolPayload.
			Content json.RawMessage `json:"content"`
		} `json:"content"`
	} `json:"message"`
	// Partial-message deltas, emitted with --include-partial-messages.
	Delta struct {
		Type string `json:"type"`
		Text string `json:"text"`
	} `json:"delta"`
	Result string `json:"result"`
}

func parseClaudeStreamLine(line []byte) ([]Event, string, string) {
	var l claudeStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}

	var events []Event
	var text string

	switch l.Type {
	case "system":
		// The init event is where the session ID arrives. Capturing it is what makes
		// a resumable conversation possible at all: the previous implementation
		// echoed back whatever ID it was given, so a first turn could never learn
		// one, and --resume had nothing to resume.
		return nil, "", l.SessionID

	case "stream_event":
		if l.Delta.Type == "text_delta" && l.Delta.Text != "" {
			return []Event{{Kind: EventText, Text: l.Delta.Text}}, l.Delta.Text, l.SessionID
		}
		return nil, "", l.SessionID

	case "assistant":
		for _, block := range l.Message.Content {
			switch block.Type {
			case "text":
				if block.Text != "" && !hasPartialDeltas {
					events = append(events, Event{Kind: EventText, Text: block.Text})
					text += block.Text
				}
			case "thinking":
				if block.Text != "" {
					events = append(events, Event{Kind: EventThinking, Text: block.Text})
				}
			case "tool_use":
				events = append(events, Event{
					Kind:   EventToolUse,
					Tool:   block.Name,
					Detail: renderToolPayload(block.Input),
				})
			}
		}
		return events, text, l.SessionID

	case "user":
		for _, block := range l.Message.Content {
			if block.Type == "tool_result" {
				events = append(events, Event{
					Kind:   EventToolResult,
					Detail: renderToolPayload(block.Content),
				})
			}
		}
		return events, "", l.SessionID

	case "result":
		if l.Subtype != "success" && l.Result != "" {
			return []Event{{Kind: EventError, Text: l.Result}}, "", l.SessionID
		}
		return nil, "", l.SessionID
	}

	return nil, "", l.SessionID
}

type geminiStreamLine struct {
	Type      string `json:"type"`
	SessionID string `json:"session_id"`
	Role      string `json:"role"`
	Content   string `json:"content"`
	Message   string `json:"message"`
	ToolName  string `json:"tool_name"`
}

func parseGeminiStreamLine(line []byte) ([]Event, string, string) {
	var l geminiStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	switch l.Type {
	case "init":
		return nil, "", l.SessionID
	case "message":
		if l.Role == "assistant" && l.Content != "" {
			return []Event{{Kind: EventText, Text: l.Content}}, l.Content, l.SessionID
		}
	case "tool_use":
		return []Event{{Kind: EventToolUse, Tool: l.ToolName}}, "", l.SessionID
	case "tool_result":
		return []Event{{Kind: EventToolResult, Tool: l.ToolName, Detail: l.Content}}, "", l.SessionID
	case "error":
		return []Event{{Kind: EventError, Text: l.Message}}, "", l.SessionID
	}
	return nil, "", l.SessionID
}

type agyStreamLine struct {
	Event          string `json:"event"`
	ConversationID string `json:"conversation_id"`
	StepUpdate     struct {
		ConversationID string          `json:"conversation_id"`
		StepType       string          `json:"step_type"`
		ToolName       string          `json:"tool_name"`
		TextDelta      string          `json:"text_delta"`
		ToolInfo       json.RawMessage `json:"tool_info"`
	} `json:"step_update"`
	Result struct {
		ConversationID string `json:"conversation_id"`
		Status         string `json:"status"`
		Error          string `json:"error"`
	} `json:"result"`
}

func parseAgyStreamLine(line []byte) ([]Event, string, string) {
	var l agyStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	sessionID := l.ConversationID
	if sessionID == "" {
		sessionID = l.StepUpdate.ConversationID
	}
	if sessionID == "" {
		sessionID = l.Result.ConversationID
	}
	switch l.Event {
	case "init":
		return nil, "", sessionID
	case "step_update":
		if l.StepUpdate.TextDelta != "" {
			return []Event{{Kind: EventText, Text: l.StepUpdate.TextDelta}}, l.StepUpdate.TextDelta, sessionID
		}
		if l.StepUpdate.StepType == "tool" {
			return []Event{{Kind: EventToolUse, Tool: l.StepUpdate.ToolName, Detail: renderToolPayload(l.StepUpdate.ToolInfo)}}, "", sessionID
		}
	case "result":
		if l.Result.Status != "" && l.Result.Status != "SUCCESS" {
			return []Event{{Kind: EventError, Text: l.Result.Error}}, "", sessionID
		}
	}
	return nil, "", sessionID
}

type codexStreamLine struct {
	Type     string `json:"type"`
	ThreadID string `json:"thread_id"`
	Message  string `json:"message"`
	Item     struct {
		Type             string `json:"type"`
		Text             string `json:"text"`
		Command          string `json:"command"`
		AggregatedOutput string `json:"aggregated_output"`
	} `json:"item"`
	Error struct {
		Message string `json:"message"`
	} `json:"error"`
}

func parseCodexStreamLine(line []byte) ([]Event, string, string) {
	var l codexStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	switch l.Type {
	case "thread.started":
		return nil, "", l.ThreadID
	case "item.started":
		if l.Item.Type == "command_execution" {
			return []Event{{Kind: EventToolUse, Tool: "command", Detail: l.Item.Command}}, "", l.ThreadID
		}
	case "item.completed":
		switch l.Item.Type {
		case "agent_message":
			if l.Item.Text != "" {
				return []Event{{Kind: EventText, Text: l.Item.Text}}, l.Item.Text, l.ThreadID
			}
		case "command_execution":
			return []Event{{Kind: EventToolResult, Tool: "command", Detail: l.Item.AggregatedOutput}}, "", l.ThreadID
		}
	case "turn.failed", "error":
		message := l.Error.Message
		if message == "" {
			message = l.Message
		}
		return []Event{{Kind: EventError, Text: message}}, "", l.ThreadID
	}
	return nil, "", l.ThreadID
}

type openCodeStreamLine struct {
	Type      string `json:"type"`
	SessionID string `json:"sessionID"`
	Text      string `json:"text"`
	Part      struct {
		Type  string `json:"type"`
		Text  string `json:"text"`
		Tool  string `json:"tool"`
		State struct {
			Output string `json:"output"`
			Error  string `json:"error"`
		} `json:"state"`
	} `json:"part"`
}

func parseOpenCodeStreamLine(line []byte) ([]Event, string, string) {
	var l openCodeStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	text := l.Text
	if text == "" {
		text = l.Part.Text
	}
	switch l.Type {
	case "text":
		if text != "" {
			return []Event{{Kind: EventText, Text: text}}, text, l.SessionID
		}
	case "reasoning":
		return []Event{{Kind: EventThinking, Text: text}}, "", l.SessionID
	case "tool_use":
		return []Event{{Kind: EventToolUse, Tool: l.Part.Tool}}, "", l.SessionID
	case "tool_result":
		return []Event{{Kind: EventToolResult, Tool: l.Part.Tool, Detail: l.Part.State.Output}}, "", l.SessionID
	case "error":
		message := l.Part.State.Error
		if message == "" {
			message = text
		}
		return []Event{{Kind: EventError, Text: message}}, "", l.SessionID
	}
	return nil, "", l.SessionID
}

type qwenStreamLine struct {
	Type      string          `json:"type"`
	Subtype   string          `json:"subtype"`
	SessionID string          `json:"session_id"`
	Role      string          `json:"role"`
	Content   json.RawMessage `json:"content"`
	Message   struct {
		Role    string          `json:"role"`
		Content json.RawMessage `json:"content"`
	} `json:"message"`
	ToolName string          `json:"tool_name"`
	Name     string          `json:"name"`
	Input    json.RawMessage `json:"input"`
}

func parseQwenStreamLine(line []byte) ([]Event, string, string) {
	var l qwenStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	if (l.Type == "system" || l.Type == "init" || l.Subtype == "session_start") && l.SessionID != "" {
		return nil, "", l.SessionID
	}
	role, content := l.Role, l.Content
	if l.Message.Role != "" || len(l.Message.Content) > 0 {
		role, content = l.Message.Role, l.Message.Content
	}
	if l.Type == "assistant" || l.Type == "message" && role == "assistant" || role == "assistant" {
		events, text := parseContentEvents(content)
		return events, text, l.SessionID
	}
	if l.Type == "tool_use" {
		name := l.ToolName
		if name == "" {
			name = l.Name
		}
		return []Event{{Kind: EventToolUse, Tool: name, Detail: renderToolPayload(l.Input)}}, "", l.SessionID
	}
	if l.Type == "tool_result" || role == "tool" {
		return []Event{{Kind: EventToolResult, Tool: l.ToolName, Detail: renderToolPayload(content)}}, "", l.SessionID
	}
	return nil, "", l.SessionID
}

type kimiStreamLine struct {
	Role       string          `json:"role"`
	Type       string          `json:"type"`
	Content    json.RawMessage `json:"content"`
	SessionID  string          `json:"session_id"`
	SessionID2 string          `json:"sessionId"`
	Data       struct {
		SessionID  string `json:"session_id"`
		SessionID2 string `json:"sessionId"`
		ID         string `json:"id"`
	} `json:"data"`
	Name string `json:"name"`
}

func parseKimiStreamLine(line []byte) ([]Event, string, string) {
	var l kimiStreamLine
	if json.Unmarshal(line, &l) != nil {
		return nil, "", ""
	}
	sessionID := firstNonEmpty(l.SessionID, l.SessionID2, l.Data.SessionID, l.Data.SessionID2)
	if l.Role == "meta" && l.Type == "session.resume_hint" {
		return nil, "", firstNonEmpty(sessionID, l.Data.ID)
	}
	if l.Role == "assistant" {
		events, text := parseContentEvents(l.Content)
		return events, text, sessionID
	}
	if l.Role == "tool" {
		return []Event{{Kind: EventToolResult, Tool: l.Name, Detail: renderToolPayload(l.Content)}}, "", sessionID
	}
	return nil, "", sessionID
}

func parseContentEvents(raw json.RawMessage) ([]Event, string) {
	if len(raw) == 0 || string(raw) == "null" {
		return nil, ""
	}
	var plain string
	if json.Unmarshal(raw, &plain) == nil {
		if plain == "" {
			return nil, ""
		}
		return []Event{{Kind: EventText, Text: plain}}, plain
	}
	var blocks []struct {
		Type  string          `json:"type"`
		Text  string          `json:"text"`
		Name  string          `json:"name"`
		Input json.RawMessage `json:"input"`
	}
	if json.Unmarshal(raw, &blocks) != nil {
		return nil, ""
	}
	var events []Event
	var text strings.Builder
	for _, block := range blocks {
		switch block.Type {
		case "text", "output_text":
			if block.Text != "" {
				events = append(events, Event{Kind: EventText, Text: block.Text})
				text.WriteString(block.Text)
			}
		case "thinking", "reasoning":
			if block.Text != "" {
				events = append(events, Event{Kind: EventThinking, Text: block.Text})
			}
		case "tool_use", "tool_call":
			events = append(events, Event{Kind: EventToolUse, Tool: block.Name, Detail: renderToolPayload(block.Input)})
		}
	}
	return events, text.String()
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

const hasPartialDeltas = true

func renderToolPayload(raw json.RawMessage) string {
	if len(raw) == 0 {
		return ""
	}
	s := strings.TrimSpace(string(raw))

	var asString string
	if json.Unmarshal(raw, &asString) == nil {
		s = asString
	}

	const max = 2000
	if len(s) > max {
		return s[:max] + "… (truncated)"
	}
	return s
}
