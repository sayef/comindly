// Package auditlog is the audit event model and the client other Go services record through.
package auditlog

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"time"
)

// Event is one audited change. Events are never updated or deleted once stored.
type Event struct {
	ID         string          `json:"id"`
	StudyID    string          `json:"study_id"`
	Actor      string          `json:"actor"`
	Action     string          `json:"action"`
	EntityType string          `json:"entity_type"`
	EntityID   string          `json:"entity_id"`
	Reason     string          `json:"reason,omitempty"`
	Before     json.RawMessage `json:"before,omitempty"`
	After      json.RawMessage `json:"after,omitempty"`
	OccurredAt time.Time       `json:"occurred_at"`
	PrevHash   string          `json:"prev_hash"`
	Hash       string          `json:"hash"`
}

// Seal chains the event to its predecessor so any later edit breaks the chain.
func (e *Event) Seal(prevHash string) {
	e.OccurredAt = e.OccurredAt.UTC()
	e.PrevHash = prevHash
	e.Hash = ""
	body, _ := json.Marshal(e)
	sum := sha256.Sum256(append([]byte(prevHash), body...))
	e.Hash = hex.EncodeToString(sum[:])
}
