package agentpolicy

import "testing"

func TestCapabilityTokenIsBoundToRuntimeKeyAndProfile(t *testing.T) {
	token, err := MintCapabilityToken("runtime-key", ProfileDreamMemory)
	if err != nil {
		t.Fatal(err)
	}
	if profile, ok := VerifyCapabilityToken("runtime-key", token); !ok || profile != ProfileDreamMemory {
		t.Fatalf("valid token rejected: profile=%q ok=%v", profile, ok)
	}
	if _, ok := VerifyCapabilityToken("different-key", token); ok {
		t.Fatal("token was accepted for a different daemon runtime key")
	}
	if _, ok := VerifyCapabilityToken("runtime-key", token+"x"); ok {
		t.Fatal("tampered token was accepted")
	}
}

func TestCapabilityTokenRejectsUnknownProfile(t *testing.T) {
	if _, err := MintCapabilityToken("runtime-key", "unrestricted"); err == nil {
		t.Fatal("unknown profile received a token")
	}
}
