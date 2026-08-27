package commands

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/signal"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/livesearch/prep"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

// `graphit live` is a second front end onto the same session runtime the web UI
// uses, not a second implementation of it.
//
// It runs the manager in this process rather than talking to a server over SSE. A
// server is a thing to start, a port to find and a process to leave running; a
// question asked from a terminal should not require any of that. What the two front
// ends share is everything that decides behaviour — the session, its event log, the
// ephemeral project, the agent invocation — so an answer here and an answer in the
// browser are the same answer.
//
// The old `wiki search` command is untouched and still does what it did.

func newLiveCmd() *cobra.Command {
	var (
		artifacts []string
		ide       string
		asJSON    bool
	)

	cmd := &cobra.Command{
		Use:   "live [question]",
		Short: "Live search — ask an agent about documentation, code graphs and your memory, streaming.",
		Long: brand.DisplayName + ` Live Search — a streaming, agentic search over whatever you select.

Each search gets its own throwaway project: the artifacts you choose are installed
into it, their documentation is compiled, their code graphs are made queryable, your
own memory is brought in, and an agent is run inside it with the framework's tools.
Nothing about the project is registered anywhere, and it is deleted when you remove
the session.

Artifacts (-a, repeatable) accept any Hub type:
  <id>                       type resolved from the registry
  <type>:<id>                explicit type, for an ID that exists under several
  <id>@<version>             pinned version

Types: ` + artifactTypeList() + `

With a question, the answer streams and the command exits. Without one, the session
stays open and you can keep asking; Ctrl-C stops the answer in progress rather than
the session, and again — or /exit — leaves.

Leaving does not delete the session. Use '` + brand.BinName() + ` live remove <id>'
or the remove button in the web UI.

Examples:
  ` + brand.BinName() + ` live "how does retry work?" -a acme-docs
  ` + brand.BinName() + ` live -a acme-docs -a acme-graph
  ` + brand.BinName() + ` live "where is the parser?" -a ast:acme-graph --ide cursor
  ` + brand.BinName() + ` live "what changed?" -a acme-docs@2.1.0 --json
  ` + brand.BinName() + ` live sessions
  ` + brand.BinName() + ` live remove 01ARZ3NDEKTSV4RRFFQ69G5FAV`,
		Args: cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			question := ""
			if len(args) == 1 {
				question = args[0]
			}
			verbose, _ := cmd.Flags().GetBool("verbose")
			return runLive(cmd.Context(), liveOptions{
				question:  question,
				artifacts: artifacts,
				ide:       ide,
				asJSON:    asJSON,
				verbose:   verbose,
			})
		},
	}

	cmd.Flags().StringArrayVarP(&artifacts, "artifact", "a", nil,
		"Hub artifact to search: [<type>:]<id>[@<version>] (repeatable)")
	cmd.Flags().StringVar(&ide, "ide", "",
		"IDE conventions for the throwaway project (default: your configured IDE)")
	cmd.Flags().BoolVar(&asJSON, "json", false,
		"Emit one JSON event per line instead of formatted output")

	cmd.AddCommand(newLiveSessionsCmd(), newLiveRemoveCmd())
	return cmd
}

func artifactTypeList() string {
	names := make([]string, 0, len(hub.ValidTypes))
	for _, t := range hub.ValidTypes {
		names = append(names, string(t))
	}
	return strings.Join(names, ", ")
}

type liveOptions struct {
	question  string
	artifacts []string
	ide       string
	asJSON    bool
	verbose   bool
}

