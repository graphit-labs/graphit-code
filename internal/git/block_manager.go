package git

import (
	"os"
	"regexp"
	"strings"
)

type BlockStyle struct {
	Start string

	End string

	EndPrefix string

	EndSuffix string
}

var ShellBlockStyle = BlockStyle{
	Start:     "# --- ",
	End:       " ---",
	EndPrefix: "# --- END ",
	EndSuffix: " ---",
}

var HTMLBlockStyle = BlockStyle{
	Start:     "<!-- ",
	End:       " -->",
	EndPrefix: "<!-- END ",
	EndSuffix: " -->",
}


var XMLBlockStyle = BlockStyle{
	Start:     "<",
	End:       ">",
	EndPrefix: "</",
	EndSuffix: ">",
}

var nlNormRe = regexp.MustCompile(`\n{3,}`)

func buildBlockRegex(marker string, style BlockStyle) *regexp.Regexp {
	start := regexp.QuoteMeta(style.Start + marker + style.End)
	end := regexp.QuoteMeta(style.EndPrefix + marker + style.EndSuffix)
	return regexp.MustCompile(`(?s)\n*` + start + `.*?` + end + `\n*`)
}


func stripBlock(content, marker string, style BlockStyle) string {
	re := buildBlockRegex(marker, style)
	result := re.ReplaceAllString(content, "\n\n")
	result = nlNormRe.ReplaceAllString(result, "\n\n")
	result = strings.TrimLeft(result, "\n")
	return result
}

func isShellShebangOnly(s string) bool {
	s = strings.TrimSpace(s)
	switch s {
	case "",
		"#!/bin/sh",
		"#!/bin/bash",
		"#!/usr/bin/env sh",
		"#!/usr/bin/env bash":
		return true
	}
	return false
}

func InjectBlock(filePath, content, marker, shebang string) error {
	return InjectBlockStyled(filePath, content, marker, shebang, ShellBlockStyle)
}

func InjectBlockStyled(filePath, content, marker, shebang string, style BlockStyle) error {
	existing := ""
	if data, err := os.ReadFile(filePath); err == nil {
		existing = string(data)
	}

	newBody := style.Start + marker + style.End + "\n" +
		strings.TrimSpace(content) +
		"\n" + style.EndPrefix + marker + style.EndSuffix

	re := buildBlockRegex(marker, style)
	if re.MatchString(existing) {

		updated := re.ReplaceAllStringFunc(existing, func(match string) string {

			leading := len(match) - len(strings.TrimLeft(match, "\n"))
			trailing := len(match) - len(strings.TrimRight(match, "\n"))

			if leading > 2 {
				leading = 2
			}
			if trailing > 2 {
				trailing = 2
			}

			return strings.Repeat("\n", leading) + newBody + strings.Repeat("\n", trailing)
		})

		updated = nlNormRe.ReplaceAllString(updated, "\n\n")
		updated = strings.TrimLeft(updated, "\n")
		if strings.TrimSpace(updated) != "" {
			updated = strings.TrimRight(updated, "\n") + "\n"
		}

		if err := os.WriteFile(filePath, []byte(updated), 0644); err != nil {
			return err
		}
		if shebang != "" {
			_ = os.Chmod(filePath, 0755)
		}
		return nil
	}

	stripped := stripBlock(existing, marker, style)

	if isShellShebangOnly(stripped) {
		if shebang != "" {
			stripped = shebang + "\n"
		} else {
			stripped = ""
		}
	}

	if stripped != "" {
		stripped = strings.TrimRight(stripped, "\n") + "\n"
	}

	var block string
	if stripped != "" {
		block = "\n" + newBody + "\n"
	} else {
		block = newBody + "\n"
	}

	if err := os.WriteFile(filePath, []byte(stripped+block), 0644); err != nil {
		return err
	}

	if shebang != "" {
		_ = os.Chmod(filePath, 0755)
	}
	return nil
}

func RemoveBlock(filePath, marker string, deleteIfEmpty bool) (bool, error) {
	return RemoveBlockStyled(filePath, marker, deleteIfEmpty, ShellBlockStyle)
}

func RemoveBlockStyled(filePath, marker string, deleteIfEmpty bool, style BlockStyle) (bool, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return false, nil
	}

	original := string(data)
	cleaned := stripBlock(original, marker, style)

	if strings.TrimSpace(cleaned) != "" {
		cleaned = strings.TrimRight(cleaned, "\n") + "\n"
	} else {
		cleaned = ""
	}

	if original == cleaned {
		return false, nil
	}

	if deleteIfEmpty && isShellShebangOnly(cleaned) {
		if err := os.Remove(filePath); err != nil {
			return false, err
		}
		return true, nil
	}

	if err := os.WriteFile(filePath, []byte(cleaned), 0644); err != nil {
		return false, err
	}
	return true, nil
}
