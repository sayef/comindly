package auditlog

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// Client records audit events in audit-trail over HTTP.
type Client struct {
	baseURL string
	http    *http.Client
}

// NewClient returns a client for the audit-trail service at baseURL.
func NewClient(baseURL string) *Client {
	return &Client{baseURL: baseURL, http: &http.Client{Timeout: 5 * time.Second}}
}

// Record stores one event; callers treat a failure as a failure of the change itself.
func (c *Client) Record(e Event) error {
	if e.OccurredAt.IsZero() {
		e.OccurredAt = time.Now().UTC()
	}
	body, err := json.Marshal(e)
	if err != nil {
		return err
	}
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/audit/events", bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("content-type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusCreated {
		return fmt.Errorf("audit-trail answered %d", resp.StatusCode)
	}
	return nil
}
