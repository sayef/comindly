//! Comindly index — the versioned LanceDB store (local dir or S3).
//!
//! Tables: `symbols`, `edges`, `file_facts`, `search`, `enrichment`, `flows`, `style_guide`, `repo_meta`.
//! Each overwrite creates a new Lance version (prior versions retained), so the newest manifest
//! is the atomic "latest" pointer. Remote stores use the platform's own credentials.

use std::collections::HashMap;
use std::sync::Arc;

use crate::facts::FileFacts;
use crate::model::{Edge, Symbol};
use anyhow::{Context, Result};
use lancedb::arrow::arrow_array::{
    ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, RecordBatch, StringArray,
    UInt32Array, UInt64Array,
};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::table::AddDataMode;

/// One enrichment/flow row: (symbol or entry id, generated text, associated queries).
type TextRow = (GlobalSymbolId, String, Vec<String>);
/// One search-table row: (symbol id, search text, embedding vector).
type SearchRow = (GlobalSymbolId, String, Vec<f32>);
/// A persisted (symbol id, embedding vector) pair.
type VectorRow = (GlobalSymbolId, Vec<f32>);

fn symbols_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("symbol_id", DataType::Utf8, false),
        Field::new("scheme", DataType::Utf8, false),
        Field::new("package_manager", DataType::Utf8, false),
        Field::new("package", DataType::Utf8, false),
        Field::new("version", DataType::Utf8, false),
        Field::new("descriptor", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("language", DataType::Utf8, false),
        Field::new("repo", DataType::Utf8, false),
        Field::new("file_path", DataType::Utf8, false),
        Field::new("line_start", DataType::UInt32, false),
        Field::new("col_start", DataType::UInt32, false),
        Field::new("line_end", DataType::UInt32, false),
        Field::new("col_end", DataType::UInt32, false),
        Field::new("signature", DataType::Utf8, true),
        Field::new("docstring", DataType::Utf8, true),
        Field::new("fp_sig", DataType::UInt64, true),
        Field::new("fp_body", DataType::UInt64, true),
        Field::new("fp_shape", DataType::UInt64, true),
    ]))
}

fn edges_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("src_id", DataType::Utf8, false),
        Field::new("dst_id", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("cross_repo", DataType::Boolean, false),
    ]))
}

fn symbols_batch(symbols: &[Symbol]) -> Result<RecordBatch> {
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.render()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.scheme.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.package_manager.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.package.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.version.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.id.descriptor.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.name.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| format!("{:?}", s.kind)),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| format!("{:?}", s.language)),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.repo.0.clone()),
        )),
        Arc::new(StringArray::from_iter_values(
            symbols.iter().map(|s| s.file_path.clone()),
        )),
        Arc::new(UInt32Array::from_iter_values(
            symbols.iter().map(|s| s.range.start.line),
        )),
        Arc::new(UInt32Array::from_iter_values(
            symbols.iter().map(|s| s.range.start.column),
        )),
        Arc::new(UInt32Array::from_iter_values(
            symbols.iter().map(|s| s.range.end.line),
        )),
        Arc::new(UInt32Array::from_iter_values(
            symbols.iter().map(|s| s.range.end.column),
        )),
        Arc::new(StringArray::from_iter(
            symbols.iter().map(|s| s.signature.clone()),
        )),
        Arc::new(StringArray::from_iter(
            symbols.iter().map(|s| s.docstring.clone()),
        )),
        Arc::new(UInt64Array::from_iter(
            symbols.iter().map(|s| s.fingerprint.map(|f| f.sig)),
        )),
        Arc::new(UInt64Array::from_iter(
            symbols.iter().map(|s| s.fingerprint.map(|f| f.body)),
        )),
        Arc::new(UInt64Array::from_iter(
            symbols.iter().map(|s| s.fingerprint.map(|f| f.shape)),
        )),
    ];
    RecordBatch::try_new(symbols_schema(), arrays).context("build symbols batch")
}

