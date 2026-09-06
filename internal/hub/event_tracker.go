package hub

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/oklog/ulid/v2"
)

type EventTracker struct {
	store *S3Store
}

func NewEventTracker(st *S3Store) *EventTracker {
	return &EventTracker{store: st}
}

func (t *EventTracker) TrackEvent(
	ctx context.Context,
	action string,
	projectID string,
	artifact map[string]string,
	extraCtx map[string]string,
) {
	if t == nil || t.store == nil {
		return
	}
	if hubaccess.ValidateProjectID(projectID) != nil {
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

	artifactType := ""
	if artifact != nil {
		artifactType = artifact["type"]
	}
	key := EventKey(projectID, artifactType, action, time.Now(), generateULID())
	t.store.WriteEventFile(ctx, projectID, key, body)
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
