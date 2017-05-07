extern crate tantivy;

use tantivy::Index;
use std::path::PathBuf;
use clap::ArgMatches;
use futures::Future;

const HEAP_SIZE: usize = 300_000_000;


fn error_msg(err: tantivy::Error) -> String {
    format!("Merge failed : {:?}", err)
}

pub fn run_merge_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    run_merge(index_directory).map_err(error_msg)
    // we rollback to force a gc.

}


fn run_merge(path: PathBuf) -> tantivy::Result<()> {
    let index = Index::open(&path)?;
    let segments = index.searchable_segment_ids()?;
    let segment_meta = index
        .writer(HEAP_SIZE)?
        .merge(&segments)
        .wait()
        .map_err(|_| tantivy::Error::ErrorInThread(String::from("Merge got cancelled")));
    println!("Merge finished with segment meta {:?}", segment_meta);
    println!("Garbage collect irrelevant segments.");
    Index::open(&path)?
        .writer_with_num_threads(1, 40_000_000)?
        .garbage_collect_files()?;
    Ok(())
}
