package hubaccess

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

var (
	ErrUnauthenticated = errors.New("trusted Hub subject is unavailable")
	ErrDenied          = errors.New("project access denied")
)

type Subject struct {
	UserID  string
	TeamIDs []string
}

type subjectContextKey struct{}

func WithTrustedSubject(ctx context.Context, subject Subject) (context.Context, error) {
	normalized, err := normalizeSubject(subject)
	if err != nil {
		return nil, err
	}
	return context.WithValue(ctx, subjectContextKey{}, normalized), nil
}

func TrustedSubject(ctx context.Context) (Subject, error) {
	subject, ok := ctx.Value(subjectContextKey{}).(Subject)
	if ok {
		return normalizeSubject(subject)
	}
	userID := strings.TrimSpace(config.ResolveConfig("hub.subject.user", nil, nil))
	if userID == "" {
		return Subject{UserID: AnonymousUserID}, nil
	}
	teamsValue := config.ResolveConfig("hub.subject.teams", nil, nil)
	teams := strings.FieldsFunc(teamsValue, func(r rune) bool { return r == ',' || r == ';' })
	return normalizeSubject(Subject{UserID: userID, TeamIDs: teams})
}

func normalizeSubject(subject Subject) (Subject, error) {
	subject.UserID = strings.TrimSpace(subject.UserID)
	if err := ValidateSubjectID("user", subject.UserID); err != nil {
		return Subject{}, fmt.Errorf("%w: %v", ErrUnauthenticated, err)
	}
	if IsAnonymousUserID(subject.UserID) {
		return Subject{UserID: AnonymousUserID}, nil
	}
	seen := make(map[string]struct{}, len(subject.TeamIDs))
	teams := make([]string, 0, len(subject.TeamIDs))
	for _, teamID := range subject.TeamIDs {
		teamID = strings.TrimSpace(teamID)
		if err := ValidateSubjectID("team", teamID); err != nil {
			return Subject{}, fmt.Errorf("%w: %v", ErrUnauthenticated, err)
		}
		if _, exists := seen[teamID]; exists {
			continue
		}
		seen[teamID] = struct{}{}
		teams = append(teams, teamID)
	}
	sort.Strings(teams)
	subject.TeamIDs = teams
	return subject, nil
}

type ObjectReader interface {
	Get(context.Context, string) ([]byte, error)
}

type GrantDocument struct {
	Version  int        `json:"v"`
	Projects []Selector `json:"projects"`
}

type Selector struct {
	ID         string `json:"id,omitempty"`
	NamePrefix string `json:"name_prefix,omitempty"`
	All        bool   `json:"all,omitempty"`
}

type Grants struct {
	all      bool
	ids      map[string]struct{}
	prefixes []string
}

type projectDocument struct {
	Version int `json:"v"`
	Project struct {
		ID     string `json:"id"`
		Name   string `json:"name"`
		Status string `json:"status"`
	} `json:"project"`
}

func Resolve(ctx context.Context, reader ObjectReader, subject Subject) (Grants, error) {
	if reader == nil {
		return Grants{}, fmt.Errorf("resolve Hub grants: authorization backend is unavailable")
	}
	subject, err := normalizeSubject(subject)
	if err != nil {
		return Grants{}, err
	}
	keys := make([]string, 0, len(subject.TeamIDs)+3)
	keys = append(keys, GlobalProjectsKey())
	if IsAnonymousUserID(subject.UserID) {
		keys = append(keys, AnonymousProjectsKey())
	} else {
		keys = append(keys, AuthenticatedProjectsKey(), UserProjectsKey(subject.UserID))
	}
	for _, teamID := range subject.TeamIDs {
		keys = append(keys, TeamProjectsKey(teamID))
	}

	documents := make([]GrantDocument, len(keys))
	errs := make([]error, len(keys))
	var wg sync.WaitGroup
	for i, key := range keys {
		wg.Add(1)
		go func() {
			defer wg.Done()
			documents[i], errs[i] = readGrantDocument(ctx, reader, key)
		}()
	}
	wg.Wait()

	grants := Grants{ids: make(map[string]struct{})}
	prefixes := make(map[string]struct{})
	for i, err := range errs {
		if err != nil {
			return Grants{}, fmt.Errorf("resolve Hub grants from %s: %w", keys[i], err)
		}
		for _, selector := range documents[i].Projects {
			if selector.All {
				grants.all = true
			}
			if selector.ID != "" {
				grants.ids[selector.ID] = struct{}{}
			}
			if selector.NamePrefix != "" {
				prefixes[selector.NamePrefix] = struct{}{}
			}
		}
	}
	grants.prefixes = make([]string, 0, len(prefixes))
	for prefix := range prefixes {
		grants.prefixes = append(grants.prefixes, prefix)
	}
	sort.Strings(grants.prefixes)
	canonical := grants.prefixes[:0]
	for _, prefix := range grants.prefixes {
		covered := false
		for _, existing := range canonical {
			if strings.HasPrefix(prefix, existing) {
				covered = true
				break
			}
		}
		if !covered {
			canonical = append(canonical, prefix)
		}
	}
	grants.prefixes = canonical
	return grants, nil
}

