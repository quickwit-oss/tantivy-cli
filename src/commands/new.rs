use clap::ArgMatches;
use std::convert::From;
use std::path::PathBuf;
use tantivy;
use tantivy::schema::{Schema, STRING, STORED, TEXT};
use tantivy::Index;

fn default_schema() -> Schema {
    let mut schema = Schema::new();
    schema.add_text_field("url", STRING | STORED);
    schema.add_text_field("title", TEXT | STORED);
    schema.add_text_field("body", TEXT | STORED);
    schema
}

pub fn run_new_cli(matches: &ArgMatches) -> tantivy::Result<()> {
    let index_directory = PathBuf::from(matches.value_of("index").unwrap());
    run_new(index_directory)   
}

fn run_new(directory: PathBuf) -> tantivy::Result<()> {
    let schema = default_schema();
    let mut index = try!(Index::create(&directory, schema));
    index.save_metas()
}

