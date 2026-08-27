package config

import (
	"fmt"
	"sync"

	"github.com/oklog/ulid/v2"
)

// The unit identity: which installation of the framework this is.
//
// It lives here, in the global configuration, rather than in any one module — because it is not a
// memory concept. Memory keys its `user` scope by it, but so would anything else that needs to say
// "this install" rather than "this project": telemetry attribution, a lease on a shared resource,
// or a published artifact's origin.
//
// It replaced `git config user.email`, which was a bad dependency in two ways. It required a
// second tool to be configured before a memory could be written — the error literally told the
// user to go and run `git config --global user.email` — and it tied the framework's notion of
// identity to a version control system the framework no longer uses for anything.

// UnitIDKey is the configuration key holding this installation's identity.
const UnitIDKey = "unit.id"

var unitOnce struct {
	sync.Mutex
	value string
}

// ResolveUnitID reads the identity with the usual precedence, WITHOUT generating one.
//
// It is the accessor for code that wants to know whether an identity has been set at all. Use
// UnitID to get one that is guaranteed non-empty.
func ResolveUnitID(inlineCfg, projectCfg ConfigMap) string {
	return ResolveConfig(UnitIDKey, inlineCfg, projectCfg)
}

// UnitID is this installation's identity, generating and persisting one on first use.
//
// The default is a ULID written into the GLOBAL configuration, so it works with no setup at all,
// survives upgrades, and is visible and editable like every other setting — `config get unit.id`
// answers, and `config set unit.id <value>` overrides it.
//
// Setting the SAME value on two machines is the supported way to make them one unit, which is
// what makes a person's user-scope memories follow them across both. Without that, each
// installation is its own unit and keeps its own user scope.
func UnitID() (string, error) {
	if v := ResolveUnitID(nil, nil); v != "" {
		return v, nil
	}

	// Serialised, and re-checked inside the lock: two goroutines racing here would otherwise
	// generate two identities and the second would overwrite the first.
	unitOnce.Lock()
	defer unitOnce.Unlock()
	if unitOnce.value != "" {
		return unitOnce.value, nil
	}
	if v := ResolveUnitID(nil, nil); v != "" {
		unitOnce.value = v
		return v, nil
	}

	generated := ulid.Make().String()
	if err := SetGlobalConfigValue(UnitIDKey, generated); err != nil {
		return "", fmt.Errorf("persisting the unit identity: %w", err)
	}
	unitOnce.value = generated
	return generated, nil
}

// resetUnitCache exists for tests, which change HOME between cases and must not see the identity
// generated under a previous one.
func resetUnitCache() {
	unitOnce.Lock()
	unitOnce.value = ""
	unitOnce.Unlock()
}
