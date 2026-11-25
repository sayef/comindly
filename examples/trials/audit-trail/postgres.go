package main

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/northvile-clinical/audit-trail/auditlog"
)

// Postgres holds audit_events; the service role has INSERT and SELECT only.
type Postgres struct {
	pool *pgxpool.Pool
}

func (p *Postgres) Insert(ctx context.Context, e auditlog.Event) error {
	_, err := p.pool.Exec(ctx,
		`INSERT INTO audit_events (id, study_id, actor, action, entity_type, entity_id, reason, before, after, occurred_at, prev_hash, hash)
		 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
		e.ID, e.StudyID, e.Actor, e.Action, e.EntityType, e.EntityID, e.Reason, e.Before, e.After, e.OccurredAt, e.PrevHash, e.Hash)
	return err
}

func (p *Postgres) ByEntity(ctx context.Context, entityType, entityID string) ([]auditlog.Event, error) {
	return p.query(ctx, `SELECT * FROM audit_events WHERE entity_type = $1 AND entity_id = $2 ORDER BY occurred_at`, entityType, entityID)
}

func (p *Postgres) ByStudy(ctx context.Context, studyID string) ([]auditlog.Event, error) {
	return p.query(ctx, `SELECT * FROM audit_events WHERE study_id = $1 ORDER BY occurred_at`, studyID)
}

func (p *Postgres) query(ctx context.Context, sql string, args ...any) ([]auditlog.Event, error) {
	rows, err := p.pool.Query(ctx, sql, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []auditlog.Event
	for rows.Next() {
		var e auditlog.Event
		if err := rows.Scan(&e.ID, &e.StudyID, &e.Actor, &e.Action, &e.EntityType, &e.EntityID, &e.Reason, &e.Before, &e.After, &e.OccurredAt, &e.PrevHash, &e.Hash); err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, rows.Err()
}
