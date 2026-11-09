//! `comindly serve`: MCP over stdio for agents that start a command, each message forwarded to
//! the current profile's `/mcp`.

use std::io::Write;
use std::process::ExitCode;

use anyhow::Result;
use tokio::io::AsyncBufReadExt;

pub fn serve(flag: Option<&str>) -> ExitCode {
    let c = match crate::profile::client(flag) {
        Ok(c) => c,
        Err(e) => {
            crate::term::err(&format!("{e:#}"));
            return ExitCode::FAILURE;
        }
    };
    match crate::commands::block(forward(c.base(), c.token())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            crate::term::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

async fn forward(base: &str, token: Option<&str>) -> Result<()> {
    let url = format!("{base}/mcp");
    let http = reqwest::Client::new();
    let mut session: Option<String> = None;
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        // A just-started engine answers 503 until its tools have loaded.
        let mut tries = 0;
        let mut resp = loop {
            let mut req = http
                .post(&url)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .body(line.clone());
            if let Some(t) = token {
                req = req.bearer_auth(t);
            }
            if let Some(s) = &session {
                req = req.header("mcp-session-id", s);
            }
            let r = req.send().await?;
            if r.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE && tries < 60 {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                continue;
            }
            break r.error_for_status()?;
        };
        if let Some(s) = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            session = Some(s.to_string());
        }
        let stream = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("text/event-stream"));
        let mut out = std::io::stdout();
        if !stream {
            let body = resp.text().await?;
            if !body.trim().is_empty() {
                writeln!(out, "{}", body.trim())?;
                out.flush()?;
            }
            continue;
        }
        let mut pending = String::new();
        while let Some(chunk) = resp.chunk().await? {
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = pending.find('\n') {
                let l: String = pending.drain(..=i).collect();
                if let Some(data) = l.trim_end().strip_prefix("data:") {
                    let data = data.trim();
                    if !data.is_empty() {
                        writeln!(out, "{data}")?;
                        out.flush()?;
                    }
                }
            }
        }
    }
    Ok(())
}
