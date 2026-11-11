# FAQ

**Does code leave my infrastructure?** Not unless you enable LLM steps. Parsing, resolution, embeddings,
search and the ledger run locally and write to the directory or bucket you choose. `enrich`, `flows`,
and `guide` send code to the LLM endpoint you configure; `--no-llm`
turns all of them off.

**Do I need SCIP indexers?** No. tree-sitter resolution covers the supported languages; when a SCIP index
or indexer is present, its answers are used as well.

**Does it read agent session transcripts?** No. The ledger fills only from people (`comindly remember`) and from
agents through MCP and hooks.

**How fresh is the index?** As fresh as the last build. CI builds a new version on each merge; readers
switch atomically, and hooks on a remote index read a local mirror that refreshes in the background.

**What if a record is wrong?** Invalidate it (`comindly review <id> invalidate`), or archive it if it no longer applies. Records follow the code: moved or
changed code adds a note for the next reader to check, deleted code orphans them, and agents only see
trusted ones.

**Which languages?** Python, TypeScript, JavaScript, Go, Java, Kotlin and Rust.
