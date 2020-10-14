use clap::ArgMatches;
use serde_json;
use std::convert::From;
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process;
use tantivy;
use tantivy::query::QueryParser;
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::{Index, TERMINATED};

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
        .filter(|&(_, ref field_entry)| match *field_entry.field_type() {
            FieldType::Str(ref text_field_options) => {
                text_field_options.get_indexing_options().is_some()
            }
            _ => false,
        })
        .map(|(field, _)| field)
        .collect();
    let query_parser = QueryParser::new(schema.clone(), default_fields, index.tokenizers().clone());
    let query = query_parser.parse_query(query)?;
    let searcher = index.reader()?.searcher();
    let weight = query.weight(&searcher, false)?;

    let mut stdout = io::BufWriter::new(io::stdout());

    for segment_reader in searcher.segment_readers() {
        let mut scorer = weight.scorer(segment_reader, 1.0)?;
        let store_reader = segment_reader.get_store_reader();
        while scorer.doc() != TERMINATED {
            let doc_id = scorer.doc();
            let doc = store_reader.get(doc_id)?;
            let named_doc = schema.to_named_doc(&doc);
            if let Err(e) = writeln!(stdout, "{}", serde_json::to_string(&named_doc).unwrap()) {
                if e.kind() != ErrorKind::BrokenPipe {
                    eprintln!("{}", e.to_string());
                    process::exit(1)
                }
            }
            scorer.advance();
        }
    }

    stdout.flush()?;

    Ok(())
}
