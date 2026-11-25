package main

import (
	"context"
	"log"
	"net/http"
	"os"

	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/sqs"
	"github.com/jackc/pgx/v5/pgxpool"
)

func main() {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, os.Getenv("AUDIT_DATABASE_URL"))
	if err != nil {
		log.Fatal(err)
	}
	store := NewStore(&Postgres{pool: pool})
	cfg, err := config.LoadDefaultConfig(ctx)
	if err != nil {
		log.Fatal(err)
	}
	go Consume(ctx, sqs.NewFromConfig(cfg), store)
	log.Fatal(http.ListenAndServe(":8080", routes(store)))
}
