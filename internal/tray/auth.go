package tray

import (
	"errors"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/daemonctl"
)

type authMenuState struct {
	label   string
	brokers []string
	key     string
}

func captureAuthMenu(now time.Time) authMenuState {
	store, err := auth.Open()
	if err != nil {
		return authMenuState{label: "Login status unavailable", key: "error"}
	}
	state, err := store.Load()
	if err != nil {
		return authMenuState{label: "Login status unavailable", key: "error"}
	}
	result := authMenuState{label: "Not signed in"}
	active, err := store.Active()
	if err == nil {
		if session := active.Profile.OIDC; session != nil && !session.ExpiresAt.IsZero() && !now.Before(session.ExpiresAt) && session.RefreshToken == "" {
			result.label = "Login expired: " + active.Provider.Name
		} else {
			result.label = "Signed in: " + active.Provider.Name + " (" + active.Profile.Name + ")"
		}
	} else if !errors.Is(err, auth.ErrNoActiveProfile) {
		result.label = "Login needed"
	}
	if !strings.HasPrefix(result.label, "Signed in:") {
		for _, name := range state.ProviderNames() {
			if state.Providers[name].Type == auth.ProviderBroker {
				result.brokers = append(result.brokers, name)
			}
		}
	}
	result.key = result.label + "\x00" + strings.Join(result.brokers, "\x00")
	return result
}

func profileForBroker(state auth.State, provider string) string {
	for _, name := range state.ProfileNames() {
		if state.Profiles[name].Provider == provider {
			return name
		}
	}
	if _, exists := state.Profiles[provider]; !exists {
		return provider
	}
	for i := 2; ; i++ {
		candidate := provider + "-" + strconv.Itoa(i)
		if _, exists := state.Profiles[candidate]; !exists {
			return candidate
		}
	}
}

func launchBrokerLogin(provider string) error {
	store, err := auth.Open()
	if err != nil {
		return err
	}
	state, err := store.Load()
	if err != nil {
		return err
	}
	current, ok := state.Providers[provider]
	if !ok || current.Type != auth.ProviderBroker {
		return fmt.Errorf("broker provider %q is no longer configured", provider)
	}
	exe := daemonctl.ResolveExe()
	if exe == "" {
		return fmt.Errorf("cannot resolve Graphit launcher")
	}
	cmd := exec.Command(exe, "login", "--profile", profileForBroker(state, provider), "--provider", provider)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("broker login failed: %w", err)
	}
	return nil
}
