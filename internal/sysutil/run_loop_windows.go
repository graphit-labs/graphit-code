package sysutil

import "errors"

func RunLoop(_ string, _ []string, fn func() error) error {
	for {
		err := fn()
		if !errors.Is(err, ErrReplace) {
			return err
		}
	}
}
