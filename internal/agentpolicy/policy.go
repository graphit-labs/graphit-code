// Package agentpolicy defines capability profiles shared by AI process
// launchers and the Graphit MCP server they spawn.
package agentpolicy

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"os"
	"strings"
)

const (
	EnvProfile         = "GRAPHIT_MCP_CAPABILITY_PROFILE"
	EnvCapabilityToken = "GRAPHIT_MCP_CAPABILITY_TOKEN"
	EnvRunID           = "GRAPHIT_DREAM_RUN_ID"
	ProfileDreamMemory = "dream-memory-v1"
	ProfileHeader      = "X-Graphit-Capability-Profile"
	capabilityTokenV1  = "graphit-capability-v1"
)

// MintCapabilityToken derives a bearer that authorizes exactly one server-side
// capability profile. The daemon runtime key is not copied into the token.
func MintCapabilityToken(runtimeKey, profile string) (string, error) {
	runtimeKey = strings.TrimSpace(runtimeKey)
	profile = strings.TrimSpace(profile)
	if runtimeKey == "" {
		return "", errors.New("runtime key is empty")
	}
	if profile != ProfileDreamMemory {
		return "", errors.New("unsupported capability profile")
	}
	payload := base64.RawURLEncoding.EncodeToString([]byte(profile))
	mac := hmac.New(sha256.New, []byte(runtimeKey))
	_, _ = mac.Write([]byte(capabilityTokenV1 + "." + payload))
	signature := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))
	return capabilityTokenV1 + "." + payload + "." + signature, nil
}

// VerifyCapabilityToken authenticates a scoped bearer and returns the profile
// embedded in it. Client-provided profile headers are not authorization.
func VerifyCapabilityToken(runtimeKey, token string) (string, bool) {
	parts := strings.Split(strings.TrimSpace(token), ".")
	if len(parts) != 3 || parts[0] != capabilityTokenV1 {
		return "", false
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil || string(payload) != ProfileDreamMemory {
		return "", false
	}
	want, err := MintCapabilityToken(runtimeKey, string(payload))
	if err != nil || !hmac.Equal([]byte(want), []byte(strings.TrimSpace(token))) {
		return "", false
	}
	return string(payload), true
}

func CapabilityTokenFromEnv() string {
	return strings.TrimSpace(os.Getenv(EnvCapabilityToken))
}

// DetectProfile reads the inherited Dream profile. Without process isolation,
// this is not a security boundary; the server authorizes the scoped bearer.
func DetectProfile() string {
	return os.Getenv(EnvProfile)
}