fn edges_batch(edges: &[Edge]) -> Result<RecordBatch> {
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(
            edges.iter().map(|e| e.src.render()),
        )),
        Arc::new(StringArray::from_iter_values(
            edges.iter().map(|e| e.dst.render()),
        )),
        Arc::new(StringArray::from_iter_values(
            edges.iter().map(|e| format!("{:?}", e.kind)),
        )),
        Arc::new(Float32Array::from_iter_values(
            edges.iter().map(|e| e.confidence),
        )),
        Arc::new(BooleanArray::from(
            edges.iter().map(|e| e.cross_repo).collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(edges_schema(), arrays).context("build edges batch")
}

/// Write `batch` as the full contents of table `name`, creating it if absent and otherwise
/// overwriting in place. Overwrite creates a **new Lance version** while retaining prior
/// versions — so the newest version manifest is the atomic "latest" pointer, and older
/// versions remain available for pinning/rollback (`checkout`). Returns the new version.
pub(crate) async fn write_versioned(
    db: &lancedb::Connection,
    name: &str,
    batch: RecordBatch,
) -> Result<u64> {
    // lancedb 0.31 implements `Scannable` for `RecordBatch`, so pass it directly.
    let tbl = match db.open_table(name).execute().await {
        Ok(t) => {
            t.add(batch)
                .mode(AddDataMode::Overwrite)
                .execute()
                .await
                .with_context(|| format!("overwrite table `{name}`"))?;
            t
        }
        Err(_) => db
            .create_table(name, batch)
            .execute()
            .await
            .with_context(|| format!("create table `{name}`"))?,
    };
    tbl.version()
        .await
        .with_context(|| format!("read version of `{name}`"))
}

/// Write the symbol/edge graph to a LanceDB dataset at `uri`
/// (e.g. `s3://bucket/prefix` or a local path). Returns the new `(symbols, edges)` versions.
pub async fn write_graph(uri: &str, symbols: &[Symbol], edges: &[Edge]) -> Result<(u64, u64)> {
    let db = crate::storage::connect(uri)
        .execute()
        .await
        .with_context(|| format!("connect lancedb at {uri}"))?;
    let sv = write_versioned(&db, "symbols", symbols_batch(symbols)?).await?;
    let ev = write_versioned(&db, "edges", edges_batch(edges)?).await?;
    Ok((sv, ev))
}

/// Read the current `(symbols, edges)` versions — the atomic latest pointer.
pub async fn latest_versions(uri: &str) -> Result<(u64, u64)> {
    let db = crate::storage::connect(uri).execute().await?;
    let sv = db.open_table("symbols").execute().await?.version().await?;
    let ev = db.open_table("edges").execute().await?.version().await?;
    Ok((sv, ev))
}

// ---- reading the persisted graph back into core types ----------------------------------

use crate::model::{
    EdgeKind, Fingerprint, GlobalSymbolId, Language, Position, Range, RepoId, SymbolKind,
};
use futures::TryStreamExt;
use lancedb::arrow::arrow_array::Array;
use lancedb::query::ExecutableQuery;

fn kind_from_str(s: &str) -> SymbolKind {
    match s {
        "File" => SymbolKind::File,
        "Module" => SymbolKind::Module,
        "Namespace" => SymbolKind::Namespace,
        "Class" => SymbolKind::Class,
        "Interface" => SymbolKind::Interface,
        "Trait" => SymbolKind::Trait,
        "Enum" => SymbolKind::Enum,
        "Struct" => SymbolKind::Struct,
        "Function" => SymbolKind::Function,
        "Method" => SymbolKind::Method,
        "Field" => SymbolKind::Field,
        "Variable" => SymbolKind::Variable,
        "Constant" => SymbolKind::Constant,
        "Import" => SymbolKind::Import,
        "TypeAlias" => SymbolKind::TypeAlias,
        "Process" => SymbolKind::Process,
        "Contract" => SymbolKind::Contract,
        _ => SymbolKind::Variable,
    }
}

fn lang_from_str(s: &str) -> Language {
    match s {
        "Python" => Language::Python,
        "TypeScript" => Language::TypeScript,
        "JavaScript" => Language::JavaScript,
        "Go" => Language::Go,
        "Rust" => Language::Rust,
        "Java" => Language::Java,
        "Kotlin" => Language::Kotlin,
        "Ruby" => Language::Ruby,
        "CSharp" => Language::CSharp,
        "Cpp" => Language::Cpp,
        "C" => Language::C,
        "Php" => Language::Php,
        "Scala" => Language::Scala,
        "Swift" => Language::Swift,
        // `Other("x")` Debug form, or anything unknown
        other => Language::Other(
            other
                .strip_prefix("Other(\"")
                .and_then(|s| s.strip_suffix("\")"))
                .unwrap_or(other)
                .to_string(),
        ),
    }
}

/// Parse a rendered SCIP id (`scheme pm package version descriptor`) back into components.
fn id_from_rendered(s: &str) -> GlobalSymbolId {
    let mut it = s.splitn(5, ' ');
    let mut next = || it.next().unwrap_or(".").to_string();
    GlobalSymbolId {
        scheme: next(),
        package_manager: next(),
        package: next(),
        version: next(),
        descriptor: next(),
    }
}

pub(crate) fn str_col<'a>(
    batch: &'a lancedb::arrow::arrow_array::RecordBatch,
    name: &str,
) -> Result<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .with_context(|| format!("column `{name}` missing or not Utf8"))
}

fn u32_col<'a>(
    batch: &'a lancedb::arrow::arrow_array::RecordBatch,
    name: &str,
) -> Result<&'a UInt32Array> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
        .with_context(|| format!("column `{name}` missing or not UInt32"))
}

/// Load the persisted org graph back into core types. This is the fast consumer path:
/// `serve`/`explore` load once from Lance (local mmap or S3) instead of re-parsing.
pub async fn read_graph(uri: &str) -> Result<(Vec<Symbol>, Vec<Edge>)> {
    read_graph_at(uri, None).await
}

/// The graph at given `(symbols, edges)` table versions, or the latest.
pub async fn read_graph_at(uri: &str, at: Option<(u64, u64)>) -> Result<(Vec<Symbol>, Vec<Edge>)> {
    let db = crate::storage::connect(uri).execute().await?;
    read_graph_in(&db, at).await
}

