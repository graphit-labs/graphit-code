package config

import "fmt"

// EnsureInstallationIdentity provisions every stable local identity before setup succeeds.
func EnsureInstallationIdentity() error {
	if _, err := UnitID(); err != nil {
		return fmt.Errorf("preparing unit identity: %w", err)
	}
	if _, err := ClientSecret(); err != nil {
		return fmt.Errorf("preparing event identity: %w", err)
	}
	return nil
}
