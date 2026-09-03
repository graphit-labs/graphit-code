package ai

import (
	"bufio"
	"context"
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
	args []string
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
		args:  []string{"--output-format", "stream-json", "--verbose", "--include-partial-messages"},
		parse: parseClaudeStreamLine,
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

	var args []string
	if req.SessionID != "" && spec.sessionFlag != "" {
		args = append(args, spec.sessionFlag, req.SessionID)
	}
	if structured {
		args = append(args, stream.args...)
	}
	if req.AllowTools {
		args = append(args, c.agentArgs...)
	}

	var cleanup func()
	switch spec.mode {
	case inputStdin:
		args = append(args, spec.stdinArgs...)
	case inputFile:
		tmpFile, err := writeTempPrompt(prompt)
		if err != nil {
			return nil, fmt.Errorf("writing temp prompt for %q: %w", c.binaryName, err)
		}
		cleanup = func() { _ = os.Remove(tmpFile) }
		args = append(args, spec.fileArgs...)
		args = append(args, spec.fileFlag, tmpFile)
	case inputArg:
		args = append(args, spec.argArgs...)
		args = append(args, prompt)
	}
	if cleanup != nil {
		defer cleanup()
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
	case inputArg, inputFile:
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
		SessionID:           req.SessionID,
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
