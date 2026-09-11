package ast

import (
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type grammarOverrideState struct {
	mu        sync.Mutex
	loaded    bool
	overrides map[string]string
	signature string
	lastCheck time.Time
}

func (s *grammarOverrideState) get(projectDir string) map[string]string {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now()
	if s.loaded && now.Sub(s.lastCheck) < queryStaleCheckInterval {
		return s.overrides
	}
	s.lastCheck = now

	var projectCfg config.ConfigMap
	if projectDir != "" {
		projectCfg = config.LoadProjectConfig(projectDir)
	}
	sig := config.ResolveConfig("ast.grammar", nil, projectCfg)
	if s.loaded && sig == s.signature {
		return s.overrides
	}

	s.overrides = config.ParseGrammarOverrides(sig)
	s.signature, s.loaded = sig, true
	return s.overrides
}

var grammarOverrideStates sync.Map

func grammarOverridesFor(projectDir string) map[string]string {
	v, _ := grammarOverrideStates.LoadOrStore(projectDir, &grammarOverrideState{})
	return v.(*grammarOverrideState).get(projectDir)
}

func overriddenGrammarFor(projectDir, ext string) string {
	return grammarOverridesFor(projectDir)[strings.ToLower(ext)]
}

func grammarKnownIn(projectDir, grammar string) bool {
	if grammar == "" {
		return false
	}
	extTablesMu.RLock()
	tsCfg, tsOK := tsGrammarMap[grammar]
	antlrCfg, antlrOK := antlrGrammarMap[grammar]
	extTablesMu.RUnlock()

	if tsOK {
		return grammarEnabledIn(projectDir, tsCfg.Language, tsCfg.Grammar)
	}
	if antlrOK {
		return grammarEnabledIn(projectDir, antlrCfg.Language, antlrCfg.Grammar)
	}
	if projectDir == "" {
		return false
	}
	for _, qf := range effectiveProjectQueryFiles(projectDir) {
		if effectiveGrammarName(qf) == grammar {
			return grammarEnabledIn(projectDir, qf.Language, grammar)
		}
	}
	return false
}

func invalidateGrammarOverrides() {
	grammarOverrideStates.Range(func(_, v any) bool {
		st := v.(*grammarOverrideState)
		st.mu.Lock()
		st.loaded = false
		st.mu.Unlock()
		return true
	})
}
