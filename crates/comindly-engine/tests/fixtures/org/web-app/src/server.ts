import express from "express";
import { Router } from "express";

const api = Router();

export function listJobs(req: unknown, res: unknown) {}

api.get("/jobs", listJobs);
api.post("/jobs", (req: unknown, res: unknown) => {});

const app = express();
app.use("/api/v2", api);
app.use("/api/legacy", api);
