package netutil

import "testing"

func TestOriginAllowed(t *testing.T) {
	for _, testCase := range []struct {
		name       string
		configured []string
		origin     string
		want       bool
	}{
		{name: "no origin header is not a browser request", origin: "", want: true},
		{name: "loopback is allowed without configuration", origin: "http://127.0.0.1:8080", want: true},
		{name: "localhost is allowed without configuration", origin: "http://localhost:3000", want: true},
		{name: "ipv6 loopback is allowed without configuration", origin: "http://[::1]:8080", want: true},
		{name: "any loopback address counts", origin: "http://127.0.0.2:9000", want: true},
		{name: "remote origin is refused without configuration", origin: "https://claude.ai"},
		{name: "configured origin is allowed", configured: []string{"https://claude.ai"}, origin: "https://claude.ai", want: true},
		{name: "configuration replaces the loopback default", configured: []string{"https://claude.ai"}, origin: "http://127.0.0.1:8080"},
		{name: "unconfigured origin is refused", configured: []string{"https://claude.ai"}, origin: "https://attacker.example"},
		{name: "wildcard accepts any origin", configured: []string{"*"}, origin: "https://attacker.example", want: true},
		// The loopback default is for plain local HTTP; https on a loopback name is someone
		// else's deployment and must be declared like any other origin.
		{name: "https on a loopback name is refused by default", origin: "https://localhost:8080"},
		{name: "https on a loopback name can still be configured", configured: []string{"https://localhost:8080"}, origin: "https://localhost:8080", want: true},
		{name: "malformed origin is refused", origin: "not a url", want: false},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if got := OriginAllowed(testCase.configured, testCase.origin); got != testCase.want {
				t.Fatalf("OriginAllowed(%v, %q) = %v; want %v", testCase.configured, testCase.origin, got, testCase.want)
			}
		})
	}
}
