package task

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/oklog/ulid/v2"
)

var ErrSessionNotFound = errors.New("task session not found")

func sessionSpec(v Session) SessionSpec {
	return SessionSpec{Title: v.Title, Description: v.Description, Strategy: v.Strategy}
}
func sessionSummary(v Session) SessionSummary {
	return SessionSummary{ID: v.ID, Title: v.Title, Status: v.Status, Owner: v.Owner, ProgressSummary: v.ProgressSummary, NextStep: v.NextStep, UpdatedAt: v.UpdatedAt, Revision: v.Revision}
}
func sessionActive(v Session) bool { return v.Status == StatusOpen || v.Status == StatusInProgress }
func clearSessionClaim(v *Session) {
	v.Owner = ""
	v.ClaimToken = ""
	v.ClaimedAt = ""
	v.LeaseExpiresAt = ""
	v.HeartbeatAt = ""
}
func sessionEvent(v Session, kind, actor string, from Status, summary string) SessionEvent {
	return SessionEvent{Key: fmt.Sprintf("%s/%020d", v.ID, v.Revision), SessionID: v.ID, Revision: v.Revision, Type: kind, Actor: actor, At: v.UpdatedAt, FromStatus: from, ToStatus: v.Status, Summary: summary, NextStep: v.NextStep}
}
func (s *Service) sessionChange(before Session, kind, actor, summary string) Session {
	next := before
	next.Revision++
	next.UpdatedAt = stamp(s.now().UTC())
	next.LastEvent = sessionEvent(next, kind, actor, before.Status, summary)
	return next
}
func (s *Service) SessionCreate(ctx context.Context, in SessionCreateInput) (Session, error) {
	in.Title = strings.TrimSpace(in.Title)
	in.Description = strings.TrimSpace(in.Description)
	in.Strategy = strings.TrimSpace(in.Strategy)
	in.Actor = strings.TrimSpace(in.Actor)
	if in.Title == "" || in.Description == "" || in.Strategy == "" || in.Actor == "" {
		return Session{}, errors.New("session title, detailed description, strategy and agent id are required")
	}
	key := strings.TrimSpace(in.IdempotencyKey)
	if key == "" {
		key = canonicalKey(in.Title)
	}
	var out Session
	err := s.withLock(ctx, in.Actor, func(t *tables) error {
		if err := t.ensureIndexes(ctx); err != nil {
			return err
		}
		all, err := t.allSessions(ctx)
		if err != nil {
			return err
		}
		byID := map[string]Session{}
		for _, v := range all {
			if v.IdempotencyKey == key {
				v.ClaimToken = ""
				out = v
				return nil
			}
			byID[v.ID] = v
		}
		digest := taskIDDigest(s.projectID, "session:"+key)
		id := ""
		for n := 4; n <= len(digest); n++ {
			candidate := "ses-" + digest[:n]
			if _, exists := byID[candidate]; !exists {
				id = candidate
				break
			}
		}
		if id == "" {
			return errors.New("session id collision")
		}
		now := stamp(s.now().UTC())
		out = Session{ID: id, ProjectID: s.projectID, IdempotencyKey: key, Title: in.Title, Description: in.Description, Strategy: in.Strategy, Status: StatusOpen, CreatedAt: now, UpdatedAt: now, Revision: 1}
		out.LastEvent = sessionEvent(out, "created", in.Actor, "", "session created")
		out.LastEvent.SpecRevision = &SessionSpecRevision{Key: out.LastEvent.Key, SessionID: id, SourceRevision: 1, Actor: in.Actor, Reason: "session created", At: now, After: sessionSpec(out)}
		return s.putSessionCAS(ctx, t, Session{}, &out)
	})
	return out, err
}

