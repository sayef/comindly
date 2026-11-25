package main

import (
	"context"
	"errors"
	"sync"

	"github.com/northvile-clinical/audit-trail/auditlog"
)

// ErrDuplicate is returned for an event id that is already stored; redeliveries are expected.
var ErrDuplicate = errors.New("event already recorded")

// Store is the append-only event log, one hash chain per study.
type Store struct {
	mu     sync.Mutex
	db     *Postgres
	heads  map[string]string
	seenID map[string]bool
}

func NewStore(db *Postgres) *Store {
	return &Store{db: db, heads: map[string]string{}, seenID: map[string]bool{}}
}

// Append seals and stores an event. There is deliberately no update or delete.
func (s *Store) Append(ctx context.Context, e auditlog.Event) (auditlog.Event, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.seenID[e.ID] {
		return e, ErrDuplicate
	}
	e.Seal(s.heads[e.StudyID])
	if err := s.db.Insert(ctx, e); err != nil {
		return e, err
	}
	s.heads[e.StudyID] = e.Hash
	s.seenID[e.ID] = true
	return e, nil
}

// History lists the events for one entity, oldest first.
func (s *Store) History(ctx context.Context, entityType, entityID string) ([]auditlog.Event, error) {
	return s.db.ByEntity(ctx, entityType, entityID)
}

// Verify walks a study's chain and reports the first event whose hash does not match.
func (s *Store) Verify(ctx context.Context, studyID string) (string, error) {
	events, err := s.db.ByStudy(ctx, studyID)
	if err != nil {
		return "", err
	}
	prev := ""
	for _, e := range events {
		want := e.Hash
		e.Seal(prev)
		if e.Hash != want {
			return e.ID, nil
		}
		prev = want
	}
	return "", nil
}
