package livesearch

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/oklog/ulid/v2"
)

// A live search session is a server-side object with its own lifetime, and that is
// the whole point of this file.
//
// The previous model ran the agent inside the HTTP handler: the run was the request.
// That made every property of the run a property of a TCP connection — closing the
// tab killed the work, a proxy timeout killed the work, and a second question had to
// re-establish everything the first one had already built. Since preparing the
// ephemeral project costs most of the wall clock, throwing it away on disconnect is
// throwing away the expensive part.
//
// Here the session owns a context derived from Background, never from a request. The
// goroutine that runs a turn answers to the session, so a client may disconnect,
// reconnect, or never come back, and the run finishes either way. Cancelling is then
// something a user does explicitly instead of something a network does accidentally.

// State is what a session is currently doing.
type State string

const (
	// StatePreparing means the ephemeral project is being built. No turn can run yet.
	StatePreparing State = "preparing"
	// StateReady means the session is idle and can take a turn.
	StateReady State = "ready"
	// StateRunning means a turn is in progress.
	StateRunning State = "running"
	// StateFailed means preparation failed and the session cannot work at all. A
	// turn that errors does not land here; see runTurn.
	StateFailed State = "failed"
	// StateClosed means the session was removed.
	StateClosed State = "closed"
)

// Errors callers are expected to distinguish, because each one is a different
// answer to "why can't I send this".
var (
	ErrNotFound  = errors.New("session not found")
	ErrPreparing = errors.New("session is still preparing")
	ErrBusy      = errors.New("session is already running a turn")
	ErrFailed    = errors.New("session failed to prepare")
	ErrClosed    = errors.New("session is closed")
)

// workspaceDirName is the subdirectory the ephemeral project lives in.
//
// The project is a level below the session directory rather than being it, because
// the session's own bookkeeping — session.json, events.jsonl — sits in that
// directory, and the preparation step runs the AST and knowledge indexers over the
// project root. Sharing one directory would have the session indexing its own
// transcript and then answering questions about it.
const workspaceDirName = "workspace"

const (
	metaFileName   = "session.json"
	eventsFileName = "events.jsonl"
)

// tailBuffer is how many events a subscriber may fall behind before it is dropped.
const tailBuffer = 256

// Artifact is one Hub artifact the user chose for this session. Any type is
// allowed: a knowledge wiki, a code graph, a rule, a skill, whatever the Hub
// carries.
type Artifact struct {
	ID      string `json:"id"`
	Type    string `json:"type"`
	Version string `json:"version,omitempty"`
}

// Meta is the part of a session that survives a restart.
type Meta struct {
	ID        string     `json:"id"`
	State     State      `json:"state"`
	IDE       string     `json:"ide"`
	Title     string     `json:"title,omitempty"`
	Artifacts []Artifact `json:"artifacts,omitempty"`
	CreatedAt time.Time  `json:"created_at"`
	UpdatedAt time.Time  `json:"updated_at"`
	// Error explains a failed state.
	Error string `json:"error,omitempty"`
}

// Options describes a session to create.
type Options struct {
	// IDE selects which agent CLI conventions the ephemeral project is set up for.
	IDE string
	// Title is a human label, usually the first question.
	Title string
	// Artifacts are the Hub artifacts to install into the ephemeral project.
	Artifacts []Artifact
	// Prompt is an optional first question, asked as soon as preparation finishes.
	//
	// It exists so that asking does not depend on a client still being connected
	// when the workspace becomes ready. Preparation takes most of the wall clock;
	// requiring the caller to watch for the ready state and then send the question
	// would put the same waiting loop in every client, and would waste the whole
	// preparation whenever the user closed the tab while it ran.
	Prompt string
}

// PrepareFunc builds the ephemeral project for a session. It reports progress
// through the callback, which is surfaced to subscribers as KindPrep events.
//
// It is injected rather than called directly so that the runtime can be tested
// without a Hub, a network or an indexer, and so that preparation can grow
// (artifact installs, index compilation, memory seeding) without this file
// changing.
type PrepareFunc func(ctx context.Context, s *Session, progress func(string)) error

// ReclaimFunc deletes anything a session left outside its own directory, given the
// session ID that keyed it.
//
// Injected for the same reason as PrepareFunc: what a session could have left behind
// lives in the store, memory and knowledge packages, and this one imports none of
// them — they pull in the generated ANTLR parsers, which cost minutes to link and
// which the runtime has no need of.
//
// A session should not create any such thing in the first place. This exists because
// earlier versions did, and because "should not" is a property of the current code
// rather than of the machine it runs on.
type ReclaimFunc func(sessionID string)