// parseArtifactSpec reads one -a value.
//
// The version is split on the first "@" and not the last, which is the rule
// hub.Install applies to the string this is reassembled into. An artifact ID may
// legally contain "@", so neither rule is right for every input — but agreeing with
// the installer means the artifact this command reports is the artifact that gets
// installed, which is the property worth having.
func parseArtifactSpec(spec string) (livesearch.Artifact, error) {
	raw := strings.TrimSpace(spec)
	if raw == "" {
		return livesearch.Artifact{}, errors.New("empty artifact")
	}

	var artType string
	// A type prefix is unambiguous because an artifact ID cannot contain a colon.
	if prefix, rest, found := strings.Cut(raw, ":"); found {
		artType, raw = prefix, rest
		if !validArtifactType(artType) {
			return livesearch.Artifact{}, fmt.Errorf("unknown artifact type %q (known: %s)", artType, artifactTypeList())
		}
	}

	id, version := raw, ""
	if parts := strings.SplitN(raw, "@", 2); len(parts) == 2 {
		id, version = parts[0], parts[1]
	}
	if id == "" {
		return livesearch.Artifact{}, fmt.Errorf("artifact %q has no ID", spec)
	}
	if err := hub.ValidateArtifactID(id); err != nil {
		return livesearch.Artifact{}, err
	}
	return livesearch.Artifact{ID: id, Type: artType, Version: version}, nil
}

func validArtifactType(name string) bool {
	for _, t := range hub.ValidTypes {
		if string(t) == name {
			return true
		}
	}
	return false
}

func parseArtifactSpecs(specs []string) ([]livesearch.Artifact, error) {
	out := make([]livesearch.Artifact, 0, len(specs))
	for _, spec := range specs {
		a, err := parseArtifactSpec(spec)
		if err != nil {
			return nil, err
		}
		out = append(out, a)
	}
	return out, nil
}

func runLive(ctx context.Context, opts liveOptions) error {
	if len(opts.artifacts) == 0 {
		return fmt.Errorf("choose at least one artifact to search with -a (see '%s live --help')", brand.BinName())
	}
	chosen, err := parseArtifactSpecs(opts.artifacts)
	if err != nil {
		return err
	}

	ideName := opts.ide
	if ideName == "" {
		ideName = config.ResolveIDE("", nil, nil)
	}
	if err := prep.ValidateIDE(ideName); err != nil {
		return err
	}

	if ctx == nil {
		ctx = context.Background()
	}

	mgr := livesearch.NewManagerFromConfig("", prep.Prepare)
	// Closed, not removed. The session's transcript outlives this command so it can
	// be read again from the web UI; deleting it is something the user asks for.
	defer mgr.CloseAll()

	session, err := mgr.Create(livesearch.Options{
		IDE:       ideName,
		Artifacts: chosen,
		Prompt:    opts.question,
	})
	if err != nil {
		return err
	}

	r := newLiveRenderer(os.Stdout, opts.asJSON, opts.verbose)
	if !opts.asJSON {
		r.p.Info("session %s", session.ID())
	}

	events, stop := session.Subscribe(0)
	defer stop()

	// This command owns Ctrl-C, so it has to take it. The process installs a global
	// handler that prints "Interrupted." and exits immediately, which is right for a
	// command that either finishes or is abandoned — and wrong for this one, where
	// the first Ctrl-C means "stop this answer" and the session is meant to survive
	// it. Without the reset the process would die before the turn was cancelled, and
	// the behaviour promised in --help would never happen.
	signal.Reset(os.Interrupt)
	interrupts := make(chan os.Signal, 2)
	signal.Notify(interrupts, os.Interrupt)
	defer signal.Stop(interrupts)

	// One question: stream it and leave. The turn was dispatched by the session
	// itself once the workspace was ready, so nothing has to be sent here.
	if opts.question != "" {
		return r.streamTurn(ctx, session, events, interrupts)
	}

	if err := r.streamUntilReady(ctx, session, events, interrupts); err != nil {
		return err
	}
	return r.converse(ctx, session, events, interrupts, os.Stdin)
}

// liveRenderer turns session events into terminal output.
type liveRenderer struct {
	out     io.Writer
	p       *output.Printer
	asJSON  bool
	verbose bool
	// midText records that the last thing written was a chunk of the answer, so a
	// following message can start on its own line.
	midText bool
}

// newLiveRenderer points the answer stream and the printed messages at the same
// destination, which is what keeps them in order.
func newLiveRenderer(out io.Writer, asJSON, verbose bool) *liveRenderer {
	return &liveRenderer{
		out:     out,
		p:       output.NewPrinterTo("", out),
		asJSON:  asJSON,
		verbose: verbose,
	}
}

