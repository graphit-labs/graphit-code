package netutil

import (
	"net"
	"net/http"
	"net/url"
	"strings"

	"github.com/rs/cors"
)

// CORSOptions describes what one listener needs beyond the origin rule. Each surface exchanges
// different headers, so methods and headers are per listener while the origin decision is not.
type CORSOptions struct {
	AllowedMethods []string
	AllowedHeaders []string
	ExposedHeaders []string
}

// OriginAllowed is the project's single rule for deciding whether a browser origin may call a
// Graphit listener.
//
// With no configured list only loopback origins pass, which keeps a workstation usable without
// any configuration. A configured list replaces that default with exact matches, or `*` to
// accept any origin. Both listeners answer the same way, so a configured origin cannot work on
// one Graphit surface and silently fail on another.
func OriginAllowed(configured []string, origin string) bool {
	if origin == "" {
		// Not a browser request: there is no origin to police.
		return true
	}
	if len(configured) == 0 {
		return isLoopbackOrigin(origin)
	}
	for _, allowed := range configured {
		if allowed == "*" || allowed == origin {
			return true
		}
	}
	return false
}

// isLoopbackOrigin reports whether an origin is plain HTTP on this machine. The whole loopback
// range counts, parsed rather than string-matched, so an address such as http://127.0.0.2:9000
// is treated like any other loopback address.
//
// The scheme stays restricted to http on purpose: this default exists for local development
// over plain HTTP, and an https origin on a loopback name is someone else's deployment rather
// than the developer's own browser. Reaching a Graphit listener over https from any origin is
// what the configured allowlist is for.
func isLoopbackOrigin(origin string) bool {
	parsed, err := url.Parse(origin)
	if err != nil || parsed.Host == "" || parsed.Scheme != "http" {
		return false
	}
	host := parsed.Hostname()
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

// Credentials are never enabled: Graphit listeners authenticate with the Authorization header
// rather than cookies, which also makes the invalid wildcard-with-credentials combination
// impossible to configure.
func CORS(configured []string, options CORSOptions) func(http.Handler) http.Handler {
	policy := cors.New(cors.Options{
		AllowOriginFunc:  func(origin string) bool { return OriginAllowed(configured, origin) },
		AllowedMethods:   options.AllowedMethods,
		AllowedHeaders:   options.AllowedHeaders,
		ExposedHeaders:   options.ExposedHeaders,
		AllowCredentials: false,
	})
	return policy.Handler
}
