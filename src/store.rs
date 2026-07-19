use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
};

const TOMBSTONE: &[u8] = "\0\0\0\0".as_bytes();

pub struct KVStore {
    map: HashMap<Vec<u8>, u64>, // key: offset
    w_file: File,               // [key_len][val_len][key][val]
    r_file: File,
    cursor: u64,
}

struct Entry {
    bytes: u64,
    // key_len: u32,
    // val_len: u32,
    key: Vec<u8>,
    val: Vec<u8>,
}

impl KVStore {
    pub fn open(p: &str) -> Result<Self, io::Error> {
        let wf = File::options().create(true).append(true).open(p)?;
        let rf = File::open(p)?;

        let mut kvs = Self {
            map: HashMap::new(),
            w_file: wf,
            r_file: rf,
            cursor: 0,
        };

        kvs.init_map()?;

        Ok(kvs)
    }
    pub fn put(&mut self, k: &[u8], v: &[u8]) -> Result<u64, io::Error> {
        let k_len = (k.len() as u32).to_le_bytes();
        let v_len = (v.len() as u32).to_le_bytes();
        let line = [&k_len, &v_len, k, v];

        let bytes = line.concat();

        self.w_file.write_all(&bytes)?;

        self.map.insert(k.to_vec(), self.cursor);
        self.cursor += bytes.len() as u64;

        Ok(bytes.len() as u64)
    }
    pub fn get(&mut self, k: &[u8]) -> Result<Option<Vec<u8>>, io::Error> {
        match self.map.get(k) {
            Some(offset) => {
                let entry = self.read_entry(*offset)?;
                Ok(Some(entry.val))
            }
            None => Ok(None),
        }
    }
    pub fn del(&mut self, k: &[u8]) -> Result<Option<Vec<u8>>, io::Error> {
        let v = self.get(k)?;
        self.put(k, TOMBSTONE)?;
        self.map.remove(k);

        Ok(v)
    }
    fn init_map(&mut self) -> Result<(), io::Error> {
        loop {
            match self.read_entry(self.cursor) {
                Ok(entry) => {
                    if entry.val == TOMBSTONE {
                        self.map.remove(entry.key.as_slice());
                    } else {
                        self.map.insert(entry.key, self.cursor);
                    }
                    self.cursor += entry.bytes;
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn read_entry(&mut self, offset: u64) -> Result<Entry, io::Error> {
        self.r_file.seek(SeekFrom::Start(offset))?;

        let mut buf: [u8; 8] = [0; 8];

        self.r_file.read_exact(&mut buf)?;

        let key_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let val_len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

        let mut key = vec![0; key_len as usize];
        let mut val = vec![0; val_len as usize];

        self.r_file.read_exact(&mut key)?;
        self.r_file.read_exact(&mut val)?;

        Ok(Entry {
            bytes: (4 + 4 + key_len + val_len) as u64,
            // key_len,
            // val_len,
            key,
            val,
        })
    }
}