// render writes one event. It reports whether the event ended a turn.
func (r *liveRenderer) render(ev livesearch.Event) bool {
	if r.asJSON {
		// One event per line, exactly as it was recorded, so a script downstream
		// sees what the log holds rather than a prettified summary of it.
		if data, err := json.Marshal(ev); err == nil {
			_, _ = fmt.Fprintf(r.out, "%s\n", data)
		}
		return ev.Kind == livesearch.KindTurnDone
	}

	switch ev.Kind {
	case livesearch.KindText:
		// Written raw and unbuffered: chunks concatenate into the answer, and any
		// prefix or newline this added would be inside a sentence.
		_, _ = fmt.Fprint(r.out, ev.Text)
		r.midText = ev.Text != ""
	case livesearch.KindPrep:
		r.breakText()
		r.p.Step("%s", ev.Text)
	case livesearch.KindPrompt:
		r.breakText()
		r.p.Header("? %s", ev.Text)
	case livesearch.KindThinking:
		if r.verbose {
			r.breakText()
			r.p.Detail("thinking", ev.Text)
		}
	case livesearch.KindToolUse:
		r.breakText()
		r.p.Step("%s %s", ev.Tool, firstLine(ev.Detail, 96))
	case livesearch.KindToolResult:
		// Tool output is large and mostly evidence for the answer that follows.
		if r.verbose {
			r.breakText()
			r.p.Detail(ev.Tool, firstLine(ev.Detail, 96))
		}
	case livesearch.KindStderr:
		if r.verbose {
			r.breakText()
			r.p.Detail("stderr", firstLine(ev.Text, 200))
		}
	case livesearch.KindError:
		r.breakText()
		r.p.Error("%s", ev.Text)
	case livesearch.KindState:
		// Deliberately silent. A failure is always preceded by the error event that
		// explains it, and the command's exit message says the run is over — a line
		// here would repeat one of them.
	case livesearch.KindTurnDone:
		r.breakText()
		return true
	}
	return false
}

// breakText ends the answer's line before something else is printed.
func (r *liveRenderer) breakText() {
	if r.midText {
		_, _ = fmt.Fprintln(r.out)
		r.midText = false
	}
}

func firstLine(s string, max int) string {
	s = strings.TrimSpace(s)
	if i := strings.IndexAny(s, "\r\n"); i >= 0 {
		s = s[:i]
	}
	if len(s) > max {
		return s[:max] + "…"
	}
	return s
}

// errInterrupted reports that the user asked to stop while nothing was running.
var errInterrupted = errors.New("interrupted")

// errSessionFailed ends the command with a non-zero status without repeating the
// reason.
//
// The reason was already streamed as an error event and printed the moment it
// happened. Returning it again would print the same sentence twice, which reads like
// two things went wrong.
var errSessionFailed = errors.New("the session could not be prepared")

