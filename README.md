# rudb

A small persistent key-value **storage engine**, written from scratch in Rust.

It's a hands-on learning project — the goal is to understand database internals
(on-disk data structures, durability, crash recovery, concurrency) and to learn Rust
by earning every feature instead of reaching for a crate. Part of my
[project backlog](https://github.com/drippypale/backlog).

## Design (current)

An **append-only log** in the style of [Bitcask](https://riak.com/assets/bitcask-intro.pdf):
keys live in memory, values live on disk.

- **Log file** — every write *appends* a length-prefixed record, never modifies in place:

  ```
  [flag: u8][key_len: u32 LE][val_len: u32 LE][key bytes][val bytes]
  ```

  `flag` is `0` = set, `1` = tombstone — deletion lives in the record format, not the
  value bytes, so any byte pattern is a legal value.
- **In-memory index** — `HashMap<Vec<u8>, u64>` mapping each key to the **byte offset**
  of its latest record in the log. The file is the source of truth; the map just says
  where to look.
- **`get`** — index lookup → `seek` to the offset → read the record → return the value.
- **`del`** — appends a tombstone record and drops the key from the index; replay
  treats a tombstone as "remove this key".
- **Startup replay** — on `open`, the log is scanned front-to-back to rebuild the index.
  Records are replayed in file order, so the **last write for a key wins**. A clean
  `UnexpectedEof` ends replay; a torn tail record from a crash is safely ignored.
- **Compaction** — `compact()` rewrites the log with only the live records (those the
  index points at), then atomically swaps it in (`fsync` temp → `rename` → reopen
  handles → rebuild index). Dead records and tombstones are reclaimed.
- **Durability** — a per-store `SyncPolicy` (`Always` = `fsync` every write; `Never` =
  let the OS flush) trades durability against throughput. See
  [`examples/durability_bench.rs`](examples/durability_bench.rs) — on this dev machine,
  ~271 writes/s (`Always`, true `F_FULLFSYNC`) vs ~107k writes/s (`Never`): a ~400× gap.

Keys and values are arbitrary **bytes** (`Vec<u8>` / `&[u8]`), never assumed to be UTF-8.

## Usage

```sh
cargo run                 # starts a REPL over stdin
```

Commands:

| Command        | Effect                                  |
| -------------- | --------------------------------------- |
| `put <k> <v>`  | store/overwrite a value                 |
| `get <k>`      | print the value, or `not found`         |
| `del <k>`      | delete a key                            |
| `q` / EOF      | quit                                    |

Data persists across restarts — `put` a key, quit, restart, `get` it back.

## Roadmap

Built stage by stage; each stage is a self-contained, shippable milestone.

- [x] **Stage 0 — In-memory KV store.** `HashMap`-backed `get`/`put`/`del` + REPL.
- [x] **Stage 1 — Append-only log.** Length-prefixed records, in-memory offset index,
      tombstone deletes, crash-safe startup replay. Data survives a restart.
- [x] **Stage 2 — Compaction & durability.** Flag-based tombstones, crash-safe log
      compaction (atomic rename swap), configurable `fsync` policy + a benchmark proving
      the durability/throughput tradeoff. _(Segment files deferred as a stretch goal.)_
- [ ] **Stage 3 — On-disk B+Tree.** Page-based index; store more keys than fit in RAM;
      ordered iteration & range scans.
- [ ] **Stage 4 — WAL + crash recovery.** Write-ahead log, buffer pool with LRU eviction.
- [ ] **Stage 5 — Transactions & concurrency.** Single-writer/multi-reader, then MVCC.
- [ ] **Stage 6 — Fork:** a query layer, an LSM-tree variant, or a network protocol (RESP).

## Known limitations (tracked for later)

- **Compaction is manual** — `compact()` must be called explicitly (or via
  `compact_on_init`); there's no automatic trigger on a size/dead-ratio threshold yet.
- **No group commit** — `SyncPolicy` is all-or-nothing (`Always`/`Never`); an
  `EverySec`-style batched fsync (near-`Never` throughput, bounded loss window) is the
  obvious next durability mode.
- **Whole index in RAM** — every key must fit in memory (Bitcask's tradeoff); addressed
  by the on-disk B+Tree in Stage 3.
- **Reads need `&mut`** — `get` seeks a shared file handle, so it takes `&mut self`
  (no concurrent readers). Revisited in Stage 5.
