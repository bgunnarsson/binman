package app

import (
	"fmt"

	"github.com/bgunnarsson/binman/internal/brufile"
	"github.com/bgunnarsson/binman/internal/envfile"
	"github.com/bgunnarsson/binman/internal/postmanfile"
)

// EnvSourceKind identifies how an env file should be parsed.
type EnvSourceKind int

const (
	envKindDotenv EnvSourceKind = iota
	envKindBrunoEnv
	envKindPostmanEnv
)

// EnvSource is one selectable environment surfaced in the env dropdown.
// It abstracts over .env files, Bruno environments/*.bru, and
// *.postman_environment.json so resolveEnvVars can treat them uniformly.
type EnvSource struct {
	Label string
	Path  string
	Kind  EnvSourceKind
}

// Parse reads and parses the source, returning its key→value map.
func (s EnvSource) Parse() (map[string]string, error) {
	switch s.Kind {
	case envKindDotenv:
		return envfile.Parse(s.Path)
	case envKindBrunoEnv:
		return brufile.LoadEnvironment(s.Path)
	case envKindPostmanEnv:
		return postmanfile.LoadEnvironment(s.Path)
	}
	return nil, fmt.Errorf("envsource: unknown kind %d", s.Kind)
}

// DiscoverEnvSources finds every env file relevant to a request loaded from
// dir, walking up to root. Returned order: .env files, then Bruno envs, then
// Postman envs.
func DiscoverEnvSources(dir, root string) []EnvSource {
	var out []EnvSource

	for _, ef := range envfile.Find(dir, root) {
		out = append(out, EnvSource{
			Label: ef.Label,
			Path:  ef.Path,
			Kind:  envKindDotenv,
		})
	}

	for _, env := range brufile.FindEnvironments(dir, root) {
		out = append(out, EnvSource{
			Label: env.Label,
			Path:  env.Path,
			Kind:  envKindBrunoEnv,
		})
	}

	for _, env := range postmanfile.FindEnvironments(dir, root) {
		out = append(out, EnvSource{
			Label: env.Label,
			Path:  env.Path,
			Kind:  envKindPostmanEnv,
		})
	}

	return out
}