func (s *Service) SessionClaim(ctx context.Context, id, actor string, lease time.Duration) (Session, error) {
	actor = strings.TrimSpace(actor)
	if actor == "" {
		return Session{}, errors.New("agent id is required")
	}
	if lease <= 0 {
		lease = DefaultLease
	}
	var out Session
	err := s.withLock(ctx, actor, func(t *tables) error {
		if err := s.reconcileSessionsLocked(ctx, t, actor); err != nil {
			return err
		}
		current, ok, err := t.getSession(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrSessionNotFound
		}
		if current.Status == StatusInProgress {
			if current.Owner == actor {
				out = current
				return nil
			}
			return ErrClaimed
		}
		if current.Status != StatusOpen {
			return fmt.Errorf("session %s is %s", id, current.Status)
		}
		all, err := t.allSessions(ctx)
		if err != nil {
			return err
		}
		for _, v := range all {
			if v.Status == StatusInProgress && v.Owner == actor && v.LeaseExpiresAt > stamp(s.now().UTC()) {
				return fmt.Errorf("agent %q already coordinates session %s", actor, v.ID)
			}
		}
		next := s.sessionChange(current, "claimed", actor, "session claimed")
		next.Status = StatusInProgress
		next.Owner = actor
		next.ClaimToken = ulid.Make().String()
		next.ClaimEpoch++
		next.ClaimedAt = stamp(s.now().UTC())
		next.HeartbeatAt = next.ClaimedAt
		next.LeaseExpiresAt = stamp(s.now().UTC().Add(lease))
		next.LastEvent.ToStatus = next.Status
		if err := s.putSessionCAS(ctx, t, current, &next); err != nil {
			return err
		}
		out = next
		return nil
	})
	return out, err
}

func (s *Service) SessionForceTakeover(ctx context.Context, id, actor string, in ForceTakeoverInput, lease time.Duration) (Session, error) {
	if strings.TrimSpace(actor) == "" || in.ConfirmID != id || in.ExpectedRevision < 1 || strings.TrimSpace(in.Reason) == "" || lease <= 0 {
		return Session{}, errors.New("session takeover requires agent, exact confirm_id, expected_revision, reason and positive lease")
	}
	var out Session
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getSession(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrSessionNotFound
		}
		if current.Revision != in.ExpectedRevision {
			return ErrConcurrent
		}
		if current.Status != StatusInProgress {
			return errors.New("only an in-progress session can be forcibly taken over")
		}
		all, err := t.allSessions(ctx)
		if err != nil {
			return err
		}
		for _, v := range all {
			if v.ID != id && v.Status == StatusInProgress && v.Owner == actor && v.LeaseExpiresAt > stamp(s.now().UTC()) {
				return fmt.Errorf("agent already coordinates session %s", v.ID)
			}
		}
		next := s.sessionChange(current, "force_takeover", actor, in.Reason)
		next.Owner = actor
		next.ClaimToken = ulid.Make().String()
		next.ClaimEpoch++
		next.ClaimedAt = stamp(s.now().UTC())
		next.HeartbeatAt = next.ClaimedAt
		next.LeaseExpiresAt = stamp(s.now().UTC().Add(lease))
		if err := s.putSessionCAS(ctx, t, current, &next); err != nil {
			return err
		}
		out = next
		return nil
	})
	return out, err
}

func (s *Service) mutateSession(ctx context.Context, id, token, actor string, lease time.Duration, kind, summary string, apply func(*tables, Session, *Session) error) (Session, error) {
	var out Session
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getSession(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrSessionNotFound
		}
		if actor == "" || current.Status != StatusInProgress || token == "" || current.ClaimToken != token || current.Owner != actor || current.LeaseExpiresAt <= stamp(s.now().UTC()) {
			return ErrFence
		}
		next := s.sessionChange(current, kind, actor, summary)
		if err := apply(t, current, &next); err != nil {
			return err
		}
		if next.Status == StatusInProgress {
			if lease <= 0 {
				lease = DefaultLease
			}
			next.HeartbeatAt = stamp(s.now().UTC())
			next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, s.now().UTC(), lease)
		}
		next.LastEvent.ToStatus = next.Status
		next.LastEvent.NextStep = next.NextStep
		if err := s.putSessionCAS(ctx, t, current, &next); err != nil {
			return err
		}
		out = next
		return nil
	})
	return out, err
}

func (s *Service) SessionRevise(ctx context.Context, id, token, actor string, in SessionReviseInput, lease time.Duration) (Session, error) {
	if in.ExpectedRevision < 1 || strings.TrimSpace(in.Reason) == "" {
		return Session{}, errors.New("expected_revision and revision reason are required")
	}
	return s.mutateSession(ctx, id, token, actor, lease, "revised", in.Reason, func(_ *tables, before Session, next *Session) error {
		if before.Revision != in.ExpectedRevision {
			return ErrConcurrent
		}
		for _, f := range []struct {
			src  *string
			dest *string
		}{{in.Title, &next.Title}, {in.Description, &next.Description}, {in.Strategy, &next.Strategy}} {
			if f.src != nil {
				*f.dest = strings.TrimSpace(*f.src)
				if *f.dest == "" {
					return errors.New("session title, description and strategy cannot be empty")
				}
			}
		}
		if sessionSpec(before) == sessionSpec(*next) {
			return errors.New("session revision has no specification changes")
		}
		next.LastEvent.SpecRevision = &SessionSpecRevision{Key: next.LastEvent.Key, SessionID: id, SourceRevision: next.Revision, Actor: actor, Reason: in.Reason, At: next.UpdatedAt, Before: sessionSpec(before), After: sessionSpec(*next)}
		return nil
	})
}

