package main

import (
	"context"
	"encoding/json"
	"log"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/sqs"
	"github.com/northvile-clinical/audit-trail/auditlog"
)

// ingestQueue receives audit-events, trial-events and safety-events through SNS subscriptions.
const ingestQueue = "https://sqs.eu-central-1.amazonaws.com/123456789012/audit-ingest"

type envelope struct {
	Message string `json:"Message"`
}

type domainEvent struct {
	EventID    string          `json:"event_id"`
	Type       string          `json:"type"`
	StudyID    string          `json:"study_id"`
	Payload    json.RawMessage `json:"payload"`
	OccurredAt string          `json:"occurred_at"`
}

// Consume appends every delivered event; duplicates are acknowledged without a second write.
func Consume(ctx context.Context, client *sqs.Client, s *Store) {
	for ctx.Err() == nil {
		out, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
			QueueUrl:            aws.String(ingestQueue),
			MaxNumberOfMessages: 10,
			WaitTimeSeconds:     20,
		})
		if err != nil {
			log.Printf("receive: %v", err)
			continue
		}
		for _, m := range out.Messages {
			if err := ingest(ctx, s, aws.ToString(m.Body)); err != nil {
				log.Printf("ingest %s: %v", aws.ToString(m.MessageId), err)
				continue
			}
			client.DeleteMessage(ctx, &sqs.DeleteMessageInput{QueueUrl: aws.String(ingestQueue), ReceiptHandle: m.ReceiptHandle})
		}
	}
}

func ingest(ctx context.Context, s *Store, body string) error {
	var env envelope
	if err := json.Unmarshal([]byte(body), &env); err != nil {
		return err
	}
	var d domainEvent
	if err := json.Unmarshal([]byte(env.Message), &d); err != nil {
		return err
	}
	e := toAuditEvent(d)
	_, err := s.Append(ctx, e)
	if err == ErrDuplicate {
		return nil
	}
	return err
}

func toAuditEvent(d domainEvent) auditlog.Event {
	e := auditlog.Event{ID: d.EventID, StudyID: d.StudyID, Action: d.Type, Actor: "system", After: d.Payload}
	if d.Type == "audit.recorded" {
		var p struct {
			Actor, Action, EntityType, EntityID, Reason string
		}
		json.Unmarshal(d.Payload, &p)
		e.Actor, e.Action, e.EntityType, e.EntityID, e.Reason = p.Actor, p.Action, p.EntityType, p.EntityID, p.Reason
	}
	return e
}
