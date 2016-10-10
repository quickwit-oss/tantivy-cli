extern crate tantivy;

use tantivy::Index;
use std::path::PathBuf;
use clap::ArgMatches;

const HEAP_SIZE: usize = 300_000_000;

pub fn run_merge_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    run_merge(index_directory).map_err(|e| format!("Indexing failed : {:?}", e))
}


fn run_merge(path: PathBuf) -> tantivy::Result<()> {
    let index = try!(Index::open(&path));
    let segments = index.searchable_segments();
    let mut index_writer = try!(index.writer(HEAP_SIZE));
    index_writer.merge(&segments)
}