async fn read_graph_in(
    db: &lancedb::Connection,
    at: Option<(u64, u64)>,
) -> Result<(Vec<Symbol>, Vec<Edge>)> {
    let open = |name: &'static str, v: Option<u64>| async move {
        let t = db.open_table(name).execute().await?;
        if let Some(v) = v {
            t.checkout(v).await?;
        }
        anyhow::Ok(t)
    };

    // symbols
    let mut symbols = Vec::new();
    let mut st = open("symbols", at.map(|a| a.0))
        .await?
        .query()
        .execute()
        .await?;
    while let Some(b) = st.try_next().await? {
        let (scheme, pm, pkg, ver, desc) = (
            str_col(&b, "scheme")?,
            str_col(&b, "package_manager")?,
            str_col(&b, "package")?,
            str_col(&b, "version")?,
            str_col(&b, "descriptor")?,
        );
        let (name, kind, lang, repo, file) = (
            str_col(&b, "name")?,
            str_col(&b, "kind")?,
            str_col(&b, "language")?,
            str_col(&b, "repo")?,
            str_col(&b, "file_path")?,
        );
        let (ls, cs, le, ce) = (
            u32_col(&b, "line_start")?,
            u32_col(&b, "col_start")?,
            u32_col(&b, "line_end")?,
            u32_col(&b, "col_end")?,
        );
        let sig = str_col(&b, "signature")?;
        let doc = str_col(&b, "docstring")?;
        // Absent in indexes written before fingerprints existed.
        let fp = |name: &str| {
            b.column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<UInt64Array>())
        };
        let (fs, fb, fsh) = (fp("fp_sig"), fp("fp_body"), fp("fp_shape"));
        for i in 0..b.num_rows() {
            symbols.push(Symbol {
                id: GlobalSymbolId {
                    scheme: scheme.value(i).to_string(),
                    package_manager: pm.value(i).to_string(),
                    package: pkg.value(i).to_string(),
                    version: ver.value(i).to_string(),
                    descriptor: desc.value(i).to_string(),
                },
                name: name.value(i).to_string(),
                kind: kind_from_str(kind.value(i)),
                language: lang_from_str(lang.value(i)),
                repo: RepoId(repo.value(i).to_string()),
                file_path: file.value(i).to_string(),
                range: Range {
                    start: Position {
                        line: ls.value(i),
                        column: cs.value(i),
                    },
                    end: Position {
                        line: le.value(i),
                        column: ce.value(i),
                    },
                },
                signature: (!sig.is_null(i)).then(|| sig.value(i).to_string()),
                docstring: (!doc.is_null(i)).then(|| doc.value(i).to_string()),
                fingerprint: match (fs, fb, fsh) {
                    (Some(a), Some(c), Some(d)) if !a.is_null(i) => Some(Fingerprint {
                        sig: a.value(i),
                        body: c.value(i),
                        shape: d.value(i),
                    }),
                    _ => None,
                },
            });
        }
    }

    // edges
    let mut edges = Vec::new();
    let mut st = open("edges", at.map(|a| a.1))
        .await?
        .query()
        .execute()
        .await?;
    while let Some(b) = st.try_next().await? {
        let src = str_col(&b, "src_id")?;
        let dst = str_col(&b, "dst_id")?;
        let kind = str_col(&b, "kind")?;
        let conf = b
            .column_by_name("confidence")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
            .context("confidence column")?;
        let cross = b
            .column_by_name("cross_repo")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
            .context("cross_repo column")?;
        for i in 0..b.num_rows() {
            edges.push(Edge {
                src: id_from_rendered(src.value(i)),
                dst: id_from_rendered(dst.value(i)),
                kind: edge_kind_from_str(kind.value(i)),
                confidence: conf.value(i),
                cross_repo: cross.value(i),
            });
        }
    }

    Ok((symbols, edges))
}

fn edge_kind_from_str(s: &str) -> EdgeKind {
    match s {
        "Contains" => EdgeKind::Contains,
        "Imports" => EdgeKind::Imports,
        "Calls" => EdgeKind::Calls,
        "Instantiates" => EdgeKind::Instantiates,
        "Inherits" => EdgeKind::Inherits,
        "Implements" => EdgeKind::Implements,
        "References" => EdgeKind::References,
        "Defines" => EdgeKind::Defines,
        "Uses" => EdgeKind::Uses,
        "ParticipatesIn" => EdgeKind::ParticipatesIn,
        "Serves" => EdgeKind::Serves,
        "Consumes" => EdgeKind::Consumes,
        "Publishes" => EdgeKind::Publishes,
        "Subscribes" => EdgeKind::Subscribes,
        "Reads" => EdgeKind::Reads,
        "Writes" => EdgeKind::Writes,
        _ => EdgeKind::References,
    }
}

/// Blocking wrapper for [`read_graph`].
/// Graph versions as `(symbols version, edges version, UTC time)`, oldest first. Each graph
/// write commits both tables, so the n-th versions of each belong together.
pub async fn graph_versions(uri: &str) -> Result<Vec<(u64, u64, String)>> {
    let db = crate::storage::connect(uri).execute().await?;
    let s = db.open_table("symbols").execute().await?;
    let e = db.open_table("edges").execute().await?;
    versions_of(&s, &e).await
}

async fn versions_of(s: &lancedb::Table, e: &lancedb::Table) -> Result<Vec<(u64, u64, String)>> {
    let (s, e) = (s.list_versions().await?, e.list_versions().await?);
    Ok(s.iter()
        .zip(e.iter())
        .map(|(a, b)| (a.version, b.version, a.timestamp.to_rfc3339()))
        .collect())
}

