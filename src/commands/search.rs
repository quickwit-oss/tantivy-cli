use clap::ArgMatches;
use rustc_serialize::json::as_json;
use std::convert::From;
use std::path::Path;
use std::path::PathBuf;
use tantivy;
use tantivy::Index;
use tantivy::query::QueryParser;
use tantivy::schema::Field;
use tantivy::schema::FieldType;

pub fn run_search_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.value_of("index").unwrap());
    let query = matches.value_of("query").unwrap();
    run_search(&index_directory, &query).map_err(|e| format!("{:?}", e))
}

fn run_search(directory: &Path, query: &str) -> tantivy::Result<()> {     
    let index = Index::open(directory)?;
    let schema = index.schema();
    let default_fields: Vec<Field> = schema
        .fields()
        .iter()
        .enumerate()
        .filter(
            |&(_, ref field_entry)| {
                match *field_entry.field_type() {
                    FieldType::Str(ref text_field_options) => {
                        text_field_options.get_indexing_options().is_indexed()
                    },
                    FieldType::I64(_) => false,
                    FieldType::U64(_) => false
                }
            }
        )
        .map(|(i, _)| Field(i as u32))
        .collect();
    let query_parser = QueryParser::new(schema.clone(), default_fields);
    let query = query_parser.parse_query(query)?;
    let searcher = index.searcher();
    let weight = query.weight(&searcher)?;
    let schema = index.schema();
    for segment_reader in searcher.segment_readers() {
        let mut scorer = try!(weight.scorer(segment_reader));
        while scorer.advance() {
            let doc_id = scorer.doc();
            let doc = segment_reader.doc(doc_id)?;
            let named_doc = schema.to_named_doc(&doc);
            println!("{}", as_json(&named_doc));
        }
    }
    Ok(())
}
