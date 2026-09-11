package config

import "strings"

const DefaultUIHost = "127.0.0.1"

func ResolveUIHost(inlineCfg, projectCfg ConfigMap) string {
	host := strings.TrimSpace(ResolveConfig("ui.host", inlineCfg, projectCfg))
	if host == "" {
		return DefaultUIHost
	}
	return host
}

func ResolveUIAllowedOrigins(inlineCfg, projectCfg ConfigMap) []string {
	raw := ResolveConfig("ui.allowed_origins", inlineCfg, projectCfg)
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
