package task

import (
	"context"
	"encoding/json"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

// AgentIDForSession is the stable owner identity shared by MCP and native
// lifecycle hooks. The unit distinguishes installations; the host session
// distinguishes concurrent agents on one installation.
func AgentIDForSession(sessionID string) string {
	unit, err := config.UnitID()
	if err != nil {
		unit = "unknown-unit"
	}
	if strings.TrimSpace(sessionID) == "" {
		return unit
	}
	return unit + ":" + strings.TrimSpace(sessionID)
}

// AgentIDFromHook extracts the identity fields exposed by supported adapters.
// Unknown payloads deliberately return empty: releasing every task on a project
// would be less safe than allowing one lease to expire.
func AgentIDFromHook(input []byte) string {
	var value any
	if json.Unmarshal(input, &value) != nil {
		return ""
	}
	keys := []string{"agent_id", "agentId", "session_id", "sessionId", "sessionID", "conversation_id", "conversationId", "thread_id", "threadId", "threadID"}
	var find func(any, string) string
	find = func(v any, wanted string) string {
		switch x := v.(type) {
		case map[string]any:
			if text, ok := x[wanted].(string); ok && strings.TrimSpace(text) != "" {
				return text
			}
			childKeys := make([]string, 0, len(x))
			for key := range x {
				childKeys = append(childKeys, key)
			}
			sort.Strings(childKeys)
			for _, key := range childKeys {
				if found := find(x[key], wanted); found != "" {
					return found
				}
			}
		case []any:
			for _, child := range x {
				if found := find(child, wanted); found != "" {
					return found
				}
			}
		}
		return ""
	}
	var session string
	for _, key := range keys {
		if session = find(value, key); session != "" {
			break
		}
	}
	if session == "" {
		return ""
	}
	return AgentIDForSession(session)
}

// Reconcile repairs Task/Session projections and expires stale claims. It is
// safe at every lifecycle boundary and idempotent when nothing changed.
func (s *Service) Reconcile(ctx context.Context) error {
	return s.withTables(ctx, func(t *tables) error {
		if err := t.ensureIndexes(ctx); err != nil {
			return err
		}
		if err := t.refresh(ctx); err != nil {
			return err
		}
		if err := s.reconcileLocked(ctx, t, "hook"); err != nil {
			return err
		}
		return s.reconcileSessionsLocked(ctx, t, "hook")
	})
}

// HeartbeatOwned renews the independent Task and coordinator Session claims of
// one agent. Post-tool hooks never renew the coordinator for an unrelated worker;
// explicit progress still requires the corresponding claim's fencing token.
func (s *Service) HeartbeatOwned(ctx context.Context, actor string, lease time.Duration) error {
	if actor == "" {
		return s.Reconcile(ctx)
	}
	if lease <= 0 {
		lease = DefaultLease
	}
	return s.withLockFast(ctx, actor, func(t *tables) error {
		if _, err := s.heartbeatOwnedSessionsLocked(ctx, t, actor, lease); err != nil {
			return err
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		for _, current := range all {
			if current.Status != StatusInProgress || current.Owner != actor {
				continue
			}
			next := current
			now := s.now().UTC()
			next.HeartbeatAt = stamp(now)
			next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
			next.UpdatedAt = stamp(now)
			next.Revision++
			next.LastEvent = newEvent(next, "heartbeat", actor, current.Status, next.Status, next.ProgressSummary, next.NextStep)
			if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
				return err
			}
			return s.projectTask(ctx, t, next, actor)
		}
		return nil
	})
}

// ReleaseOwned makes a stopped agent's Task and coordinator Session claimable,
// preserving descriptive checkpoints for takeover. A stop never completes work.
func (s *Service) ReleaseOwned(ctx context.Context, actor string) error {
	if actor == "" {
		return s.Reconcile(ctx)
	}
	return s.withLockFast(ctx, actor, func(t *tables) error {
		// A host stop is a lifecycle event, never a semantic checkpoint or completion.
		if _, err := s.releaseOwnedSessionsLocked(ctx, t, actor, "", ""); err != nil {
			return err
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		for _, current := range all {
			if current.Status != StatusInProgress || current.Owner != actor {
				continue
			}
			next := current
			next.Status = StatusOpen
			clearClaim(&next)
			next.UpdatedAt = stamp(s.now().UTC())
			next.Revision++
			next.LastEvent = newEvent(next, "agent_stopped", actor, current.Status, next.Status, next.ProgressSummary, next.NextStep)
			if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
				return err
			}
			if err := s.projectTask(ctx, t, next, actor); err != nil {
				return err
			}
		}
		return nil
	})
}
