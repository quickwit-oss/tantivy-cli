extern crate tantivy;

use tantivy::Index;
use std::path::PathBuf;
use clap::ArgMatches;
use futures::Future;

const HEAP_SIZE: usize = 300_000_000;

pub fn run_merge_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    run_merge(index_directory)
}


fn run_merge(path: PathBuf) -> Result<(), String> {
    let index = Index::open(&path).unwrap();
    let segments = index.searchable_segment_ids().unwrap();
    let mut index_writer = index.writer(HEAP_SIZE).unwrap();
    index_writer
        .merge(&segments)
        .wait()
        .map_err(|e| format!("Indexing failed : {:?}", e))?;
    Ok(())
}
