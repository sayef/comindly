package store

type DB struct{}

func (d *DB) Query(q string) string {
	return q
}

func (s *Store) Count() int {
	return len(s.Get("all"))
}