pub fn graph_versions_blocking(uri: &str) -> Result<Vec<(u64, u64, String)>> {
    crate::rt::block_on(graph_versions(uri))
}

pub fn read_graph_blocking(uri: &str) -> Result<(Vec<Symbol>, Vec<Edge>)> {
    crate::rt::block_on(read_graph(uri))
}

/// Read back `(symbol_count, edge_count)` — proves the persisted tables are queryable.
pub async fn count_rows(uri: &str) -> Result<(usize, usize)> {
    let db = crate::storage::connect(uri).execute().await?;
    let symbols = db
        .open_table("symbols")
        .execute()
        .await?
        .count_rows(None)
        .await?;
    let edges = db
        .open_table("edges")
        .execute()
        .await?
        .count_rows(None)
        .await?;
    Ok((symbols, edges))
}

/// Blocking wrapper so the (sync) CLI can persist without an async main.
/// Returns the new `(symbols, edges)` Lance versions.
pub fn write_graph_blocking(uri: &str, symbols: &[Symbol], edges: &[Edge]) -> Result<(u64, u64)> {
    crate::rt::block_on(write_graph(uri, symbols, edges))
}

/// Blocking wrapper for the read-back count.
pub fn count_rows_blocking(uri: &str) -> Result<(usize, usize)> {
    crate::rt::block_on(count_rows(uri))
}

/// Blocking wrapper for the current `(symbols, edges)` versions.
pub fn latest_versions_blocking(uri: &str) -> Result<(u64, u64)> {
    crate::rt::block_on(latest_versions(uri))
}

// ---- embeddings (semantic search vectors) -----------------------------------------------

// ---- repo metadata (last-indexed commit for incremental) --------------------------------

/// Record the commit a repo dataset was indexed at (single-row `repo_meta` table).
pub async fn write_repo_meta(uri: &str, repo: &str, commit: &str) -> Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("commit", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![repo])) as ArrayRef,
            Arc::new(StringArray::from(vec![commit])) as ArrayRef,
        ],
    )
    .context("build repo_meta batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "repo_meta", batch).await?;
    Ok(())
}

/// The commit a repo dataset was last indexed at, if recorded.
pub async fn read_repo_meta(uri: &str) -> Result<Option<String>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("repo_meta").execute().await else {
        return Ok(None);
    };
    let mut st = tbl.query().execute().await?;
    while let Some(b) = st.try_next().await? {
        if b.num_rows() > 0 {
            return Ok(Some(str_col(&b, "commit")?.value(0).to_string()));
        }
    }
    Ok(None)
}

/// Blocking wrapper for [`write_repo_meta`].
pub fn write_repo_meta_blocking(uri: &str, repo: &str, commit: &str) -> Result<()> {
    crate::rt::block_on(write_repo_meta(uri, repo, commit))
}

/// Record the indexed commit for *several* repos (multi-row `repo_meta`) — used by the org
/// `link` so an incremental rebuild knows which repos changed.
pub async fn write_repo_commits(uri: &str, repos: &[IndexedRepo]) -> Result<()> {
    if repos.is_empty() {
        return Ok(());
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("commit", DataType::Utf8, false),
        Field::new("remote", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                repos.iter().map(|r| r.name.clone()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                repos.iter().map(|r| r.commit.clone()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                repos.iter().map(|r| r.remote.clone()),
            )) as ArrayRef,
        ],
    )
    .context("build repo_commits batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "repo_meta", batch).await?;
    Ok(())
}

/// A repository in the index: its name, the commit indexed and where it is cloned from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IndexedRepo {
    pub name: String,
    pub commit: String,
    pub remote: Option<String>,
}

