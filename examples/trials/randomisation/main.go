package main

import (
	"log"
	"net/http"

	"github.com/northvile-clinical/audit-trail/auditlog"
)

func main() {
	h := &Handler{
		store: NewStore(),
		audit: auditlog.NewClient(envOr("AUDIT_TRAIL_URL", "http://audit-trail")),
	}
	log.Fatal(http.ListenAndServe(":8080", h.routes()))
}
