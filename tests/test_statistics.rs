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

use rocksdb::*;
use rocksdb::{DBStatisticsTickerType as TickerType, DBStatisticsHistogramType as HistogramType};
use tempdir::TempDir;

#[test]
fn test_db_statistics() {
    let path = TempDir::new("_rust_rocksdb_statistics").expect("");
    let mut opts = DBOptions::new();
    opts.create_if_missing(true);
    opts.enable_statistics();
    let db = DB::open(opts, path.path().to_str().unwrap()).unwrap();
    let wopts = WriteOptions::new();

    db.put_opt(b"k0", b"a", &wopts).unwrap();
    db.put_opt(b"k1", b"b", &wopts).unwrap();
    db.put_opt(b"k2", b"c", &wopts).unwrap();
    db.flush(true /* sync */).unwrap(); // flush memtable to sst file.
    assert_eq!(db.get(b"k0").unwrap().unwrap(), b"a");
    assert_eq!(db.get(b"k1").unwrap().unwrap(), b"b");
    assert_eq!(db.get(b"k2").unwrap().unwrap(), b"c");

    assert!(db.get_statistics_ticker_count(TickerType::BlockCacheHit) > 0);
    assert!(db.get_and_reset_statistics_ticker_count(TickerType::BlockCacheHit) > 0);
    assert_eq!(db.get_statistics_ticker_count(TickerType::BlockCacheHit), 0);
    assert!(db.get_statistics_histogram_string(HistogramType::GetMicros)
        .is_some());
    assert!(db.get_statistics_histogram(HistogramType::GetMicros)
        .is_some());
}