// Session is one live search.
type Session struct {
	id  string
	dir string

	client  ai.StreamClient
	prepare PrepareFunc

	// ctx is the session's own, derived from Background so that no request can
	// cancel it, and cancelled by Cancel or Close.
	ctx    context.Context
	cancel context.CancelFunc

	log *eventLog

	// initialPrompt is asked once, when preparation succeeds. Written at
	// construction and read only by the preparation goroutine.
	initialPrompt string

	// mu guards the session's mutable state.
	mu           sync.Mutex
	state        State
	meta         Meta
	turnCancel   context.CancelFunc
	cliSessionID string

	// work counts the goroutines that write inside the session directory: the
	// preparation, and each turn. Close waits on it so that when Close returns,
	// nothing is still writing — which is what makes deleting the directory
	// afterwards safe rather than merely usually safe.
	work sync.WaitGroup

	// subMu guards the subscriber set and serialises appending to the log with
	// broadcasting, which is what makes sequence order and delivery order the same
	// order. It is deliberately separate from mu: a state change wants to persist
	// under mu and then emit, and one lock for both would deadlock on itself.
	subMu   sync.Mutex
	subs    map[int64]chan Event
	nextSub int64
	closed  bool
}

// closeGrace bounds how long Close waits for work to stop.
//
// Cancelling the context kills the agent subprocess, so the wait is normally
// instant. The bound exists for the case where it is not: a remove button that can
// hang forever is a remove button nobody trusts.
const closeGrace = 10 * time.Second

// ID returns the session identifier.
func (s *Session) ID() string { return s.id }

// Dir returns the session directory, which holds the bookkeeping and the workspace.
func (s *Session) Dir() string { return s.dir }

// WorkspaceDir returns the ephemeral project directory: the one an agent CLI is
// run inside, and therefore the one it discovers rules, skills and MCP servers from.
func (s *Session) WorkspaceDir() string { return filepath.Join(s.dir, workspaceDirName) }

// State returns the current state.
func (s *Session) State() State {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.state
}

// Meta returns a copy of the persisted metadata.
func (s *Session) Meta() Meta {
	s.mu.Lock()
	defer s.mu.Unlock()
	m := s.meta
	m.Artifacts = append([]Artifact(nil), s.meta.Artifacts...)
	return m
}

// LastSeq returns the sequence number of the most recent event, which a client can
// use as a starting point.
func (s *Session) LastSeq() int64 { return s.log.lastSeq() }

// Manager owns the live sessions of a process.
type Manager struct {
	root    string
	client  ai.StreamClient
	prepare PrepareFunc
	reclaim ReclaimFunc

	mu       sync.Mutex
	sessions map[string]*Session
}

// DefaultRoot is where sessions live: ~/.graphit/sessions/<id>/.
//
// Not under the chat session tree, which buckets by a hash of the project
// directory. A live search session has no project to be bucketed under — it *is*
// its own project, created for the session and deleted with it.
func DefaultRoot() string {
	g := brand.GlobalDir()
	if g == "" {
		return ""
	}
	return filepath.Join(g, "sessions")
}

// NewManagerFromConfig builds a manager using the agent CLI from the user's
// configuration.
//
// A CLI that cannot stream leaves the manager without a client, and sessions then
// refuse turns with a clear message instead of appearing to run one. That is why the
// error from building the client is dropped rather than returned: not having a usable
// agent is a reason a turn fails, not a reason a session cannot be created and
// inspected.
func NewManagerFromConfig(root string, prepare PrepareFunc) *Manager {
	var stream ai.StreamClient
	if client, err := ai.NewClientFromConfig(); err == nil {
		if sc, ok := client.(ai.StreamClient); ok {
			stream = sc
		}
	}
	return NewManager(root, stream, prepare)
}

// NewManager builds a manager. An empty root means DefaultRoot.
func NewManager(root string, client ai.StreamClient, prepare PrepareFunc) *Manager {
	if root == "" {
		root = DefaultRoot()
	}
	return &Manager{
		root:     root,
		client:   client,
		prepare:  prepare,
		sessions: make(map[string]*Session),
	}
}

// Root returns the directory sessions are created under.
func (m *Manager) Root() string { return m.root }

