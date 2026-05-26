package hub

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/oklog/ulid/v2"
)

type EventTracker struct {
	gitStore *GitStore
}

func NewEventTracker(gs *GitStore) *EventTracker {
	return &EventTracker{gitStore: gs}
}

func (t *EventTracker) TrackEvent(
	action string,
	projectID string,
	artifact map[string]string,
	extraCtx map[string]string,
) {
	if t == nil || t.gitStore == nil {
		return
	}

	payload := map[string]any{
		"timestamp": time.Now().UTC().Format(time.RFC3339),
		"action":    action,
	}
	if projectID != "" {
		if h := computeProjectHash(projectID); h != "" {
			payload["project_hash"] = h
		}
	}
	if len(artifact) > 0 {
		anon := make(map[string]string)
		if v, ok := artifact["type"]; ok {
			anon["type"] = v
		}
		if v, ok := artifact["version"]; ok {
			anon["version"] = v
		}
		if len(anon) > 0 {
			payload["artifact"] = anon
		}
	}
	for k, v := range extraCtx {
		if v != "" {
			payload[k] = v
		}
	}

	body, err := json.MarshalIndent(payload, "", "  ")
	if err != nil {
		return
	}

	key := buildEventKey(action, artifact)
	t.gitStore.WriteEventFile(key, body)
}

func buildEventKey(action string, artifact map[string]string) string {
	ulid := generateULID()
	shortAction := action
	if i := strings.LastIndex(action, "."); i >= 0 {
		shortAction = action[i+1:]
	}

	if artifact != nil {
		artType := artifact["type"]
		if artType == "" {
			artType = "unknown"
		}
		return fmt.Sprintf("%s/%s_%s.json", artType, ulid, shortAction)
	}

	return fmt.Sprintf("%s_%s.json", ulid, shortAction)
}

const crockfordAlphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

func generateULID() string {
	nowMs := uint64(time.Now().UnixMilli())

	var b [10]byte
	rand.Read(b[:]) //nolint:errcheck

	ts := make([]byte, 10)
	for i := 9; i >= 0; i-- {
		ts[i] = crockfordAlphabet[nowMs&0x1F]
		nowMs >>= 5
	}

	rnd := make([]byte, 16)
	v := binary.BigEndian.Uint64(b[:8])
	v2 := binary.BigEndian.Uint16(b[8:])
	combined := (v << 16) | uint64(v2)
	for i := 15; i >= 0; i-- {
		rnd[i] = crockfordAlphabet[combined&0x1F]
		combined >>= 5
	}

	return string(ts) + string(rnd)
}

const clientSecretFile = "client_secret"
const clientIDFile = "client_id"

var (
	clientSecretOnce  sync.Once
	clientSecretCache string

	clientIDOnce  sync.Once
	clientIDCache string
)

func getOrCreateClientSecret() string {
	clientSecretOnce.Do(func() {
		clientSecretCache = readOrGenerateConfig("client.secret")
	})
	return clientSecretCache
}

func getOrCreateClientID() string {
	clientIDOnce.Do(func() {
		clientIDCache = readOrGenerateConfig("client.id")
	})
	return clientIDCache
}

func readOrGenerateConfig(key string) string {
	val, ok, err := config.GetGlobalConfigValue(key)
	if err == nil && ok && val != "" {
		return strings.TrimSpace(val)
	}

	result := ulid.Make().String()

	_ = config.SetGlobalConfigValue(key, result)
	return result
}

func computeProjectHash(projectID string) string {
	secret := getOrCreateClientSecret()
	if secret == "" {
		return ""
	}
	h := sha256.Sum256([]byte(projectID + secret))
	return hex.EncodeToString(h[:])
}
