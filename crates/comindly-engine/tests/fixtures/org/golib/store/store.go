package store

type Store struct {
	db *DB
}

func New() *Store {
	return &Store{db: &DB{}}
}

func (s *Store) Get(id string) string {
	return s.db.Query(id)
}