// Create makes a session, starts preparing it, and returns immediately.
//
// Preparation runs in its own goroutine so that the caller — an HTTP handler with a
// client waiting — gets a session ID it can subscribe to before the slow work
// starts, rather than a connection held open through it.
func (m *Manager) Create(opts Options) (*Session, error) {
	if m.root == "" {
		return nil, errors.New("cannot resolve the sessions directory")
	}
	id := newSessionID()
	dir := filepath.Join(m.root, id)
	if err := os.MkdirAll(filepath.Join(dir, workspaceDirName), 0o755); err != nil {
		return nil, fmt.Errorf("creating session directory: %w", err)
	}

	log, err := openEventLog(filepath.Join(dir, eventsFileName))
	if err != nil {
		return nil, err
	}

	now := time.Now().UTC()
	label := opts.Title
	if label == "" {
		label = title(opts.Prompt)
	}
	ctx, cancel := context.WithCancel(context.Background())
	s := &Session{
		id:            id,
		dir:           dir,
		client:        m.client,
		prepare:       m.prepare,
		ctx:           ctx,
		cancel:        cancel,
		log:           log,
		initialPrompt: opts.Prompt,
		state:         StatePreparing,
		subs:          make(map[int64]chan Event),
		meta: Meta{
			ID:        id,
			State:     StatePreparing,
			IDE:       opts.IDE,
			Title:     label,
			Artifacts: append([]Artifact(nil), opts.Artifacts...),
			CreatedAt: now,
			UpdatedAt: now,
		},
	}
	if err := s.persist(); err != nil {
		_ = log.close()
		cancel()
		return nil, err
	}

	m.mu.Lock()
	m.sessions[id] = s
	m.mu.Unlock()

	s.work.Add(1)
	go s.runPreparation()
	return s, nil
}

// Get returns a live session.
func (m *Manager) Get(id string) (*Session, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	s, ok := m.sessions[id]
	if !ok {
		return nil, ErrNotFound
	}
	return s, nil
}

// List reports every session on disk, newest first.
//
// A session directory left by a previous process is reported as failed rather than
// as whatever it was doing when the process died: a persisted "running" with no
// goroutine behind it describes a run that cannot produce another event, and
// showing it as active invites a client to wait for one.
func (m *Manager) List() ([]Meta, error) {
	entries, err := os.ReadDir(m.root)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("reading sessions: %w", err)
	}

	m.mu.Lock()
	live := make(map[string]*Session, len(m.sessions))
	for id, s := range m.sessions {
		live[id] = s
	}
	m.mu.Unlock()

	var out []Meta
	for _, e := range entries {
		if !e.IsDir() || !validSessionID(e.Name()) {
			continue
		}
		if s, ok := live[e.Name()]; ok {
			out = append(out, s.Meta())
			continue
		}
		meta, err := m.metaFromDisk(e.Name())
		if err != nil {
			continue
		}
		out = append(out, meta)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].CreatedAt.After(out[j].CreatedAt) })
	return out, nil
}

// Meta returns a session's metadata whether or not it is live in this process.
//
// The fallback to disk is what lets a client that listed sessions after a restart
// open one of them: the session is gone as an object, but it is still a directory
// with a story in it.
func (m *Manager) Meta(id string) (Meta, error) {
	if !validSessionID(id) {
		return Meta{}, ErrNotFound
	}
	m.mu.Lock()
	s, live := m.sessions[id]
	m.mu.Unlock()
	if live {
		return s.Meta(), nil
	}
	return m.metaFromDisk(id)
}

// Replay yields the recorded events of a session, whether or not it is live here.
//
// This is what the durable log is for. A session's story is on disk, so a process that
// never ran it can still tell it — and without this, a client that listed sessions
// after a restart could see one and not be allowed to read it, which is the log's
// entire purpose refused on a technicality.
//
// Only history: a session with no goroutine behind it will never gain another event,
// so there is nothing to follow.
func (m *Manager) Replay(id string, after int64, fn func(Event) error) error {
	if !validSessionID(id) {
		return ErrNotFound
	}
	path := filepath.Join(m.root, id, eventsFileName)
	if _, err := os.Stat(path); err != nil {
		return ErrNotFound
	}
	// Constructed rather than opened: opening for append would create the file and
	// rewrite its last line, which is not a read.
	return (&eventLog{path: path}).replay(after, 0, fn)
}

// metaFromDisk loads persisted metadata and corrects the states that cannot be
// true of a session with no process behind it.
func (m *Manager) metaFromDisk(id string) (Meta, error) {
	meta, err := loadMeta(filepath.Join(m.root, id, metaFileName))
	if err != nil {
		return Meta{}, ErrNotFound
	}
	if meta.State == StatePreparing || meta.State == StateRunning {
		meta.State = StateFailed
		meta.Error = "interrupted before it finished"
	}
	return meta, nil
}

