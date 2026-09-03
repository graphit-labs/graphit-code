package config

import (
	"fmt"
	"sync"

	"github.com/oklog/ulid/v2"
)

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