func addSessionCheckpoint(next *Session, actor string, in SessionCheckpointInput) {
	next.CheckpointSequence++
	next.ProgressSummary = in.Summary
	next.NextStep = in.NextStep
	next.LastEvent.Checkpoint = &SessionCheckpointRecord{Key: next.LastEvent.Key, SessionID: next.ID, Sequence: next.CheckpointSequence, Revision: next.Revision, Actor: actor, At: next.UpdatedAt, Summary: in.Summary, Problems: in.Problems, Decisions: in.Decisions, Strategy: in.Strategy, NextStep: in.NextStep}
}
func (s *Service) SessionCheckpoint(ctx context.Context, id, token, actor string, in SessionCheckpointInput, lease time.Duration) (Session, error) {
	in.Summary = strings.TrimSpace(in.Summary)
	in.NextStep = strings.TrimSpace(in.NextStep)
	if in.Summary == "" || in.NextStep == "" {
		return Session{}, errors.New("session checkpoint summary and next_step are required")
	}
	return s.mutateSession(ctx, id, token, actor, lease, "checkpoint", in.Summary, func(_ *tables, _ Session, next *Session) error { addSessionCheckpoint(next, actor, in); return nil })
}
func (s *Service) SessionHeartbeat(ctx context.Context, id, token, actor string, lease time.Duration) (Session, error) {
	return s.mutateSession(ctx, id, token, actor, lease, "heartbeat", "session lease renewed", func(_ *tables, _ Session, _ *Session) error { return nil })
}
func (s *Service) SessionRelease(ctx context.Context, id, token, actor, summary, nextStep string) (Session, error) {
	summary = strings.TrimSpace(summary)
	nextStep = strings.TrimSpace(nextStep)
	if summary == "" || nextStep == "" {
		return Session{}, errors.New("session release requires descriptive summary and next_step")
	}
	return s.mutateSession(ctx, id, token, actor, 0, "released", summary, func(_ *tables, _ Session, next *Session) error {
		addSessionCheckpoint(next, actor, SessionCheckpointInput{Summary: summary, NextStep: nextStep})
		next.Status = StatusOpen
		clearSessionClaim(next)
		return nil
	})
}
func (s *Service) closeSession(ctx context.Context, id, token, actor, summary string, status Status) (Session, error) {
	summary = strings.TrimSpace(summary)
	if summary == "" {
		return Session{}, errors.New("session closure requires a final summary or cancellation reason")
	}
	return s.mutateSession(ctx, id, token, actor, 0, string(status), summary, func(t *tables, _ Session, next *Session) error {
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		var pending []string
		for _, v := range all {
			if v.SessionID != id {
				continue
			}
			if v.Status != StatusCompleted && v.Status != StatusCancelled {
				pending = append(pending, v.ID)
			} else if v.Status == StatusCompleted && (v.Flagged || len(completionViolations(v, all)) > 0) {
				pending = append(pending, v.ID)
			}
		}
		if len(pending) > 0 {
			return fmt.Errorf("session %s has unresolved tasks: %s", id, strings.Join(pending, ", "))
		}
		next.Status = status
		next.ProgressSummary = summary
		next.NextStep = ""
		next.CompletedAt = next.UpdatedAt
		next.CompletedBy = actor
		clearSessionClaim(next)
		return nil
	})
}
func (s *Service) SessionComplete(ctx context.Context, id, token, actor, summary string) (Session, error) {
	return s.closeSession(ctx, id, token, actor, summary, StatusCompleted)
}
func (s *Service) SessionCancel(ctx context.Context, id, token, actor, reason string) (Session, error) {
	return s.closeSession(ctx, id, token, actor, reason, StatusCancelled)
}

