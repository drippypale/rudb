use std::time::Instant;

use rudb::store;
use tempfile::tempdir;

fn main() {
    let n = 500;
    let dir = tempdir().unwrap();
    let path = dir.path().join("kvs-test.bin");

    let mut kvs = store::KVStore::open(
        path.as_path(),
        store::Options {
            sync_policy: store::SyncPolicy::Always,
            compact_on_init: false,
        },
    )
    .unwrap();

    let start = Instant::now();
    for i in 0..n {
        kvs.put(format!("key{i}").as_bytes(), b"val1").unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "SyncPolicy::Always Elapsed -> {n} writes in {elapsed:?} → {:.0} writes/sec",
        n as f64 / elapsed.as_secs_f64()
    );

    let mut kvs = store::KVStore::open(
        path.with_extension("spn").as_path(),
        store::Options {
            sync_policy: store::SyncPolicy::Never,
            compact_on_init: false,
        },
    )
    .unwrap();

    let start = Instant::now();
    for i in 0..n {
        kvs.put(format!("key{i}").as_bytes(), b"val1").unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "SyncPolicy::Never Elapsed -> {n} writes in {elapsed:?} → {:.0} writes/sec",
        n as f64 / elapsed.as_secs_f64()
    )
}
