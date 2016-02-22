extern crate tantivy;
extern crate time;

use tantivy::core::collector::{CountCollector, FirstNCollector, MultiCollector};
use tantivy::core::schema::*;
use tantivy::core::searcher::Searcher;
use tantivy::core::directory::Directory;
use std::io;
use std::convert::From;
use std::path::PathBuf;
use tantivy::core::analyzer::*;
use std::io::BufRead;
use time::PreciseTime;

fn handle_query(searcher: &Searcher, terms: &Vec<Term>, print_fields: &Vec<Field>) -> usize {
    let mut count_collector = CountCollector::new();
    let mut first_3_collector = FirstNCollector::with_limit(3);
    {
        let mut multi_collector = MultiCollector::from(vec!(&mut count_collector, &mut first_3_collector));
        searcher.search(&terms, &mut multi_collector);
    }
    let mut num_docs = 0;
    for doc_address in first_3_collector.docs().iter() {
        let doc = searcher.get_doc(doc_address);
        for print_field in print_fields.iter() {
            for txt in doc.get(print_field) {
                println!("  - txt: {:?}", txt);
            }
        }
    }
    count_collector.count()
}

fn main() {
    let directory = Directory::open(&PathBuf::from("/data/wiki-index/")).unwrap();
    let schema = directory.schema();
    let url_field = schema.field("url").unwrap();
    let title_field = schema.field("title").unwrap();
    let body_field = schema.field("body").unwrap();
    let print_fields = vec!(title_field, url_field);

    let mut directory = Directory::open(&PathBuf::from("/data/wiki-index/")).unwrap();
    let searcher = Searcher::for_directory(directory);
    let tokenizer = SimpleTokenizer::new();

    println!("Ready");
    let stdin = io::stdin();
    loop {
        let mut input = String::new();
        print!("> ");
        stdin.read_line(&mut input);
        if input == "exit\n" {
            break;
        }
        let mut terms: Vec<Term> = Vec::new();
        let mut token_it = tokenizer.tokenize(&input);
        loop {
            match token_it.next() {
                Some(token) => {
                    terms.push(Term::from_field_text(&body_field, &token));
                }
                None => { break; }
            }
        }
        println!("Input: {:?}", input);
        println!("Keywords {:?}", terms);
        let start = PreciseTime::now();
        let num_docs = handle_query(&searcher, &terms, &print_fields);
        let stop = PreciseTime::now();
        println!("Elasped time {:?} microseconds", start.to(stop).num_microseconds().unwrap());
        println!("Num_docs {:?}", num_docs);
    }

}