func (s *Service) SessionGet(ctx context.Context, id string) (SessionDetail, error) {
	var out SessionDetail
	err := s.withTables(ctx, func(t *tables) error {
		v, ok, err := t.getSession(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrSessionNotFound
		}
		detail, err := sessionDetailFromTables(ctx, t, v)
		if err != nil {
			return err
		}
		out = detail
		return nil
	})
	return out, err
}
func sessionDetailFromTables(ctx context.Context, t *tables, v Session) (SessionDetail, error) {
	rows, err := t.sessionHistory(ctx, t.sessionEvents, v.ID)
	if err != nil {
		return SessionDetail{}, err
	}
	out := SessionDetail{Events: []SessionEvent{}, Checkpoints: []SessionCheckpointRecord{}, SpecRevisions: []SessionSpecRevision{}, Tasks: []CatalogItem{}}
	seen := map[string]bool{}
	for _, row := range rows {
		var e SessionEvent
		if err := json.Unmarshal([]byte(text(row, "body_json")), &e); err != nil {
			return out, err
		}
		out.Events = append(out.Events, e)
		seen[e.Key] = true
	}
	if v.LastEvent.Key != "" && !seen[v.LastEvent.Key] {
		out.Events = append(out.Events, v.LastEvent)
	}
	sort.Slice(out.Events, func(i, j int) bool { return out.Events[i].Revision < out.Events[j].Revision })
	for _, e := range out.Events {
		if e.Checkpoint != nil {
			out.Checkpoints = append(out.Checkpoints, *e.Checkpoint)
		}
		if e.SpecRevision != nil {
			out.SpecRevisions = append(out.SpecRevisions, *e.SpecRevision)
		}
	}
	all, err := t.allTasks(ctx)
	if err != nil {
		return out, err
	}
	byID := indexTasks(all)
	for _, task := range all {
		if task.SessionID == v.ID {
			decorate(&task, byID)
			out.Tasks = append(out.Tasks, CatalogItem{ID: task.ID, SessionID: v.ID, Title: task.Title, Type: task.Type, Status: task.Status, Priority: task.Priority, Owner: task.Owner, Flagged: task.Flagged, Ready: task.Ready, BlockedBy: task.BlockedBy, UpdatedAt: task.UpdatedAt})
		}
	}
	v.ClaimToken = ""
	out.Session = v
	return out, nil
}
func (s *Service) SessionList(ctx context.Context, opts SessionListOptions) ([]SessionSummary, error) {
	if opts.Status != "" && !ValidStatus(opts.Status) {
		return nil, fmt.Errorf("invalid session status %q", opts.Status)
	}
	out := []SessionSummary{}
	err := s.withTables(ctx, func(t *tables) error {
		all, err := t.allSessions(ctx)
		if err != nil {
			return err
		}
		for _, v := range all {
			if opts.Status != "" && string(v.Status) != opts.Status || opts.Owner != "" && v.Owner != opts.Owner || opts.Active && !sessionActive(v) {
				continue
			}
			out = append(out, sessionSummary(v))
		}
		return nil
	})
	sort.Slice(out, func(i, j int) bool {
		if out[i].UpdatedAt == out[j].UpdatedAt {
			return out[i].ID < out[j].ID
		}
		return out[i].UpdatedAt > out[j].UpdatedAt
	})
	return out, err
}
func (s *Service) SessionSearch(ctx context.Context, query string, limit int) ([]SessionSearchResult, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, errors.New("session search query is required")
	}
	if limit <= 0 {
		limit = 20
	}
	out := []SessionSearchResult{}
	err := s.withTables(ctx, func(t *tables) error {
		all, err := t.allSessions(ctx)
		if err != nil {
			return err
		}
		byID := map[string]Session{}
		for _, v := range all {
			byID[v.ID] = v
		}
		scores := map[string]float64{}
		for _, table := range []*lancestore.Table{t.sessions, t.sessionCheckpoints, t.sessionRevisions, t.sessionEvents} {
			for offset := 0; ; offset += pageSize {
				hits, err := table.Search(ctx, lancestore.Query{Text: query, TextColumn: "search_text", Limit: pageSize, Offset: offset})
				if err != nil {
					return err
				}
				for _, hit := range hits {
					id := text(hit.Row, "session_id")
					if table == t.sessions {
						id = text(hit.Row, "id")
					}
					if _, exists := byID[id]; exists {
						if score, ok := scores[id]; !ok || hit.Score > score {
							scores[id] = hit.Score
						}
					}
				}
				if len(hits) < pageSize {
					break
				}
			}
		}
		// The latest authoritative event remains discoverable if its projections failed.
		for _, v := range all {
			e := v.LastEvent
			body, _ := json.Marshal(e)
			if strings.Contains(strings.ToLower(string(body)), strings.ToLower(query)) {
				if _, ok := scores[v.ID]; !ok {
					scores[v.ID] = 0
				}
			}
		}
		for id, score := range scores {
			out = append(out, SessionSearchResult{SessionSummary: sessionSummary(byID[id]), Score: score})
		}
		return nil
	})
	sort.Slice(out, func(i, j int) bool {
		if out[i].Score == out[j].Score {
			return out[i].ID < out[j].ID
		}
		return out[i].Score > out[j].Score
	})
	if len(out) > limit {
		out = out[:limit]
	}
	return out, err
}