/// The repositories recorded in the index (indexes made before remotes were stored have none).
pub async fn read_indexed_repos(uri: &str) -> Result<Vec<IndexedRepo>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("repo_meta").execute().await else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut st = tbl.query().execute().await?;
    while let Some(b) = st.try_next().await? {
        let repo = str_col(&b, "repo")?;
        let commit = str_col(&b, "commit")?;
        let remote = str_col(&b, "remote").ok();
        for i in 0..b.num_rows() {
            out.push(IndexedRepo {
                name: repo.value(i).to_string(),
                commit: commit.value(i).to_string(),
                remote: remote
                    .filter(|r| r.is_valid(i))
                    .map(|r| r.value(i).to_string()),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The indexed commit of each repository, by name.
pub async fn read_repo_commits(uri: &str) -> Result<std::collections::HashMap<String, String>> {
    Ok(read_indexed_repos(uri)
        .await?
        .into_iter()
        .map(|r| (r.name, r.commit))
        .collect())
}

/// Blocking wrapper for [`write_repo_commits`].
pub fn write_repo_commits_blocking(uri: &str, repos: &[IndexedRepo]) -> Result<()> {
    crate::rt::block_on(write_repo_commits(uri, repos))
}

/// Blocking wrapper for [`read_repo_commits`].
pub fn read_repo_commits_blocking(uri: &str) -> Result<std::collections::HashMap<String, String>> {
    crate::rt::block_on(read_repo_commits(uri))
}

/// Blocking wrapper for [`read_repo_meta`].
pub fn read_repo_meta_blocking(uri: &str) -> Result<Option<String>> {
    crate::rt::block_on(read_repo_meta(uri))
}

// ---- hybrid search table (BM25 FTS + vector, native LanceDB fusion) ---------------------

use lance_index::scalar::FullTextSearchQuery;
use lancedb::index::scalar::FtsIndexBuilder;
use lancedb::index::vector::IvfHnswFlatIndexBuilder;
use lancedb::index::Index;
use lancedb::query::{QueryBase, QueryExecutionOptions, Select};

fn search_schema(dim: usize) -> Arc<Schema> {
    let item = Arc::new(Field::new("item", DataType::Float32, true));
    Arc::new(Schema::new(vec![
        Field::new("symbol_id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(item, dim as i32), false),
    ]))
}

/// Write the denormalized retrieval table (`symbol_id`, `text`, `vector`) and build a native
/// BM25 full-text index on `text`. Hybrid search then fuses this FTS index with vector search
/// using LanceDB's own RRF reranker — no hand-rolled BM25.
pub async fn write_search_table(uri: &str, rows: &[SearchRow]) -> Result<u64> {
    let Some((_, _, first)) = rows.first() else {
        return Ok(0);
    };
    let dim = first.len();
    let ids = StringArray::from_iter_values(rows.iter().map(|(id, _, _)| id.render()));
    let texts = StringArray::from_iter_values(rows.iter().map(|(_, t, _)| t.clone()));
    let flat: Vec<f32> = rows
        .iter()
        .flat_map(|(_, _, v)| v.iter().copied())
        .collect();
    let item = Arc::new(Field::new("item", DataType::Float32, true));
    let vectors =
        FixedSizeListArray::try_new(item, dim as i32, Arc::new(Float32Array::from(flat)), None)
            .context("build vector column")?;
    let batch = RecordBatch::try_new(
        search_schema(dim),
        vec![
            Arc::new(ids) as ArrayRef,
            Arc::new(texts) as ArrayRef,
            Arc::new(vectors) as ArrayRef,
        ],
    )
    .context("build search batch")?;

    let db = crate::storage::connect(uri).execute().await?;
    let version = write_versioned(&db, "search", batch).await?;

    // (Re)build the BM25 full-text index on `text`.
    let tbl = db.open_table("search").execute().await?;
    tbl.create_index(&["text"], Index::FTS(FtsIndexBuilder::default()))
        .execute()
        .await
        .context("create FTS index")?;

    // An ANN index LanceDB keeps in its cache, so a query on object storage reads only the rows
    // it returns instead of every vector.
    let parts = (rows.len() / 20_000).max(1) as u32;
    let ann = IvfHnswFlatIndexBuilder::default().num_partitions(parts);
    if let Err(e) = tbl
        .create_index(&["vector"], Index::IvfHnswFlat(ann))
        .execute()
        .await
    {
        eprintln!("comindly: vector ANN index skipped (non-fatal): {e}");
    }
    Ok(version)
}

/// Read `(symbol_id, vector)` from the search table for incremental reuse. `Ok(None)` if absent.
pub async fn read_search_vectors(uri: &str) -> Result<Option<Vec<VectorRow>>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("search").execute().await else {
        return Ok(None);
    };
    let mut out = Vec::new();
    let mut st = tbl
        .query()
        .select(Select::columns(&["symbol_id", "vector"]))
        .execute()
        .await?;
    while let Some(b) = st.try_next().await? {
        let ids = str_col(&b, "symbol_id")?;
        let vecs = b
            .column_by_name("vector")
            .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>())
            .context("vector column")?;
        for i in 0..b.num_rows() {
            let sub = vecs.value(i);
            let f = sub.as_any().downcast_ref::<Float32Array>().context("f32")?;
            out.push((id_from_rendered(ids.value(i)), f.values().to_vec()));
        }
    }
    Ok(Some(out))
}

/// Native hybrid search: BM25 full-text + vector, fused by LanceDB's RRF reranker. Returns
/// `(rendered_symbol_id, relevance_score)` best-first. `query_vector` must match the index dim.
pub async fn hybrid_search(
    uri: &str,
    query_text: &str,
    query_vector: Vec<f32>,
    k: usize,
) -> Result<Vec<(String, f32)>> {
    let db = crate::storage::connect(uri).execute().await?;
    let tbl = db.open_table("search").execute().await?;
    hybrid_in(&tbl, query_text, query_vector, k).await
}

async fn hybrid_in(
    tbl: &lancedb::Table,
    query_text: &str,
    query_vector: Vec<f32>,
    k: usize,
) -> Result<Vec<(String, f32)>> {
    let mut st = tbl
        .query()
        .select(lancedb::query::Select::columns(&["symbol_id"]))
        .full_text_search(FullTextSearchQuery::new(query_text.to_owned()))
        .nearest_to(query_vector)?
        .limit(k)
        // execute_hybrid fuses FTS + vector with an RRF reranker (k=60) by default.
        .execute_hybrid(QueryExecutionOptions::default())
        .await?;

    let mut out = Vec::new();
    let mut rank = 0usize;
    while let Some(b) = st.try_next().await? {
        let ids = str_col(&b, "symbol_id")?;
        // LanceDB attaches a `_relevance_score` column after reranking; fall back to rank order.
        let scores = b
            .column_by_name("_relevance_score")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
        for i in 0..b.num_rows() {
            let score = scores
                .map(|s| s.value(i))
                .unwrap_or(1.0 / (rank as f32 + 1.0));
            out.push((ids.value(i).to_string(), score));
            rank += 1;
        }
    }
    Ok(out)
}

/// Open tables of one index for a long-running reader: LanceDB keeps their metadata and indexes in
/// its session cache and re-checks for a newer version at most every `fresh`.
pub struct Tables {
    db: lancedb::Connection,
    open: tokio::sync::Mutex<std::collections::HashMap<&'static str, lancedb::Table>>,
}

impl Tables {
    pub async fn connect(uri: &str, fresh: std::time::Duration) -> Result<Tables> {
        let db = crate::storage::connect(uri)
            .read_consistency_interval(fresh)
            .execute()
            .await
            .with_context(|| format!("connect lancedb at {uri}"))?;
        Ok(Tables {
            db,
            open: Default::default(),
        })
    }

    async fn table(&self, name: &'static str) -> Result<lancedb::Table> {
        let mut open = self.open.lock().await;
        if let Some(t) = open.get(name) {
            return Ok(t.clone());
        }
        let t = self.db.open_table(name).execute().await?;
        open.insert(name, t.clone());
        Ok(t)
    }

    pub async fn hybrid_search(
        &self,
        query_text: &str,
        query_vector: Vec<f32>,
        k: usize,
    ) -> Result<Vec<(String, f32)>> {
        hybrid_in(&self.table("search").await?, query_text, query_vector, k).await
    }

    pub async fn versions(&self) -> Result<Vec<(u64, u64, String)>> {
        versions_of(&self.table("symbols").await?, &self.table("edges").await?).await
    }

    /// The graph at `(symbols, edges)` versions; fresh handles, so the shared ones keep tracking
    /// the latest version.
    pub async fn graph_at(&self, at: (u64, u64)) -> Result<(Vec<Symbol>, Vec<Edge>)> {
        read_graph_in(&self.db, Some(at)).await
    }
}

/// Blocking wrappers.
pub fn write_search_table_blocking(uri: &str, rows: &[SearchRow]) -> Result<u64> {
    crate::rt::block_on(write_search_table(uri, rows))
}
pub fn read_search_vectors_blocking(uri: &str) -> Result<Option<Vec<VectorRow>>> {
    crate::rt::block_on(read_search_vectors(uri))
}
pub fn hybrid_search_blocking(
    uri: &str,
    text: &str,
    vector: Vec<f32>,
    k: usize,
) -> Result<Vec<(String, f32)>> {
    crate::rt::block_on(hybrid_search(uri, text, vector, k))
}

// ---- LLM enrichment (summaries + generated queries) -------------------------------------

fn enrichment_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("symbol_id", DataType::Utf8, false),
        Field::new("summary", DataType::Utf8, false),
        Field::new("queries", DataType::Utf8, false), // JSON array of strings
    ]))
}