func ResolveTrusted(ctx context.Context, reader ObjectReader) (Grants, Subject, error) {
	subject, err := TrustedSubject(ctx)
	if err != nil {
		return Grants{}, Subject{}, err
	}
	grants, err := Resolve(ctx, reader, subject)
	return grants, subject, err
}

func readGrantDocument(ctx context.Context, reader ObjectReader, key string) (GrantDocument, error) {
	data, err := reader.Get(ctx, key)
	if errors.Is(err, s3store.ErrNotFound) {
		return GrantDocument{}, nil
	}
	if err != nil {
		return GrantDocument{}, err
	}
	if len(strings.TrimSpace(string(data))) == 0 {
		return GrantDocument{}, nil
	}
	var document GrantDocument
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&document); err != nil {
		return GrantDocument{}, fmt.Errorf("invalid grant document: %w", err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		if err == nil {
			return GrantDocument{}, errors.New("invalid grant document: multiple JSON values")
		}
		return GrantDocument{}, fmt.Errorf("invalid grant document: %w", err)
	}
	if document.Version != 1 {
		return GrantDocument{}, fmt.Errorf("unsupported grant document version %d", document.Version)
	}
	for i := range document.Projects {
		selector, err := normalizeSelector(document.Projects[i])
		if err != nil {
			return GrantDocument{}, fmt.Errorf("selector %d: %w", i, err)
		}
		document.Projects[i] = selector
	}
	return document, nil
}

func normalizeSelector(selector Selector) (Selector, error) {
	set := 0
	if selector.ID != "" {
		set++
		selector.ID = strings.TrimSpace(selector.ID)
		if err := ValidateProjectID(selector.ID); err != nil {
			return Selector{}, err
		}
	}
	if selector.NamePrefix != "" {
		set++
		prefix, err := NormalizeNamePrefix(selector.NamePrefix)
		if err != nil {
			return Selector{}, err
		}
		selector.NamePrefix = prefix
	}
	if selector.All {
		set++
	}
	if set != 1 {
		return Selector{}, errors.New("selector must set exactly one of id, name_prefix, or all")
	}
	return selector, nil
}

func (g Grants) Allows(projectID, normalizedName string) bool {
	if g.all {
		return true
	}
	if _, ok := g.ids[projectID]; ok {
		return true
	}
	for _, prefix := range g.prefixes {
		if strings.HasPrefix(normalizedName, prefix) {
			return true
		}
	}
	return false
}

func (g Grants) ExactIDs() []string {
	ids := make([]string, 0, len(g.ids))
	for id := range g.ids {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

func (g Grants) NamePrefixes() []string {
	return append([]string(nil), g.prefixes...)
}

func (g Grants) All() bool { return g.all }

func Authorize(ctx context.Context, reader ObjectReader, projectID, normalizedName string) error {
	grants, _, err := ResolveTrusted(ctx, reader)
	if err != nil {
		return err
	}
	if !grants.Allows(projectID, normalizedName) {
		return fmt.Errorf("%w: %s", ErrDenied, projectID)
	}
	return nil
}

func AuthorizeProject(ctx context.Context, reader ObjectReader, projectID string) error {
	if err := ValidateProjectID(projectID); err != nil {
		return err
	}
	data, err := reader.Get(ctx, ProjectMetadataKey(projectID))
	if err != nil {
		return err
	}
	var document projectDocument
	if json.Unmarshal(data, &document) != nil || document.Version != 2 || document.Project.ID != projectID || document.Project.Status != "active" {
		return fmt.Errorf("invalid project metadata for %s", projectID)
	}
	name, err := NormalizeProjectName(document.Project.Name)
	if err != nil || name != document.Project.Name {
		return fmt.Errorf("invalid project metadata name for %s", projectID)
	}
	return Authorize(ctx, reader, projectID, name)
}
