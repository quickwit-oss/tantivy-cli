use clap::ArgMatches;
use serde_json;
use std::convert::From;
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process;
use tantivy::aggregation::agg_req::Aggregations;
use tantivy::aggregation::AggregationCollector;
use tantivy::aggregation::AggregationLimits;
use tantivy::query::{EnableScoring, QueryParser};
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::Document;
use tantivy::{self, TantivyDocument};
use tantivy::{Index, TERMINATED};

pub fn run_search_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.get_one::<String>("index").unwrap());
    let query = matches.get_one::<String>("query").unwrap();
    let agg = matches.get_one::<String>("aggregation");
    run_search(&index_directory, &query, &agg).map_err(|e| format!("{:?}", e))
}

fn run_search(
    directory: &Path,
    query: &str,
    agg: &std::option::Option<&String>,
) -> tantivy::Result<()> {
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
    let weight = query.weight(EnableScoring::enabled_from_searcher(&searcher))?;

    let mut stdout = io::BufWriter::new(io::stdout());
    if let Some(agg) = agg {
        let agg_req: Aggregations = serde_json::from_str(agg).unwrap();
        let collector = AggregationCollector::from_aggs(agg_req, AggregationLimits::default());
        let agg_res = searcher.search(&query, &collector).unwrap();

        if let Err(e) = writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&agg_res).unwrap()
        ) {
            if e.kind() != ErrorKind::BrokenPipe {
                eprintln!("{}", e.to_string());
                process::exit(1)
            }
        }
    } else {
        for segment_reader in searcher.segment_readers() {
            let mut scorer = weight.scorer(segment_reader, 1.0)?;
            let store_reader = segment_reader.get_store_reader(100)?;
            while scorer.doc() != TERMINATED {
                let doc_id = scorer.doc();
                let doc: TantivyDocument = store_reader.get(doc_id)?;
                let named_doc = doc.to_named_doc(&schema);
                if let Err(e) = writeln!(stdout, "{}", serde_json::to_string(&named_doc).unwrap()) {
                    if e.kind() != ErrorKind::BrokenPipe {
                        eprintln!("{}", e.to_string());
                        process::exit(1)
                    }
                }
                scorer.advance();
            }
        }
    }
    stdout.flush()?;

    Ok(())
}