/// Persist LLM enrichment (one-line summary + generated NL queries) as a Lance `enrichment`
/// table. `queries` is a JSON array string. Returns the new version. No-op if empty.
pub async fn write_enrichment(uri: &str, rows: &[TextRow]) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let ids = StringArray::from_iter_values(rows.iter().map(|(id, _, _)| id.render()));
    let summaries = StringArray::from_iter_values(rows.iter().map(|(_, s, _)| s.clone()));
    let queries = StringArray::from_iter_values(
        rows.iter()
            .map(|(_, _, q)| serde_json::to_string(q).unwrap_or_else(|_| "[]".into())),
    );
    let batch = RecordBatch::try_new(
        enrichment_schema(),
        vec![
            Arc::new(ids) as ArrayRef,
            Arc::new(summaries) as ArrayRef,
            Arc::new(queries) as ArrayRef,
        ],
    )
    .context("build enrichment batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "enrichment", batch).await
}

/// Load persisted enrichment as `(symbol_id, summary, queries)`. `Ok(None)` if absent.
pub async fn read_enrichment(uri: &str) -> Result<Option<Vec<TextRow>>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("enrichment").execute().await else {
        return Ok(None);
    };
    let mut out = Vec::new();
    let mut st = tbl.query().execute().await?;
    while let Some(b) = st.try_next().await? {
        let ids = str_col(&b, "symbol_id")?;
        let summaries = str_col(&b, "summary")?;
        let queries = str_col(&b, "queries")?;
        for i in 0..b.num_rows() {
            let q: Vec<String> = serde_json::from_str(queries.value(i)).unwrap_or_default();
            out.push((
                id_from_rendered(ids.value(i)),
                summaries.value(i).to_string(),
                q,
            ));
        }
    }
    Ok(Some(out))
}

/// Blocking wrapper for [`write_enrichment`].
pub fn write_enrichment_blocking(uri: &str, rows: &[TextRow]) -> Result<u64> {
    crate::rt::block_on(write_enrichment(uri, rows))
}

/// Blocking wrapper for [`read_enrichment`].
pub fn read_enrichment_blocking(uri: &str) -> Result<Option<Vec<TextRow>>> {
    crate::rt::block_on(read_enrichment(uri))
}

