use clap::ArgMatches;
use serde_json;
use std::convert::From;
use std::path::Path;
use std::path::PathBuf;
use tantivy;
use tantivy::query::QueryParser;
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::Index;

pub fn run_search_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.value_of("index").unwrap());
    let query = matches.value_of("query").unwrap();
    run_search(&index_directory, &query).map_err(|e| format!("{:?}", e))
}

fn run_search(directory: &Path, query: &str) -> tantivy::Result<()> {
    let index = Index::open_in_dir(directory)?;
    let schema = index.schema();
    let default_fields: Vec<Field> = schema
        .fields()
        .iter()
        .enumerate()
        .filter(|&(_, ref field_entry)| match *field_entry.field_type() {
            FieldType::Str(ref text_field_options) => {
                text_field_options.get_indexing_options().is_some()
            }
            _ => false,
        })
        .map(|(i, _)| Field(i as u32))
        .collect();
    let query_parser = QueryParser::new(schema.clone(), default_fields, index.tokenizers().clone());
    let query = query_parser.parse_query(query)?;
    let searcher = index.reader()?.searcher();
    let weight = query.weight(&searcher, false)?;
    let schema = index.schema();
    for segment_reader in searcher.segment_readers() {
        let mut scorer = weight.scorer(segment_reader)?;
        let store_reader = segment_reader.get_store_reader();
        while scorer.advance() {
            let doc_id = scorer.doc();
            let doc = store_reader.get(doc_id)?;
            let named_doc = schema.to_named_doc(&doc);
            println!("{}", serde_json::to_string(&named_doc).unwrap());
        }
    }
    Ok(())
}