// Remove closes a session and deletes everything it owns, including the ephemeral
// project. This is the remove button.
func (m *Manager) Remove(id string) error {
	if !validSessionID(id) {
		return ErrNotFound
	}
	m.mu.Lock()
	s, live := m.sessions[id]
	delete(m.sessions, id)
	m.mu.Unlock()

	dir := filepath.Join(m.root, id)
	if live {
		// Close before deleting, and not only for tidiness: Windows refuses to
		// remove a file that still has an open handle, so a session removed while
		// its log was open would fail there and succeed everywhere else.
		_ = s.Close()
		dir = s.dir
	}
	if err := os.RemoveAll(dir); err != nil {
		return fmt.Errorf("removing session: %w", err)
	}
	if m.reclaim != nil {
		// After the directory, and never instead of it: reclaiming global state is
		// housekeeping for residue older versions left, while deleting the session is
		// what the caller asked for. A failure to find anything to reclaim is the
		// normal case and is not reported.
		m.reclaim(id)
	}
	return nil
}

// SetReclaim installs the hook Remove calls after deleting a session, to collect
// anything keyed by the session ID outside the session's own directory.
func (m *Manager) SetReclaim(fn ReclaimFunc) { m.reclaim = fn }

// CloseAll shuts every session down without deleting anything, for process exit.
func (m *Manager) CloseAll() {
	m.mu.Lock()
	all := make([]*Session, 0, len(m.sessions))
	for _, s := range m.sessions {
		all = append(all, s)
	}
	m.sessions = make(map[string]*Session)
	m.mu.Unlock()

	// In parallel, because each Close waits for its own session's work to stop and
	// closing serially would add those waits together on the way out.
	var wg sync.WaitGroup
	for _, s := range all {
		wg.Add(1)
		go func(s *Session) {
			defer wg.Done()
			_ = s.Close()
		}(s)
	}
	wg.Wait()
}

// Subscribe returns a stream of events after the given sequence number, history
// first and then live ones, plus a function to stop listening.
//
// The ordering here is the reason this is not two calls. Reading the history and
// then registering for new events loses whatever arrives in between; registering
// and then reading the history delivers those twice. So registration and the
// snapshot of "how far the history goes" happen under one lock, and the events in
// between are attributed to exactly one phase: the file replay stops at the
// snapshot, and the live channel skips anything at or below it.
func (s *Session) Subscribe(after int64) (<-chan Event, func()) {
	s.subMu.Lock()
	tail := make(chan Event, tailBuffer)
	id := s.nextSub
	s.nextSub++
	s.subs[id] = tail
	closed := s.closed
	s.subMu.Unlock()

	upto := s.log.lastSeq()

	out := make(chan Event)
	done := make(chan struct{})
	var stopOnce sync.Once
	stop := func() {
		stopOnce.Do(func() {
			close(done)
			s.unsubscribe(id)
		})
	}

	go func() {
		defer close(out)
		err := s.log.replay(after, upto, func(ev Event) error {
			select {
			case out <- ev:
				return nil
			case <-done:
				return errStopReplay
			}
		})
		if err != nil {
			select {
			case out <- Event{Kind: KindError, Text: err.Error(), At: time.Now().UTC()}:
			case <-done:
				return
			}
		}
		if closed {
			// Nothing will ever be appended, so waiting on the tail would hang a
			// client that only wanted the history of a removed session.
			return
		}
		for {
			select {
			case ev, ok := <-tail:
				if !ok {
					// Dropped for falling behind. The client reconnects with
					// Last-Event-ID and picks up from the log.
					return
				}
				if ev.Seq <= upto {
					continue // already delivered by the replay
				}
				select {
				case out <- ev:
				case <-done:
					return
				}
			case <-done:
				return
			}
		}
	}()

	return out, stop
}

func (s *Session) unsubscribe(id int64) {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	// Deleted without closing: the reader goroutine stops on its own done channel,
	// and closing here as well would race with the broadcast that closes dropped
	// subscribers.
	delete(s.subs, id)
}