// streamUntilReady shows preparation progress and returns when the session can take
// a question.
func (r *liveRenderer) streamUntilReady(ctx context.Context, s *livesearch.Session, events <-chan livesearch.Event, interrupts <-chan os.Signal) error {
	for {
		select {
		case ev, open := <-events:
			if !open {
				return errors.New("the session ended before it was ready")
			}
			r.render(ev)
			if ev.Kind == livesearch.KindState {
				switch ev.State {
				case livesearch.StateReady:
					return nil
				case livesearch.StateFailed:
					return errSessionFailed
				}
			}
		case <-interrupts:
			return errInterrupted
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// streamTurn renders events until the turn ends.
//
// An interrupt cancels the turn and keeps reading, because cancelling produces the
// events that say it was cancelled — returning immediately would leave the last thing
// on screen looking like the agent stopped for no reason.
func (r *liveRenderer) streamTurn(ctx context.Context, s *livesearch.Session, events <-chan livesearch.Event, interrupts <-chan os.Signal) error {
	cancelled := false
	for {
		select {
		case ev, open := <-events:
			if !open {
				return nil
			}
			if r.render(ev) {
				return nil
			}
			if ev.Kind == livesearch.KindState && ev.State == livesearch.StateFailed {
				return errSessionFailed
			}
		case <-interrupts:
			if cancelled {
				// Asked twice: stop waiting and let the deferred close tear the
				// session down.
				r.breakText()
				return errInterrupted
			}
			cancelled = true
			r.breakText()
			r.p.Warn("stopping this answer — Ctrl-C again to leave")
			s.Cancel()
		case <-ctx.Done():
			s.Cancel()
			return ctx.Err()
		}
	}
}

// converse is the interactive loop: read a question, stream the answer, repeat.
func (r *liveRenderer) converse(ctx context.Context, s *livesearch.Session, events <-chan livesearch.Event, interrupts <-chan os.Signal, in io.Reader) error {
	lines := newLineReader(in)
	defer lines.stop()

	r.p.Info("ask a question, or /exit to leave")
	for {
		_, _ = fmt.Fprint(r.out, "\n> ")

		var line string
		select {
		case l, open := <-lines.lines:
			if !open {
				// End of input — a pipe closing or Ctrl-D. Leaving is the only
				// sensible reading of it.
				_, _ = fmt.Fprintln(r.out)
				return nil
			}
			line = strings.TrimSpace(l)
		case <-interrupts:
			_, _ = fmt.Fprintln(r.out)
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}

		switch line {
		case "":
			continue
		case "/exit", "/quit":
			return nil
		}

		if err := s.Send(line); err != nil {
			r.p.Error("%s", err)
			continue
		}
		if err := r.streamTurn(ctx, s, events, interrupts); err != nil {
			if errors.Is(err, errInterrupted) {
				return nil
			}
			return err
		}
	}
}

// lineReader reads lines in the background.
//
// A goroutine, because the loop has to wait on a keystroke and a signal at the same
// time, and a blocking read cannot be selected on.
type lineReader struct {
	lines chan string
	done  chan struct{}
}

func newLineReader(in io.Reader) *lineReader {
	lr := &lineReader{lines: make(chan string), done: make(chan struct{})}
	go func() {
		defer close(lr.lines)
		buf := make([]byte, 0, 256)
		one := make([]byte, 1)
		for {
			n, err := in.Read(one)
			if n > 0 {
				if one[0] == '\n' {
					select {
					case lr.lines <- string(buf):
					case <-lr.done:
						return
					}
					buf = buf[:0]
					continue
				}
				if one[0] != '\r' {
					buf = append(buf, one[0])
				}
			}
			if err != nil {
				if len(buf) > 0 {
					select {
					case lr.lines <- string(buf):
					case <-lr.done:
					}
				}
				return
			}
		}
	}()
	return lr
}

func (lr *lineReader) stop() { close(lr.done) }

func newLiveSessionsCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "sessions",
		Short: "List live search sessions",
		Args:  cobra.NoArgs,
		RunE: func(*cobra.Command, []string) error {
			mgr := livesearch.NewManager("", nil, nil)
			sessions, err := mgr.List()
			if err != nil {
				return err
			}
			p := output.NewPrinter("")
			if len(sessions) == 0 {
				p.Info("no live search sessions")
				return nil
			}
			rows := make([][2]string, 0, len(sessions))
			for _, meta := range sessions {
				label := meta.Title
				if label == "" {
					label = "(no question yet)"
				}
				rows = append(rows, [2]string{
					meta.ID,
					fmt.Sprintf("%-9s %s  %s", meta.State, meta.CreatedAt.Local().Format(time.RFC3339), label),
				})
			}
			p.Table([2]string{"SESSION", "STATE / CREATED / QUESTION"}, rows)
			return nil
		},
	}
}

func newLiveRemoveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "remove <session-id>",
		Short: "Delete a live search session and its throwaway project",
		Args:  cobra.ExactArgs(1),
		RunE: func(_ *cobra.Command, args []string) error {
			mgr := livesearch.NewManager("", nil, nil)
			mgr.SetReclaim(prep.Reclaim)
			if err := mgr.Remove(args[0]); err != nil {
				return err
			}
			output.NewPrinter("").Success("session removed")
			return nil
		},
	}
}
