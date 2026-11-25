package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
)

var registryURL = envOr("TRIAL_REGISTRY_URL", "http://trial-registry")

// fetchScheme reads the study's randomisation design from trial-registry.
func fetchScheme(studyID string) (Scheme, error) {
	var s Scheme
	resp, err := http.Get(registryURL + "/api/studies/" + studyID + "/randomisation")
	if err != nil {
		return s, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return s, fmt.Errorf("trial-registry answered %d for %s", resp.StatusCode, studyID)
	}
	return s, json.NewDecoder(resp.Body).Decode(&s)
}

func kitCode(studyID string, seq int) string {
	return fmt.Sprintf("%s-K%05d", studyID, seq)
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
