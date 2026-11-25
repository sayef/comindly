package main

import (
	"errors"
	"sync"
	"time"
)

// Assignment is the stored allocation. Arm never leaves this service except via emergency unblinding.
type Assignment struct {
	SubjectID    string    `json:"subject_id"`
	StudyID      string    `json:"study_id"`
	Stratum      string    `json:"stratum"`
	KitCode      string    `json:"kit_code"`
	Arm          string    `json:"-"`
	RandomisedAt time.Time `json:"randomised_at"`
	Unblinded    bool      `json:"unblinded"`
}

// BlindedView is what enrolment and sites see: a kit code, no arm.
type BlindedView struct {
	SubjectID    string    `json:"subject_id"`
	KitCode      string    `json:"kit_code"`
	RandomisedAt time.Time `json:"randomised_at"`
}

var ErrAlreadyRandomised = errors.New("subject already randomised")

// Store keeps allocation lists per study and stratum and the assignments made from them.
type Store struct {
	mu          sync.Mutex
	lists       map[string]*BlockList
	assignments map[string]*Assignment
}

func NewStore() *Store {
	return &Store{lists: map[string]*BlockList{}, assignments: map[string]*Assignment{}}
}

// Assign allocates exactly once per subject; a retry returns the existing assignment.
func (s *Store) Assign(studyID, subjectID, stratum string) (*Assignment, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if a, ok := s.assignments[subjectID]; ok {
		return a, ErrAlreadyRandomised
	}
	list, ok := s.lists[studyID+"/"+stratum]
	if !ok {
		return nil, ErrListExhausted
	}
	arm, err := list.Take()
	if err != nil {
		return nil, err
	}
	a := &Assignment{
		SubjectID:    subjectID,
		StudyID:      studyID,
		Stratum:      stratum,
		KitCode:      kitCode(studyID, list.Next),
		Arm:          arm,
		RandomisedAt: time.Now().UTC(),
	}
	s.assignments[subjectID] = a
	return a, nil
}

func (s *Store) Get(subjectID string) (*Assignment, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	a, ok := s.assignments[subjectID]
	return a, ok
}

func (a *Assignment) Blinded() BlindedView {
	return BlindedView{SubjectID: a.SubjectID, KitCode: a.KitCode, RandomisedAt: a.RandomisedAt}
}
