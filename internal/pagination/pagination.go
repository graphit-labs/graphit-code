package pagination

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
)

const (
	DefaultPageSize = 20
	MaxPageSize     = 100
)

// Spec describes a paginated result set. Total is the caller's independent total cap;
// zero means unlimited. Bind must contain every input that changes result identity or order.
type Spec struct {
	PageSize        int
	Cursor          string
	Total           int
	DefaultPageSize int
	Bind            any
}

// Window is the prefix a ranked source must produce. FetchLimit includes one look-ahead row.
type Window struct {
	Offset      int
	PageSize    int
	FetchLimit  int
	total       int
	fingerprint string
}

// Page is the stable public response envelope.
type Page[T any] struct {
	Results    []T    `json:"results"`
	NextCursor string `json:"next_cursor"`
}

type cursorPayload struct {
	Version     int    `json:"v"`
	Offset      int    `json:"o"`
	Fingerprint string `json:"f"`
}

// Open validates a request and calculates the deterministic prefix required to answer it.
func Open(spec Spec) (Window, error) {
	if spec.Total < 0 {
		return Window{}, errors.New("top_k/limit cannot be negative")
	}
	pageSize := spec.PageSize
	if pageSize < 0 {
		return Window{}, errors.New("page_size cannot be negative")
	}
	if pageSize == 0 {
		pageSize = spec.DefaultPageSize
		if pageSize <= 0 {
			pageSize = DefaultPageSize
		}
		if pageSize > MaxPageSize {
			pageSize = MaxPageSize
		}
		if spec.Total > 0 && spec.Total < pageSize {
			pageSize = spec.Total
		}
	}
	if pageSize > MaxPageSize {
		return Window{}, fmt.Errorf("page_size cannot exceed %d", MaxPageSize)
	}

	fingerprint, err := fingerprint(spec.Bind, pageSize, spec.Total)
	if err != nil {
		return Window{}, err
	}
	offset := 0
	if spec.Cursor != "" {
		payload, err := decode(spec.Cursor)
		if err != nil {
			return Window{}, err
		}
		if payload.Fingerprint != fingerprint {
			return Window{}, errors.New("cursor does not belong to this query")
		}
		offset = payload.Offset
	}
	if spec.Total > 0 && offset >= spec.Total {
		return Window{}, errors.New("cursor is past the top_k/limit boundary")
	}
	maxInt := int(^uint(0) >> 1)
	if offset > maxInt-pageSize-1 {
		return Window{}, errors.New("pagination cursor offset is too large")
	}

	take := pageSize
	if spec.Total > 0 && take > spec.Total-offset {
		take = spec.Total - offset
	}
	fetch := offset + take
	if spec.Total == 0 || fetch < spec.Total {
		fetch++
	}
	return Window{Offset: offset, PageSize: take, FetchLimit: fetch, total: spec.Total, fingerprint: fingerprint}, nil
}

// Finish slices a deterministically ordered prefix and emits a cursor only when an extra row exists.
func Finish[T any](window Window, prefix []T) Page[T] {
	if window.Offset >= len(prefix) {
		return Page[T]{Results: []T{}}
	}
	end := window.Offset + window.PageSize
	if end > len(prefix) {
		end = len(prefix)
	}
	results := append([]T(nil), prefix[window.Offset:end]...)
	page := Page[T]{Results: results}
	if end < len(prefix) && (window.total == 0 || end < window.total) {
		page.NextCursor = encode(cursorPayload{Version: 1, Offset: end, Fingerprint: window.fingerprint})
	}
	return page
}

// FinishFetched completes a page from rows fetched starting at Window.Offset. It is for sources
// that can skip rows in their own iterator instead of rebuilding the ranked prefix.
func FinishFetched[T any](window Window, fetched []T) Page[T] {
	end := window.PageSize
	if end > len(fetched) {
		end = len(fetched)
	}
	results := make([]T, end)
	copy(results, fetched[:end])
	page := Page[T]{Results: results}
	next := window.Offset + end
	if end < len(fetched) && (window.total == 0 || next < window.total) {
		page.NextCursor = encode(cursorPayload{Version: 1, Offset: next, Fingerprint: window.fingerprint})
	}
	return page
}

func fingerprint(bind any, pageSize, total int) (string, error) {
	b, err := json.Marshal(struct {
		Bind     any `json:"bind"`
		PageSize int `json:"page_size"`
		Total    int `json:"total"`
	}{bind, pageSize, total})
	if err != nil {
		return "", fmt.Errorf("encoding pagination binding: %w", err)
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:]), nil
}

func encode(payload cursorPayload) string {
	b, _ := json.Marshal(payload)
	return base64.RawURLEncoding.EncodeToString(b)
}

func decode(cursor string) (cursorPayload, error) {
	b, err := base64.RawURLEncoding.DecodeString(cursor)
	if err != nil {
		return cursorPayload{}, errors.New("invalid pagination cursor")
	}
	var payload cursorPayload
	if err := json.Unmarshal(b, &payload); err != nil || payload.Version != 1 || payload.Offset < 0 || payload.Fingerprint == "" {
		return cursorPayload{}, errors.New("invalid pagination cursor")
	}
	return payload, nil
}
