package config

import (
	"bufio"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Config holds values loaded from ~/.config/binman/config.
type Config struct {
	Collection string
	// Timeout is the default per-request timeout. Zero means no timeout
	// (useful for streaming endpoints).
	Timeout time.Duration
	// ClientCert / ClientKey enable mTLS when both are set.
	ClientCert string
	ClientKey  string
}

// Load reads ~/.config/binman/config and returns the parsed config.
// Missing file or unknown keys are silently ignored.
func Load() Config {
	path := filepath.Join(configDir(), "binman", "config")
	f, err := os.Open(path)
	if err != nil {
		return Config{}
	}
	defer f.Close()

	var cfg Config
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		k, v, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		k = strings.TrimSpace(k)
		v = strings.TrimSpace(v)
		switch k {
		case "HTTP_FILES":
			cfg.Collection = v
		case "TIMEOUT":
			if d, err := time.ParseDuration(v); err == nil {
				cfg.Timeout = d
			}
		case "CLIENT_CERT":
			cfg.ClientCert = v
		case "CLIENT_KEY":
			cfg.ClientKey = v
		}
	}
	return cfg
}

func configDir() string {
	if d := os.Getenv("XDG_CONFIG_HOME"); d != "" {
		return d
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config")
}
