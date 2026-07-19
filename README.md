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
  [key_len: u32 LE][val_len: u32 LE][key bytes][val bytes]
  ```

- **In-memory index** — `HashMap<Vec<u8>, u64>` mapping each key to the **byte offset**
  of its latest record in the log. The file is the source of truth; the map just says
  where to look.
- **`get`** — index lookup → `seek` to the offset → read the record → return the value.
- **`del`** — appends a *tombstone* record and drops the key from the index; replay
  treats a tombstone as "remove this key".
- **Startup replay** — on `open`, the log is scanned front-to-back to rebuild the index.
  Records are replayed in file order, so the **last write for a key wins**. A clean
  `UnexpectedEof` ends replay; a torn tail record from a crash is safely ignored.

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
- [ ] **Stage 2 — Compaction & durability.** Compact the log (drop dead/overwritten
      records), segment files, explicit `fsync` control. Robust tombstone format.
- [ ] **Stage 3 — On-disk B+Tree.** Page-based index; store more keys than fit in RAM;
      ordered iteration & range scans.
- [ ] **Stage 4 — WAL + crash recovery.** Write-ahead log, buffer pool with LRU eviction.
- [ ] **Stage 5 — Transactions & concurrency.** Single-writer/multi-reader, then MVCC.
- [ ] **Stage 6 — Fork:** a query layer, an LSM-tree variant, or a network protocol (RESP).

## Known limitations (tracked for later)

- **Tombstone collides with data** — a delete is encoded as the value `\0\0\0\0`, so a
  legitimate 4-null-byte value would be read as a deletion. To be fixed in Stage 2 by
  moving deletion into the record format (a type/flag field) rather than the value bytes.
- **No compaction yet** — the log grows unbounded; overwritten and deleted records are
  never reclaimed until Stage 2.
- **Reads need `&mut`** — `get` seeks a shared file handle, so it takes `&mut self`
  (no concurrent readers). Revisited in Stage 5.