func (s *Service) reconcileSessionsLocked(ctx context.Context, t *tables, actor string) error {
	all, err := t.allSessions(ctx)
	if err != nil {
		return err
	}
	for _, v := range all {
		if err := s.projectSession(ctx, t, v); err != nil {
			return err
		}
		if v.Status == StatusInProgress && v.LeaseExpiresAt <= stamp(s.now().UTC()) {
			next := s.sessionChange(v, "lease_expired", "system", "session lease expired; resume from saved intent and checkpoints")
			next.Status = StatusOpen
			clearSessionClaim(&next)
			next.LastEvent.ToStatus = next.Status
			if err := s.putSessionCAS(ctx, t, v, &next); err != nil {
				return err
			}
		}
	}
	return nil
}
func (s *Service) heartbeatOwnedSessionsLocked(ctx context.Context, t *tables, actor string, lease time.Duration) ([]Session, error) {
	out := []Session{}
	if strings.TrimSpace(actor) == "" {
		return out, nil
	}
	if lease <= 0 {
		lease = DefaultLease
	}
	all, err := t.allSessions(ctx)
	if err != nil {
		return nil, err
	}
	for _, v := range all {
		if v.Status != StatusInProgress || v.Owner != actor || v.LeaseExpiresAt <= stamp(s.now().UTC()) {
			continue
		}
		next := s.sessionChange(v, "heartbeat", actor, "session lease renewed by hook")
		next.HeartbeatAt = stamp(s.now().UTC())
		next.LeaseExpiresAt = renewedLeaseExpiry(v.LeaseExpiresAt, s.now().UTC(), lease)
		if err := s.putSessionCAS(ctx, t, v, &next); err != nil {
			return nil, err
		}
		next.ClaimToken = ""
		out = append(out, next)
	}
	return out, nil
}
func (s *Service) releaseOwnedSessionsLocked(ctx context.Context, t *tables, actor, summary, nextStep string) ([]Session, error) {
	out := []Session{}
	if strings.TrimSpace(actor) == "" {
		return out, nil
	}
	all, err := t.allSessions(ctx)
	if err != nil {
		return nil, err
	}
	for _, v := range all {
		if v.Status != StatusInProgress || v.Owner != actor {
			continue
		}
		next := s.sessionChange(v, "released", actor, "hook released session; substantive checkpoint preserved")
		if summary != "" && nextStep != "" {
			addSessionCheckpoint(&next, actor, SessionCheckpointInput{Summary: summary, NextStep: nextStep})
			next.LastEvent.Summary = summary
			next.LastEvent.NextStep = nextStep
		}
		next.Status = StatusOpen
		clearSessionClaim(&next)
		next.LastEvent.ToStatus = next.Status
		if err := s.putSessionCAS(ctx, t, v, &next); err != nil {
			return nil, err
		}
		out = append(out, next)
	}
	return out, nil
}

func (s *Service) resolveTaskSession(ctx context.Context, t *tables, in CreateInput) (string, error) {
	id := strings.TrimSpace(in.SessionID)
	if in.ParentID != "" {
		parent, ok, err := t.getTask(ctx, in.ParentID)
		if err != nil {
			return "", err
		}
		if !ok {
			return "", ErrNotFound
		}
		if id == "" {
			id = parent.SessionID
		} else if id != parent.SessionID {
			return "", errors.New("parent and child must belong to the same session")
		}
	}
	if id == "" && in.ParentID == "" && strings.TrimSpace(in.Actor) != "" {
		all, err := t.allSessions(ctx)
		if err != nil {
			return "", err
		}
		for _, v := range all {
			if v.Status == StatusInProgress && v.Owner == in.Actor && v.LeaseExpiresAt > stamp(s.now().UTC()) {
				if id != "" {
					return "", errors.New("multiple coordinated sessions; pass session_id")
				}
				id = v.ID
			}
		}
	}
	if id == "" {
		if in.RequireSession {
			return "", errors.New("agent tasks require a session: create/claim a task session or pass session_id")
		}
		return "", nil
	}
	v, ok, err := t.getSession(ctx, id)
	if err != nil {
		return "", err
	}
	if !ok {
		return "", ErrSessionNotFound
	}
	if !sessionActive(v) {
		return "", fmt.Errorf("session %s is %s", id, v.Status)
	}
	return id, nil
}