/// What each generated text was written from, as `(id, fingerprint)` in `table`; same fingerprint,
/// same input, so the text is reused instead of asking the LLM again.
pub async fn write_keys(uri: &str, table: &str, keys: &[(String, String)]) -> Result<u64> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("symbol_id", DataType::Utf8, false),
        Field::new("key", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                keys.iter().map(|(i, _)| i.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                keys.iter().map(|(_, k)| k.as_str()),
            )) as ArrayRef,
        ],
    )?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, table, batch).await
}

/// The fingerprints [`write_keys`] stored in `table`, by id.
pub async fn read_keys(uri: &str, table: &str) -> Result<HashMap<String, String>> {
    let db = crate::storage::connect(uri).execute().await?;
    let tbl = db.open_table(table).execute().await?;
    let mut out = HashMap::new();
    let mut st = tbl.query().execute().await?;
    while let Some(b) = st.try_next().await? {
        let (ids, keys) = (str_col(&b, "symbol_id")?, str_col(&b, "key")?);
        for i in 0..b.num_rows() {
            out.insert(ids.value(i).to_string(), keys.value(i).to_string());
        }
    }
    Ok(out)
}

pub fn write_keys_blocking(uri: &str, table: &str, keys: &[(String, String)]) -> Result<u64> {
    crate::rt::block_on(write_keys(uri, table, keys))
}

pub fn read_keys_blocking(uri: &str, table: &str) -> HashMap<String, String> {
    crate::rt::block_on(read_keys(uri, table)).unwrap_or_default()
}

fn flows_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("symbol_id", DataType::Utf8, false),
        Field::new("narration", DataType::Utf8, false),
        Field::new("queries", DataType::Utf8, false), // JSON array of strings
    ]))
}

/// Persist pre-generated flow walkthroughs: `(entry_id, narration_markdown, flow_questions)`.
pub async fn write_flows(uri: &str, rows: &[TextRow]) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let ids = StringArray::from_iter_values(rows.iter().map(|(id, _, _)| id.render()));
    let narr = StringArray::from_iter_values(rows.iter().map(|(_, n, _)| n.clone()));
    let queries = StringArray::from_iter_values(
        rows.iter()
            .map(|(_, _, q)| serde_json::to_string(q).unwrap_or_else(|_| "[]".into())),
    );
    let batch = RecordBatch::try_new(
        flows_schema(),
        vec![
            Arc::new(ids) as ArrayRef,
            Arc::new(narr) as ArrayRef,
            Arc::new(queries) as ArrayRef,
        ],
    )
    .context("build flows batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "flows", batch).await
}

/// Load persisted flows as `(entry_id, narration, queries)`. `Ok(None)` if absent.
pub async fn read_flows(uri: &str) -> Result<Option<Vec<TextRow>>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("flows").execute().await else {
        return Ok(None);
    };
    let mut out = Vec::new();
    let mut st = tbl.query().execute().await?;
    while let Some(b) = st.try_next().await? {
        let ids = str_col(&b, "symbol_id")?;
        let narr = str_col(&b, "narration")?;
        let queries = str_col(&b, "queries")?;
        for i in 0..b.num_rows() {
            let q: Vec<String> = serde_json::from_str(queries.value(i)).unwrap_or_default();
            out.push((id_from_rendered(ids.value(i)), narr.value(i).to_string(), q));
        }
    }
    Ok(Some(out))
}

/// Blocking wrappers.
pub fn write_flows_blocking(uri: &str, rows: &[TextRow]) -> Result<u64> {
    crate::rt::block_on(write_flows(uri, rows))
}
pub fn read_flows_blocking(uri: &str) -> Result<Option<Vec<TextRow>>> {
    crate::rt::block_on(read_flows(uri))
}

/// Persist the inferred repo style guide (single-row `style_guide` table).
/// Persist one style guide per repo as `(repo, content)` rows.
pub async fn write_style_guide(uri: &str, rows: &[(String, String)]) -> Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
    ]));
    let repos: Vec<&str> = rows.iter().map(|(r, _)| r.as_str()).collect();
    let contents: Vec<&str> = rows.iter().map(|(_, c)| c.as_str()).collect();
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(repos)) as ArrayRef,
            Arc::new(StringArray::from(contents)) as ArrayRef,
        ],
    )
    .context("build style_guide batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "style_guide", batch).await?;
    Ok(())
}

/// Read the persisted per-repo style guides as `(repo, content)` pairs.
pub async fn read_style_guide(uri: &str) -> Result<Vec<(String, String)>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("style_guide").execute().await else {
        return Ok(vec![]);
    };
    let mut st = tbl.query().execute().await?;
    let mut out = Vec::new();
    while let Some(b) = st.try_next().await? {
        let repo = str_col(&b, "repo")?;
        let content = str_col(&b, "content")?;
        for i in 0..b.num_rows() {
            out.push((repo.value(i).to_string(), content.value(i).to_string()));
        }
    }
    Ok(out)
}

/// Blocking wrapper for [`write_style_guide`].
pub fn write_style_guide_blocking(uri: &str, rows: &[(String, String)]) -> Result<()> {
    crate::rt::block_on(write_style_guide(uri, rows))
}

/// Blocking wrapper for [`read_style_guide`].
pub fn read_style_guide_blocking(uri: &str) -> Result<Vec<(String, String)>> {
    crate::rt::block_on(read_style_guide(uri))
}