// emit records an event and hands it to every subscriber.
func (s *Session) emit(ev Event) {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	if s.closed {
		return
	}

	stored, err := s.log.append(ev)
	if err != nil && stored.Seq == 0 {
		// The event was never numbered, so there is nothing a subscriber could
		// acknowledge. Dropping it beats broadcasting an event with id 0, which
		// would make a reconnect replay the entire session.
		return
	}

	for id, ch := range s.subs {
		select {
		case ch <- stored:
		default:
			// A subscriber that cannot keep up is cut loose, never waited for.
			// Blocking here would let a stalled browser tab stop the agent.
			close(ch)
			delete(s.subs, id)
		}
	}
}

// Prep emits a preparation progress message.
func (s *Session) Prep(msg string) { s.emit(Event{Kind: KindPrep, Text: msg}) }

func (s *Session) runPreparation() {
	defer s.work.Done()

	var err error
	if s.prepare != nil {
		err = s.prepare(s.ctx, s, s.Prep)
	}
	switch {
	case err != nil:
		s.fail(err)
		return
	case s.ctx.Err() != nil:
		s.fail(errors.New("cancelled before it was ready"))
		return
	}
	s.setState(StateReady)

	if s.initialPrompt == "" {
		return
	}
	// The question the session was created for. A failure to start it is recorded
	// and left there: the session is ready, so the client can simply ask again.
	if err := s.Send(s.initialPrompt); err != nil {
		s.emit(Event{Kind: KindError, Text: err.Error()})
	}
}

func (s *Session) fail(cause error) {
	s.mu.Lock()
	if s.state == StateClosed {
		s.mu.Unlock()
		return
	}
	s.state = StateFailed
	s.meta.State = StateFailed
	s.meta.Error = cause.Error()
	s.meta.UpdatedAt = time.Now().UTC()
	_ = s.persistLocked()
	s.mu.Unlock()

	s.emit(Event{Kind: KindError, Text: cause.Error()})
	s.emit(Event{Kind: KindState, State: StateFailed})
}

func (s *Session) setState(st State) {
	s.mu.Lock()
	// Closed is terminal. Close waits for the turn to unwind, and a turn unwinding
	// reports itself ready — without this it would resurrect the session it was
	// just torn down with, and the persisted metadata would outlive the directory.
	if s.state == st || s.state == StateClosed {
		s.mu.Unlock()
		return
	}
	s.state = st
	s.meta.State = st
	s.meta.UpdatedAt = time.Now().UTC()
	_ = s.persistLocked()
	s.mu.Unlock()

	s.emit(Event{Kind: KindState, State: st})
}

// Send starts a turn and returns as soon as it has started.
//
// The turn runs in a goroutine holding a context derived from the session, so the
// caller's request may end the moment this returns without ending the work.
func (s *Session) Send(prompt string) error {
	prompt = strings.TrimSpace(prompt)
	if prompt == "" {
		return errors.New("empty prompt")
	}
	if s.client == nil {
		return errors.New("no agent CLI configured")
	}

	s.mu.Lock()
	switch s.state {
	case StateReady:
	case StatePreparing:
		s.mu.Unlock()
		return ErrPreparing
	case StateRunning:
		s.mu.Unlock()
		return ErrBusy
	case StateFailed:
		s.mu.Unlock()
		return ErrFailed
	case StateClosed:
		s.mu.Unlock()
		return ErrClosed
	default:
		s.mu.Unlock()
		return fmt.Errorf("unexpected session state %q", s.state)
	}

	turnCtx, cancel := context.WithCancel(s.ctx)
	s.turnCancel = cancel
	s.state = StateRunning
	s.meta.State = StateRunning
	s.meta.UpdatedAt = time.Now().UTC()
	if s.meta.Title == "" {
		s.meta.Title = title(prompt)
	}
	_ = s.persistLocked()
	cliSessionID := s.cliSessionID
	// Counted before the lock is released, not inside the goroutine: a Close that
	// arrives in between would otherwise see no work pending, return, and let a
	// turn start on a session it had already torn down.
	s.work.Add(1)
	s.mu.Unlock()

	s.emit(Event{Kind: KindState, State: StateRunning})
	s.emit(Event{Kind: KindPrompt, Text: prompt})

	go s.runTurn(turnCtx, cancel, cliSessionID, prompt)
	return nil
}

