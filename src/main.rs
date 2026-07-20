use std::{error::Error, io, path::Path};

use rudb::store;

fn main() -> Result<(), Box<dyn Error>> {
    let mut s = store::KVStore::open(
        Path::new("kvstore.bin"),
        store::Options {
            sync_policy: store::SyncPolicy::Always,
            compact_on_init: true,
        },
    )
    .unwrap();
    let mut q = String::new();

    loop {
        q.clear();
        let n = io::stdin().read_line(&mut q).expect("Failed to read");
        if n == 0 {
            return Ok(());
        }
        let comps: Vec<&str> = q.split_whitespace().collect();
        match comps.as_slice() {
            ["q"] => return Ok(()),
            ["put", kstr, vstr] => match s.put(kstr.as_bytes(), vstr.as_bytes()) {
                Ok(n) => println!("Wrote {n} bytes."),
                Err(e) => println!("{e}"),
            },
            ["get", kstr] => match s.get(kstr.as_bytes())? {
                Some(v) => {
                    let vstr = str::from_utf8(v.as_slice()).expect("Couldn't convert the value");
                    println!("{vstr}")
                }
                None => println!("not found"),
            },
            ["del", kstr] => match s.del(kstr.as_bytes())? {
                Some(v) => {
                    let vstr = String::from_utf8(v).expect("Couldn't convert the value");
                    println!("{vstr}")
                }
                None => println!("not found"),
            },
            _ => {
                println!("Unknown command ...");
            }
        }
    }
}
