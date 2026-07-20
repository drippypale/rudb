use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(PartialEq, Clone, Copy)]
enum Flag {
    Set = 0,
    Tombstone = 1,
}

#[derive(PartialEq, Clone, Copy)]
pub enum SyncPolicy {
    Always,
    Never,
    // EverySec,  TODO: : Add it later
}

impl TryFrom<u8> for Flag {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Set),
            1 => Ok(Self::Tombstone),
            _ => Err(()),
        }
    }
}

pub struct KVStore {
    map: HashMap<Vec<u8>, u64>, // key: offset
    path: PathBuf,
    w_file: File, // [key_len][val_len][key][val]
    r_file: File,
    cursor: u64,
    sync_policy: SyncPolicy,
}

#[derive(Clone, Copy)]
pub struct Options {
    pub sync_policy: SyncPolicy,
    pub compact_on_init: bool,
}

struct Entry {
    flag: Flag,
    key: Vec<u8>,
    val: Vec<u8>,
}

impl Entry {
    fn key_len_le_bytes(&self) -> [u8; 4] {
        (self.key.len() as u32).to_le_bytes()
    }
    fn val_len_le_bytes(&self) -> [u8; 4] {
        (self.val.len() as u32).to_le_bytes()
    }
    fn to_bytes(&self) -> Vec<u8> {
        let line = [
            &self.key_len_le_bytes(),
            &self.val_len_le_bytes(),
            self.key.as_slice(),
            self.val.as_slice(),
        ];

        let mut bytes = vec![self.flag as u8];
        bytes.extend_from_slice(&line.concat());
        bytes
    }

    fn bytes_len(&self) -> u64 {
        self.to_bytes().len() as u64
    }

    fn write_all(&self, f: &mut File, fsync: bool) -> Result<u64, io::Error> {
        let bytes = self.to_bytes();
        f.write_all(&bytes)?;

        if fsync {
            f.sync_all()?;
        }

        Ok(bytes.len() as u64)
    }
}

impl KVStore {
    pub fn open(p: &Path, options: Options) -> Result<Self, io::Error> {
        let wf = File::options()
            .create(true)
            .append(true)
            .open(p.as_os_str())?;
        let rf = File::open(p.as_os_str())?;

        let mut kvs = Self {
            map: HashMap::new(),
            path: p.to_path_buf(),
            w_file: wf,
            r_file: rf,
            cursor: 0,
            sync_policy: options.sync_policy,
        };

        kvs.init_map()?;
        if options.compact_on_init {
            kvs.compact()?;
        }

        Ok(kvs)
    }
    pub fn put(&mut self, k: &[u8], v: &[u8]) -> Result<u64, io::Error> {
        let e = Entry {
            flag: Flag::Set,
            key: k.to_vec(),
            val: v.to_vec(),
        };

        e.write_all(&mut self.w_file, self.sync_policy == SyncPolicy::Always)?;

        self.map.insert(k.to_vec(), self.cursor);
        self.cursor += e.bytes_len();

        Ok(e.bytes_len())
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
        let e = Entry {
            flag: Flag::Tombstone,
            key: k.to_vec(),
            val: vec![],
        };

        e.write_all(&mut self.w_file, self.sync_policy == SyncPolicy::Always)?;
        self.map.remove(k);

        Ok(v)
    }
    pub fn compact(&mut self) -> Result<(), io::Error> {
        let temp_path = self.path.with_extension("compact");
        let mut wf = File::create(&temp_path)?;

        let mut new_cursor = 0u64;
        let mut new_map: HashMap<Vec<u8>, u64> = HashMap::new();

        for (key, offset) in self.map.clone() {
            let e = self.read_entry(offset)?;
            new_map.insert(key, new_cursor);
            new_cursor += e.write_all(&mut wf, false)?;
        }

        wf.sync_all()?;
        fs::rename(&temp_path, &self.path)?;

        self.w_file = File::options().create(true).append(true).open(&self.path)?;
        self.r_file = File::open(&self.path)?;
        self.cursor = new_cursor;
        self.map = new_map;

        Ok(())
    }
    fn init_map(&mut self) -> Result<(), io::Error> {
        loop {
            match self.read_entry(self.cursor) {
                Ok(entry) => {
                    match entry.flag {
                        Flag::Tombstone => self.map.remove(&entry.key),
                        Flag::Set => self.map.insert(entry.key.clone(), self.cursor),
                    };
                    self.cursor += entry.bytes_len();
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn read_entry(&mut self, offset: u64) -> Result<Entry, io::Error> {
        self.r_file.seek(SeekFrom::Start(offset))?;

        let mut flag: [u8; 1] = [0; 1];
        self.r_file.read_exact(&mut flag)?;

        let mut buf: [u8; 8] = [0; 8];

        self.r_file.read_exact(&mut buf)?;

        let key_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let val_len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

        let mut key = vec![0; key_len as usize];
        let mut val = vec![0; val_len as usize];

        self.r_file.read_exact(&mut key)?;
        self.r_file.read_exact(&mut val)?;

        Ok(Entry {
            flag: flag[0]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid flag byte"))?,
            key,
            val,
        })
    }
}

#[cfg(test)]
mod test {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn persistence_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("kvs-test.bin");

        let options = Options {
            sync_policy: SyncPolicy::Always,
            compact_on_init: false,
        };

        let mut kvs = KVStore::open(path.as_path(), options.clone()).unwrap();
        kvs.put("key1".as_bytes(), "val1".as_bytes()).unwrap();

        kvs.put("key2".as_bytes(), "val2".as_bytes()).unwrap();
        kvs.put("key2".as_bytes(), "val2-override".as_bytes())
            .unwrap();

        kvs.put("key3".as_bytes(), "val3".as_bytes()).unwrap();
        kvs.del("key3".as_bytes()).unwrap();

        let mut kvs2 = KVStore::open(path.as_path(), options.clone()).unwrap();
        assert_eq!(
            kvs2.get("key1".as_bytes()).unwrap(),
            Some("val1".as_bytes().to_vec())
        );
        assert_eq!(
            kvs2.get("key2".as_bytes()).unwrap(),
            Some("val2-override".as_bytes().to_vec())
        );
        assert_eq!(kvs2.get("key3".as_bytes()).unwrap(), None);
    }
    #[test]
    fn compact() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("kvs-test.bin");

        let options = Options {
            sync_policy: SyncPolicy::Always,
            compact_on_init: false,
        };

        let mut kvs = KVStore::open(path.as_path(), options).unwrap();
        kvs.put("key1".as_bytes(), "val1".as_bytes()).unwrap();

        kvs.put("key2".as_bytes(), "val2".as_bytes()).unwrap();

        kvs.put("key3".as_bytes(), "val3".as_bytes()).unwrap();
        kvs.del("key3".as_bytes()).unwrap();

        let before = fs::metadata(&path).unwrap().len();
        kvs.compact().unwrap();
        let after = fs::metadata(&path).unwrap().len();

        assert!(after < before);

        assert_eq!(
            kvs.get("key1".as_bytes()).unwrap(),
            Some("val1".as_bytes().to_vec())
        );
        assert_eq!(
            kvs.get("key2".as_bytes()).unwrap(),
            Some("val2".as_bytes().to_vec())
        );
        assert_eq!(kvs.get("key3".as_bytes()).unwrap(), None);
    }
}
