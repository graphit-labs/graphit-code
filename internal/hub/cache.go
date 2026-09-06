package hub

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
)

const (
	metadataCacheTTL      = 5 * time.Minute
	metadataCacheMaxBytes = 64 << 20
)

type metadataCache struct {
	root     string
	ttl      time.Duration
	maxBytes int64
	now      func() time.Time
}

type cachedObject struct {
	Key      string    `json:"key"`
	ETag     string    `json:"etag,omitempty"`
	StoredAt time.Time `json:"stored_at"`
	Data     []byte    `json:"data"`
}

func newMetadataCache(cacheRoot string, cfg config.S3Config, subject hubaccess.Subject) *metadataCache {
	return &metadataCache{
		root:     filepath.Join(cacheRoot, "cache", hubFingerprint(cfg), subjectFingerprint(subject)),
		ttl:      metadataCacheTTL,
		maxBytes: metadataCacheMaxBytes,
		now:      time.Now,
	}
}

func hubFingerprint(cfg config.S3Config) string {
	return fingerprint(strings.Join([]string{cfg.Endpoint, cfg.Bucket, cfg.Prefix}, "\x00"))
}

func subjectFingerprint(subject hubaccess.Subject) string {
	return fingerprint(subject.UserID + "\x00" + strings.Join(subject.TeamIDs, "\x00"))
}

func fingerprint(value string) string {
	sum := sha256.Sum256([]byte(value))
	return hex.EncodeToString(sum[:16])
}

func (c *metadataCache) GetFresh(key string) ([]byte, string, bool) {
	data, err := os.ReadFile(c.path(key))
	if err != nil {
		return nil, "", false
	}
	var object cachedObject
	if json.Unmarshal(data, &object) != nil || object.Key != key || c.now().Sub(object.StoredAt) > c.ttl {
		return nil, "", false
	}
	return object.Data, object.ETag, true
}

func (c *metadataCache) Put(key string, data []byte, etag string) error {
	object := cachedObject{Key: key, ETag: etag, StoredAt: c.now().UTC(), Data: append([]byte(nil), data...)}
	raw, err := json.Marshal(object)
	if err != nil {
		return err
	}
	path := c.path(key)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".cache-*")
	if err != nil {
		return err
	}
	tmpPath := tmp.Name()
	defer os.Remove(tmpPath)
	if err := tmp.Chmod(0o600); err != nil {
		_ = tmp.Close()
		return err
	}
	if _, err := tmp.Write(raw); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return err
	}
	return c.enforceBound()
}

func (c *metadataCache) path(key string) string {
	return filepath.Join(c.root, fingerprint(key)+".json")
}

func (c *metadataCache) enforceBound() error {
	entries, err := os.ReadDir(c.root)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	type item struct {
		path string
		size int64
		at   time.Time
	}
	items := make([]item, 0, len(entries))
	var total int64
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		info, err := entry.Info()
		if err != nil {
			continue
		}
		total += info.Size()
		items = append(items, item{path: filepath.Join(c.root, entry.Name()), size: info.Size(), at: info.ModTime()})
	}
	sort.Slice(items, func(i, j int) bool { return items[i].at.Before(items[j].at) })
	for _, item := range items {
		if total <= c.maxBytes {
			break
		}
		if err := os.Remove(item.path); err == nil {
			total -= item.size
		}
	}
	return nil
}
