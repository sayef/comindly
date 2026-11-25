package main

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/northvile-clinical/audit-trail/auditlog"
)

// Handler serves randomisation; every allocation and unblinding is audited.
type Handler struct {
	store *Store
	audit *auditlog.Client
}

func (h *Handler) routes() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/randomisations", h.randomise)
	mux.HandleFunc("GET /api/randomisations/{subjectID}", h.getAssignment)
	mux.HandleFunc("POST /api/randomisations/{subjectID}/unblind", h.emergencyUnblind)
	return mux
}

type randomiseRequest struct {
	StudyID   string            `json:"study_id"`
	SubjectID string            `json:"subject_id"`
	Factors   map[string]string `json:"factors"`
	Actor     string            `json:"actor"`
}

// randomise allocates the next slot in the subject's stratum and returns the blinded view only.
func (h *Handler) randomise(w http.ResponseWriter, r *http.Request) {
	var req randomiseRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	scheme, err := fetchScheme(req.StudyID)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadGateway)
		return
	}
	stratum, err := StratumKey(scheme, req.Factors)
	if err != nil {
		http.Error(w, err.Error(), http.StatusUnprocessableEntity)
		return
	}
	a, err := h.store.Assign(req.StudyID, req.SubjectID, stratum)
	if errors.Is(err, ErrAlreadyRandomised) {
		json.NewEncoder(w).Encode(a.Blinded())
		return
	}
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	if err := h.audit.Record(auditlog.Event{
		ID: "rand-" + a.SubjectID, StudyID: a.StudyID, Actor: req.Actor,
		Action: "subject.randomised", EntityType: "subject", EntityID: a.SubjectID,
	}); err != nil {
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
		return
	}
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(a.Blinded())
}

func (h *Handler) getAssignment(w http.ResponseWriter, r *http.Request) {
	a, ok := h.store.Get(r.PathValue("subjectID"))
	if !ok {
		http.NotFound(w, r)
		return
	}
	json.NewEncoder(w).Encode(a.Blinded())
}

type unblindRequest struct {
	Role   string `json:"role"`
	Actor  string `json:"actor"`
	Reason string `json:"reason"`
}

// emergencyUnblind reveals one subject's arm to the safety physician, e.g. for a SUSAR report.
func (h *Handler) emergencyUnblind(w http.ResponseWriter, r *http.Request) {
	var req unblindRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if req.Role != "SAFETY_PHYSICIAN" || req.Reason == "" {
		http.Error(w, "unblinding needs the safety physician role and a reason", http.StatusForbidden)
		return
	}
	a, ok := h.store.Get(r.PathValue("subjectID"))
	if !ok {
		http.NotFound(w, r)
		return
	}
	if err := h.audit.Record(auditlog.Event{
		ID: "unblind-" + a.SubjectID, StudyID: a.StudyID, Actor: req.Actor, Reason: req.Reason,
		Action: "subject.unblinded", EntityType: "subject", EntityID: a.SubjectID,
	}); err != nil {
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
		return
	}
	a.Unblinded = true
	json.NewEncoder(w).Encode(map[string]string{"subject_id": a.SubjectID, "arm": a.Arm})
}