/// Persist per-file parse facts (JSON per file) so incremental runs can re-resolve everything.
pub async fn write_facts(uri: &str, facts: &[FileFacts]) -> Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("path", DataType::Utf8, false),
        Field::new("facts", DataType::Utf8, false),
    ]));
    let json: Vec<String> = facts
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()
        .context("serialize facts")?;
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                facts.iter().map(|f| f.repo.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                facts.iter().map(|f| f.path.as_str()),
            )),
            Arc::new(StringArray::from(json)),
        ],
    )
    .context("build file_facts batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "file_facts", batch).await?;
    Ok(())
}

/// Read persisted facts; `None` for indexes written before facts were stored.
pub async fn read_facts(uri: &str) -> Result<Option<Vec<FileFacts>>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("file_facts").execute().await else {
        return Ok(None);
    };
    let mut st = tbl.query().execute().await?;
    let mut out = Vec::new();
    while let Some(b) = st.try_next().await? {
        let col = str_col(&b, "facts")?;
        for i in 0..b.num_rows() {
            out.push(serde_json::from_str(col.value(i)).context("parse facts row")?);
        }
    }
    Ok(Some(out))
}

/// Persist derived conventions (e.g. history recipes), JSON per row.
pub async fn write_recipes(uri: &str, rows: &[crate::conventions::Convention]) -> Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("row", DataType::Utf8, false),
    ]));
    let json: Vec<String> = rows
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()
        .context("serialize recipes")?;
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.repo.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from(json)),
        ],
    )
    .context("build recipes batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "recipes", batch).await?;
    Ok(())
}

pub async fn read_recipes(uri: &str) -> Result<Vec<crate::conventions::Convention>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("recipes").execute().await else {
        return Ok(Vec::new());
    };
    let mut st = tbl.query().execute().await?;
    let mut out = Vec::new();
    while let Some(b) = st.try_next().await? {
        let col = str_col(&b, "row")?;
        for i in 0..b.num_rows() {
            out.push(serde_json::from_str(col.value(i)).context("parse recipe row")?);
        }
    }
    Ok(out)
}

pub fn write_recipes_blocking(uri: &str, rows: &[crate::conventions::Convention]) -> Result<()> {
    crate::rt::block_on(write_recipes(uri, rows))
}

pub fn read_recipes_blocking(uri: &str) -> Result<Vec<crate::conventions::Convention>> {
    crate::rt::block_on(read_recipes(uri))
}

/// Persist manifest/lockfile package rows (JSON per row).
pub async fn write_packages(uri: &str, rows: &[crate::manifests::PackageRow]) -> Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo", DataType::Utf8, false),
        Field::new("row", DataType::Utf8, false),
    ]));
    let json: Vec<String> = rows
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()
        .context("serialize packages")?;
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|r| r.repo.as_str()),
            )) as ArrayRef,
            Arc::new(StringArray::from(json)),
        ],
    )
    .context("build packages batch")?;
    let db = crate::storage::connect(uri).execute().await?;
    write_versioned(&db, "packages", batch).await?;
    Ok(())
}

pub async fn read_packages(uri: &str) -> Result<Vec<crate::manifests::PackageRow>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table("packages").execute().await else {
        return Ok(Vec::new());
    };
    let mut st = tbl.query().execute().await?;
    let mut out = Vec::new();
    while let Some(b) = st.try_next().await? {
        let col = str_col(&b, "row")?;
        for i in 0..b.num_rows() {
            out.push(serde_json::from_str(col.value(i)).context("parse package row")?);
        }
    }
    Ok(out)
}

pub fn write_packages_blocking(uri: &str, rows: &[crate::manifests::PackageRow]) -> Result<()> {
    crate::rt::block_on(write_packages(uri, rows))
}

pub fn read_packages_blocking(uri: &str) -> Result<Vec<crate::manifests::PackageRow>> {
    crate::rt::block_on(read_packages(uri))
}

pub fn write_facts_blocking(uri: &str, facts: &[FileFacts]) -> Result<()> {
    crate::rt::block_on(write_facts(uri, facts))
}

pub fn read_facts_blocking(uri: &str) -> Result<Option<Vec<FileFacts>>> {
    crate::rt::block_on(read_facts(uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repo_meta_gains_remotes_and_old_tables_still_read() {
        let dir = std::env::temp_dir().join(format!("comindly-meta-{}", std::process::id()));
        let uri = dir.to_str().unwrap();
        let old = RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("repo", DataType::Utf8, false),
                Field::new("commit", DataType::Utf8, false),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["orders"])) as ArrayRef,
                Arc::new(StringArray::from(vec!["abc"])) as ArrayRef,
            ],
        )
        .unwrap();
        let db = crate::storage::connect(uri).execute().await.unwrap();
        write_versioned(&db, "repo_meta", old).await.unwrap();
        let read = read_indexed_repos(uri).await.unwrap();
        assert_eq!(read[0].remote, None);
        let next = IndexedRepo {
            name: "orders".into(),
            commit: "def".into(),
            remote: Some("https://example.com/orders.git".into()),
        };
        write_repo_commits(uri, std::slice::from_ref(&next))
            .await
            .unwrap();
        assert_eq!(read_indexed_repos(uri).await.unwrap(), vec![next]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