func (s *Session) runTurn(ctx context.Context, cancel context.CancelFunc, cliSessionID, prompt string) {
	defer s.work.Done()
	defer cancel()

	req := ai.StreamRequest{
		SystemPrompt: systemPrompt,
		UserPrompt:   prompt,
		SessionID:    cliSessionID,
		WorkDir:      s.WorkspaceDir(),
		AllowTools:   true,
	}

	res, err := s.client.CompleteStream(ctx, req, func(ev ai.Event) {
		if out, ok := eventFromAI(ev); ok {
			s.emit(out)
		}
	})

	if res != nil && res.SessionID != "" && res.SessionID != cliSessionID {
		s.mu.Lock()
		s.cliSessionID = res.SessionID
		s.mu.Unlock()
	}

	switch {
	case err != nil && ctx.Err() != nil:
		s.emit(Event{Kind: KindError, Text: "cancelled"})
	case err != nil:
		s.emit(Event{Kind: KindError, Text: err.Error()})
	}

	s.emit(Event{Kind: KindTurnDone})

	// A failed turn returns the session to ready rather than failing it. The
	// expensive part — the prepared project — is still valid, and the user can ask
	// again; the error is in the log either way. StateFailed is reserved for a
	// session that cannot work at all. A session closed underneath us stays closed,
	// which setState enforces.
	s.setState(StateReady)
}

// Cancel stops the turn in progress and leaves the session usable.
func (s *Session) Cancel() {
	s.mu.Lock()
	c := s.turnCancel
	s.mu.Unlock()
	if c != nil {
		c()
	}
}

// Close ends the session: it stops any work, stops every subscriber, and releases
// the log. It does not delete anything; see Manager.Remove.
func (s *Session) Close() error {
	s.mu.Lock()
	if s.state == StateClosed {
		s.mu.Unlock()
		return nil
	}
	s.state = StateClosed
	s.meta.State = StateClosed
	s.meta.UpdatedAt = time.Now().UTC()
	_ = s.persistLocked()
	s.mu.Unlock()

	s.cancel()
	s.waitForWork()
	s.emit(Event{Kind: KindState, State: StateClosed})

	s.subMu.Lock()
	s.closed = true
	for id, ch := range s.subs {
		close(ch)
		delete(s.subs, id)
	}
	s.subMu.Unlock()

	return s.log.close()
}

// waitForWork blocks until the preparation and any turn have stopped, or until the
// grace period expires.
func (s *Session) waitForWork() {
	done := make(chan struct{})
	go func() {
		s.work.Wait()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(closeGrace):
	}
}

func (s *Session) persist() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.persistLocked()
}

func (s *Session) persistLocked() error {
	data, err := json.MarshalIndent(s.meta, "", "  ")
	if err != nil {
		return fmt.Errorf("encoding session metadata: %w", err)
	}
	if err := os.WriteFile(filepath.Join(s.dir, metaFileName), data, 0o600); err != nil {
		return fmt.Errorf("writing session metadata: %w", err)
	}
	return nil
}

func loadMeta(path string) (Meta, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return Meta{}, fmt.Errorf("reading session metadata: %w", err)
	}
	var m Meta
	if err := json.Unmarshal(data, &m); err != nil {
		return Meta{}, fmt.Errorf("decoding session metadata: %w", err)
	}
	return m, nil
}

func newSessionID() string {
	entropy := ulid.Monotonic(rand.New(rand.NewSource(time.Now().UnixNano())), 0) //nolint:gosec // identifiers, not secrets
	return ulid.MustNew(ulid.Timestamp(time.Now()), entropy).String()
}

// validSessionID keeps a session ID usable as a single path segment.
//
// Every ID reaching this package arrives from a URL, so an ID is a path traversal
// waiting to happen: "../../.." names a directory that Remove would happily delete.
// ULIDs are 26 characters of Crockford base32, and anything else is not one.
func validSessionID(id string) bool {
	if len(id) != 26 {
		return false
	}
	for _, r := range id {
		switch {
		case r >= '0' && r <= '9', r >= 'A' && r <= 'Z':
		default:
			return false
		}
	}
	return true
}

func title(prompt string) string {
	const max = 80
	t := strings.Join(strings.Fields(prompt), " ")
	if len(t) > max {
		return t[:max] + "…"
	}
	return t
}

// systemPrompt is intentionally short.
//
// The framework's mandate and installed Hub rules are not duplicated here. The
// adapter's native lifecycle hook composes them from the ephemeral project at
// agent start; skills remain host-discoverable files loaded only when needed.
const systemPrompt = `You are answering a question inside a workspace prepared for exactly this purpose.

The workspace contains the documentation wikis and code graphs that were selected for
this search, indexed and reachable through your tools. The workspace lifecycle hook
has supplied the framework's resident instructions and its skills are available on demand.

Ground your answer in what you find there. Say plainly when something is not covered
by the material available to you, rather than filling the gap from general knowledge.`
