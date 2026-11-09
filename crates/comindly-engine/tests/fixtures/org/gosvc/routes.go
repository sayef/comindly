package main

import "net/http"

func getItem(w http.ResponseWriter, r *http.Request) {}

func metrics(w http.ResponseWriter, r *http.Request) {}

func defaultMux() {
	http.HandleFunc("/metrics", metrics)
}

func routes() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/items/{id}", getItem)
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {})
	return mux
}

func fetchUser(id string) (*http.Response, error) {
	return http.Get("http://users/api/accounts/" + id)
}

type engine struct{}

func (e *engine) Group(prefix string) *engine            { return e }
func (e *engine) GET(path string, h func(c *engine))     {}
func listOrders(c *engine)                               {}
func showOrder(c *engine)                                {}

func ginRoutes(r *engine) {
	api := r.Group("/api")
	v1 := api.Group("/v1")
	v1.GET("/orders", listOrders)
	v1.GET("/orders/:id", showOrder)
}

type chiRouter struct{}

func (c *chiRouter) Route(prefix string, fn func(r *chiRouter)) {}
func (c *chiRouter) Get(path string, h func(c *engine))          {}
func listInvoices(c *engine)                                     {}

func chiRoutes(root *chiRouter) {
	root.Route("/billing", func(r *chiRouter) {
		r.Route("/v2", func(r *chiRouter) {
			r.Get("/invoices", listInvoices)
		})
	})
}
