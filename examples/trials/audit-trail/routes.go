package main

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/northvile-clinical/audit-trail/auditlog"
)

func routes(s *Store) *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/audit/events", s.appendEvent)
	mux.HandleFunc("GET /api/audit/entities/{entityType}/{entityID}", s.entityHistory)
	mux.HandleFunc("GET /api/audit/studies/{studyID}/verify", s.verifyChain)
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, r *http.Request) {})
	return mux
}

func (s *Store) appendEvent(w http.ResponseWriter, r *http.Request) {
	var e auditlog.Event
	if err := json.NewDecoder(r.Body).Decode(&e); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if e.ID == "" || e.StudyID == "" || e.Actor == "" {
		http.Error(w, "id, study_id and actor are required", http.StatusBadRequest)
		return
	}
	stored, err := s.Append(r.Context(), e)
	if errors.Is(err, ErrDuplicate) {
		w.WriteHeader(http.StatusCreated)
		return
	}
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(stored)
}

// entityHistory answers "who changed this, when and why" for monitors and inspectors.
func (s *Store) entityHistory(w http.ResponseWriter, r *http.Request) {
	events, err := s.History(r.Context(), r.PathValue("entityType"), r.PathValue("entityID"))
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	json.NewEncoder(w).Encode(events)
}

func (s *Store) verifyChain(w http.ResponseWriter, r *http.Request) {
	broken, err := s.Verify(r.Context(), r.PathValue("studyID"))
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	json.NewEncoder(w).Encode(map[string]any{"intact": broken == "", "first_broken": broken})
}
