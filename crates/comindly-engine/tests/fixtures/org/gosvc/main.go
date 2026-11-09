package main

import (
	"fmt"

	m "github.com/acme/golib/model"
	"github.com/acme/golib/store"
)

type Handler struct {
	m.Base
	st *store.Store
}

func NewHandler() *Handler {
	return &Handler{st: store.New()}
}

func (h *Handler) Serve(id string) string {
	fmt.Println(h.Health())
	return h.st.Get(id) + helper()
}

func main() {
	h := NewHandler()
	h.Serve("1")
}
