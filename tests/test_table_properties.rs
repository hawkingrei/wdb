// Copyright 2017 PingCAP, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// See the License for the specific language governing permissions and
// limitations under the License.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use rocksdb::{DB, Range, ColumnFamilyOptions, DBOptions, Writable, DBEntryType,
              TablePropertiesCollection, TablePropertiesCollector,
              TablePropertiesCollectorFactory, UserCollectedProperties};
use std::collections::HashMap;
use std::fmt;
use tempdir::TempDir;

enum Props {
    NumKeys = 0,
    NumPuts,
    NumMerges,
    NumDeletes,
}

fn encode_u32(x: u32) -> Vec<u8> {
    let mut w = Vec::new();
    w.write_u32::<LittleEndian>(x).unwrap();
    w
}

fn decode_u32(mut x: &[u8]) -> u32 {
    x.read_u32::<LittleEndian>().unwrap()
}

struct ExampleCollector {
    num_keys: u32,
    num_puts: u32,
    num_merges: u32,
    num_deletes: u32,
    last_key: Vec<u8>,
}

impl ExampleCollector {
    fn new() -> ExampleCollector {
        ExampleCollector {
            num_keys: 0,
            num_puts: 0,
            num_merges: 0,
            num_deletes: 0,
            last_key: Vec::new(),
        }
    }

    fn add(&mut self, other: &ExampleCollector) {
        self.num_keys += other.num_keys;
        self.num_puts += other.num_puts;
        self.num_merges += other.num_merges;
        self.num_deletes += other.num_deletes;
    }

    fn encode(&self) -> HashMap<Vec<u8>, Vec<u8>> {
        let mut props = HashMap::new();
        props.insert(vec![Props::NumKeys as u8], encode_u32(self.num_keys));
        props.insert(vec![Props::NumPuts as u8], encode_u32(self.num_puts));
        props.insert(vec![Props::NumMerges as u8], encode_u32(self.num_merges));
        props.insert(vec![Props::NumDeletes as u8], encode_u32(self.num_deletes));
        props
    }

    fn decode(props: &UserCollectedProperties) -> ExampleCollector {
        assert!(!props.is_empty());
        let mut c = ExampleCollector::new();
        c.num_keys = decode_u32(&props[&[Props::NumKeys as u8]]);
        c.num_puts = decode_u32(&props[&[Props::NumPuts as u8]]);
        c.num_merges = decode_u32(&props[&[Props::NumMerges as u8]]);
        c.num_deletes = decode_u32(&props[&[Props::NumDeletes as u8]]);

        for (k, v) in props {
            assert_eq!(v, props.get(k).unwrap());
        }
        assert!(props.get(&[Props::NumKeys as u8, Props::NumPuts as u8]).is_none());
        assert!(props.len() >= 4);

        c
    }
}

impl fmt::Display for ExampleCollector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "keys={}, puts={}, merges={}, deletes={}",
               self.num_keys,
               self.num_puts,
               self.num_merges,
               self.num_deletes)
    }
}

impl TablePropertiesCollector for ExampleCollector {
    fn add(&mut self, key: &[u8], _: &[u8], entry_type: DBEntryType, _: u64, _: u64) {
        if key != self.last_key.as_slice() {
            self.num_keys += 1;
            self.last_key.clear();
            self.last_key.extend_from_slice(key);
        }
        match entry_type {
            DBEntryType::Put => self.num_puts += 1,
            DBEntryType::Merge => self.num_merges += 1,
            DBEntryType::Delete |
            DBEntryType::SingleDelete => self.num_deletes += 1,
            DBEntryType::Other => {}
        }
    }

    fn finish(&mut self) -> HashMap<Vec<u8>, Vec<u8>> {
        self.encode()
    }
}

struct ExampleFactory {}

impl ExampleFactory {
    fn new() -> ExampleFactory {
        ExampleFactory {}
    }
}

impl TablePropertiesCollectorFactory for ExampleFactory {
    fn create_table_properties_collector(&mut self, _: u32) -> Box<TablePropertiesCollector> {
        Box::new(ExampleCollector::new())
    }
}

fn check_collection(collection: &TablePropertiesCollection,
                    num_files: usize,
                    num_keys: u32,
                    num_puts: u32,
                    num_merges: u32,
                    num_deletes: u32) {
    let mut res = ExampleCollector::new();
    assert!(!collection.is_empty());
    let props: HashMap<_, _> = collection.iter().collect();
    assert_eq!(props.len(), collection.len());
    for (k, v) in &props {
        assert!(k.ends_with(".sst"));
        assert_eq!(v.property_collectors_names(), "[example-collector]");
        res.add(&ExampleCollector::decode(v.user_collected_properties()));
    }
    assert_eq!(props.len(), num_files);
    assert_eq!(res.num_keys, num_keys);
    assert_eq!(res.num_puts, num_puts);
    assert_eq!(res.num_merges, num_merges);
    assert_eq!(res.num_deletes, num_deletes);
}

#[test]
fn test_table_properties_collector_factory() {
    let f = ExampleFactory::new();
    let mut opts = DBOptions::new();
    let mut cf_opts = ColumnFamilyOptions::new();
    opts.create_if_missing(true);
    cf_opts.add_table_properties_collector_factory("example-collector", Box::new(f));

    let path = TempDir::new("_rust_rocksdb_collectortest").expect("");
    let db = DB::open_cf(opts,
                         path.path().to_str().unwrap(),
                         vec!["default"],
                         vec![cf_opts])
        .unwrap();

    let samples = vec![(b"key1".to_vec(), b"value1".to_vec()),
                       (b"key2".to_vec(), b"value2".to_vec()),
                       (b"key3".to_vec(), b"value3".to_vec()),
                       (b"key4".to_vec(), b"value4".to_vec())];

    // Put 4 keys.
    for &(ref k, ref v) in &samples {
        db.put(k, v).unwrap();
        assert_eq!(v.as_slice(), &*db.get(k).unwrap().unwrap());
    }
    db.flush(true).unwrap();
    let collection = db.get_properties_of_all_tables().unwrap();
    check_collection(&collection, 1, 4, 4, 0, 0);

    // Delete 2 keys.
    let cf = db.cf_handle("default").unwrap();
    for &(ref k, _) in &samples[0..2] {
        db.delete_cf(cf, k).unwrap();
    }
    db.flush_cf(cf, true).unwrap();
    let collection = db.get_properties_of_all_tables_cf(cf).unwrap();
    check_collection(&collection, 2, 6, 4, 0, 2);

    // ["key2", "key3") covers two sst files.
    let range = Range::new(b"key2", b"key3");
    let collection = db.get_properties_of_tables_in_range(cf, &[range]).unwrap();
    check_collection(&collection, 2, 6, 4, 0, 2);

    // ["key3", "key4") covers only the first sst file.
    let range = Range::new(b"key3", b"key4");
    let collection = db.get_properties_of_tables_in_range(cf, &[range]).unwrap();
    check_collection(&collection, 1, 4, 4, 0, 0);
}
