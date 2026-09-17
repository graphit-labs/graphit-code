package config

import (
	"strconv"
	"strings"
)

const DefaultUIHost = "127.0.0.1"
const DefaultUIPort = 8080

func ResolveUIHost(inlineCfg, projectCfg ConfigMap) string {
	host := strings.TrimSpace(ResolveConfig("ui.host", inlineCfg, projectCfg))
	if host == "" {
		return DefaultUIHost
	}
	return host
}

func ResolveUIPort(inlineCfg, projectCfg ConfigMap) int {
	raw := strings.TrimSpace(ResolveConfig("ui.port", inlineCfg, projectCfg))
	if raw == "" {
		return DefaultUIPort
	}
	port, err := strconv.Atoi(raw)
	if err != nil || port < 1 || port > 65535 {
		return DefaultUIPort
	}
	return port
}

func ResolveUIAllowedOrigins(inlineCfg, projectCfg ConfigMap) []string {
	return splitAllowedOrigins(ResolveConfig("ui.allowed_origins", inlineCfg, projectCfg))
}

// splitAllowedOrigins parses the comma-separated origin list shared by every listener that
// accepts one, so ui.allowed_origins and mcp.allowed_origins cannot drift in how they read
// the same kind of value.
func splitAllowedOrigins(raw string) []string {
	if strings.TrimSpace(raw) == "" {
		return nil
	}

	seen := make(map[string]struct{})
	origins := make([]string, 0)
	for _, rawOrigin := range strings.Split(raw, ",") {
		origin := strings.TrimSpace(rawOrigin)
		if origin == "" {
			continue
		}
		if _, ok := seen[origin]; ok {
			continue
		}
		seen[origin] = struct{}{}
		origins = append(origins, origin)
	}
	return origins
}
