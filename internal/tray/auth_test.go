package tray

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestCaptureAuthMenuOffersOnlyBrokerProvidersWithoutActiveLogin(t *testing.T) {
	dir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), dir)
	state := auth.State{Version: auth.StateVersion, Providers: map[string]auth.Provider{
		"local": {Name: "local", Type: auth.ProviderLocal, Revision: 1},
		"zeta":  {Name: "zeta", Type: auth.ProviderBroker, Revision: 1},
		"alpha": {Name: "alpha", Type: auth.ProviderBroker, Revision: 1},
	}, Profiles: map[string]auth.Profile{}}
	writeAuthState(t, dir, state)
	got := captureAuthMenu(time.Now())
	if got.label != "Not signed in" || !reflect.DeepEqual(got.brokers, []string{"alpha", "zeta"}) {
		t.Fatalf("auth menu = %+v", got)
	}

	state.ActiveProfile = "account"
	state.Profiles["account"] = auth.Profile{Name: "account", Provider: "alpha", ProviderRevision: 1}
	writeAuthState(t, dir, state)
	got = captureAuthMenu(time.Now())
	if got.label != "Signed in: alpha (account)" || len(got.brokers) != 0 {
		t.Fatalf("signed-in menu = %+v", got)
	}
}

func TestProfileForBrokerReusesOrAvoidsExistingProfile(t *testing.T) {
	state := auth.State{Profiles: map[string]auth.Profile{
		"company":   {Provider: "other"},
		"company-2": {Provider: "other"},
	}}
	if got := profileForBroker(state, "company"); got != "company-3" {
		t.Fatalf("new profile = %q", got)
	}
	state.Profiles["alice-company"] = auth.Profile{Provider: "company"}
	if got := profileForBroker(state, "company"); got != "alice-company" {
		t.Fatalf("reused profile = %q", got)
	}
}

func writeAuthState(t *testing.T, dir string, state auth.State) {
	t.Helper()
	data, err := json.Marshal(state)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "auth.json"), data, 0o600); err != nil {
		t.Fatal(err)
	}
}
