use tantivy::Index;
use tantivy::schema::{Field, Schema};
use tantivy::query::QueryParser;
use std::path::Path;
use std::io::BufReader;
use std::io::BufRead;
use std::io;
use std::fs::File;
use tantivy::collector::chain;
use tantivy::collector::TopCollector;
use tantivy::collector::CountCollector;
use clap::ArgMatches;
use std::path::PathBuf;
use timer::TimerTree;


pub fn run_bench_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_path = PathBuf::from(matches.value_of("index").unwrap());
    let queries_path = PathBuf::from(matches.value_of("queries").unwrap()); // the unwrap is safe as long as it is comming from the main cli.
    let num_repeat = value_t!(matches, "num_repeat", usize).map_err(|e| format!("Failed to read num_repeat argument as an integer. {:?}", e))?;
    run_bench(&index_path, &queries_path, num_repeat).map_err(From::from)
}


fn extract_search_fields(schema: &Schema) -> Vec<Field> {
    schema.fields()
          .iter()
          .enumerate()
          .filter(|&(_, field_entry)| {
              field_entry.is_indexed()
          })
          .map(|(field_id, _)| Field(field_id as u32))
          .collect()
}

fn read_query_file(query_path: &Path) -> io::Result<Vec<String>> {
    let query_file: File = File::open(&query_path)?;
    let file = BufReader::new(&query_file);
    let mut queries = Vec::new();
    for line_res in file.lines() {
        queries.push(line_res?);
    }
    Ok(queries)
}


fn run_bench(index_path: &Path,
             query_filepath: &Path,
             num_repeat: usize) -> Result<(), String> {
    
    println!("index_path : {:?}", index_path);
    println!("Query : {:?}", index_path);
    println!("-------------------------------\n\n\n");
    
    let index = Index::open_in_dir(index_path).map_err(|e| format!("Failed to open index.\n{:?}", e))?;
    let searcher = index.searcher();
    let default_search_fields: Vec<Field> = extract_search_fields(&index.schema());
    let queries = read_query_file(query_filepath).map_err(|e| format!("Failed reading the query file:  {}", e))?;
    let query_parser = QueryParser::new(index.schema(), default_search_fields, index.tokenizers().clone());
    
    println!("SEARCH\n");
    println!("{}\t{}\t{}\t{}", "query", "num_terms", "num hits", "time in microsecs");
    for _ in 0..num_repeat {
        for query_txt in &queries {
            let query = query_parser.parse_query(&query_txt).unwrap();
            // let num_terms = query.num_terms();
            let mut top_collector = TopCollector::with_limit(10);
            let mut count_collector = CountCollector::default();
            let mut timing = TimerTree::default();
            {
                let _search = timing.open("search");
                let mut collector = chain().push(&mut top_collector).push(&mut count_collector);
                query.search(&searcher, &mut collector)
                    .map_err(|e| format!("Failed while searching query {:?}.\n\n{:?}", query_txt, e))?;
            }
            println!("{}\t{}\t{}", query_txt, count_collector.count(), timing.total_time());
        }
    }
    
    
    println!("\n\nFETCH STORE\n");
    println!("{}\t{}", "query", "time in microsecs");
    for _ in 0..num_repeat {
        for query_txt in &queries {
            let query = query_parser.parse_query(&query_txt).unwrap();
            let mut top_collector = TopCollector::with_limit(10);
            query.search(&searcher, &mut top_collector)
                .map_err(|e| format!("Failed while retrieving document for query {:?}.\n{:?}", query, e))?;
            let mut timer = TimerTree::default();
            {
                let _scoped_timer_ = timer.open("total");
                for doc_address in top_collector.docs() {
                    searcher.doc(&doc_address).unwrap();
                }
            }
            println!("{}\t{}", query_txt, timer.total_time());
        }
    }
    
    Ok(())
}

