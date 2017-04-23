extern crate tantivy;

use tantivy::{Index, SegmentMeta};
use std::path::PathBuf;
use clap::ArgMatches;
use futures::Future;

const HEAP_SIZE: usize = 300_000_000;


fn error_msg(err: tantivy::Error) -> String {
    format!("Merge failed : {:?}", err)
}

pub fn run_merge_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    let segment_meta = run_merge(index_directory).map_err(error_msg)?;
    println!("Merge finished with segment meta {:?}", segment_meta);
    Ok(())
}


fn run_merge(path: PathBuf) -> tantivy::Result<SegmentMeta> {
    let index = Index::open(&path)?;
    let segments = index.searchable_segment_ids()?;
    index
        .writer(HEAP_SIZE)?
        .merge(&segments)
        .wait()
        .map_err(|_| tantivy::Error::ErrorInThread(String::from("Merge got cancelled")))
}
