package ast

import (
	"encoding/json"
	"fmt"
)

const MaxStrLen = 4096

func SanitizeProps(props map[string]any) map[string]any {
	out := make(map[string]any, len(props))
	for k, v := range props {
		out[k] = coerceValue(v)
	}
	return out
}

func coerceValue(v any) any {
	switch val := v.(type) {
	case string:
		if len(val) > MaxStrLen {
			return val[:MaxStrLen]
		}
		return val
	case int, int64, float64, bool:
		return val
	case nil:
		return nil
	case []string:
		out := make([]string, len(val))
		for i, s := range val {
			if len(s) > MaxStrLen {
				out[i] = s[:MaxStrLen]
			} else {
				out[i] = s
			}
		}
		return out
	case []any:
		if isFlatList(val) {
			return val
		}
		return jsonFallback(val)
	default:
		return jsonFallback(val)
	}
}

func isFlatList(v []any) bool {
	for _, item := range v {
		switch item.(type) {
		case string, int, int64, float64, bool, nil:
			continue
		default:
			return false
		}
	}
	return true
}

func jsonFallback(v any) string {
	data, err := json.Marshal(v)
	if err != nil {
		s := fmt.Sprintf("%v", v)
		if len(s) > MaxStrLen {
			return s[:MaxStrLen]
		}
		return s
	}
	s := string(data)
	if len(s) > MaxStrLen {
		return s[:MaxStrLen]
	}
	return s
}
