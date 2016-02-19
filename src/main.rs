extern crate tantivy;
extern crate time;

use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use tantivy::core::postings::VecPostings;
use tantivy::core::postings::Postings;
use tantivy::core::collector::TestCollector;
use tantivy::core::serial::*;
use tantivy::core::schema::*;
use tantivy::core::codec::SimpleCodec;
use tantivy::core::global::*;
use tantivy::core::searcher::Searcher;
use tantivy::core::directory::{Directory, generate_segment_name, SegmentId};
use std::ops::DerefMut;
use tantivy::core::reader::SegmentReader;
use std::io::{ BufWriter, Write};
use std::io;
use std::convert::From;
use std::path::PathBuf;
use tantivy::core::query;
use tantivy::core::query::parse_query;
use tantivy::core::analyzer::SimpleTokenizer;
use std::borrow::Borrow;
use std::io::BufRead;
use std::fs;
use std::io::Cursor;
use time::PreciseTime;

fn count_docs(searcher: &Searcher, terms: &Vec<Term>) -> usize {
    // let terms = vec!(, Term::from_field_text(&body_field, "france"));
    let mut collector = TestCollector::new();
    searcher.search(&terms, &mut collector);
    let mut num_docs = 0;
    for doc_id in collector.docs().iter() {
        num_docs += 1;
    }
    num_docs
}

fn main() {
    let str_fieldtype = FieldOptions::new();
    let text_fieldtype = FieldOptions::new().set_tokenized_indexed();
    let mut schema = Schema::new();
    let id_field = schema.add_field("id", &str_fieldtype);
    let url_field = schema.add_field("url", &str_fieldtype);
    let title_field = schema.add_field("title", &text_fieldtype);
    let body_field = schema.add_field("body", &text_fieldtype);
    let mut directory = Directory::open(&PathBuf::from("/media/ssd/wikiindex")).unwrap();
    directory.set_schema(&schema);
    let searcher = Searcher::for_directory(directory);
    let tokenizer = SimpleTokenizer::new();

    let mut stdin = io::stdin();
    'mainloop: loop {
        let mut input = String::new();
        print!("> ");
        stdin.read_line(&mut input);
        if input == "exit\n" {
            break 'mainloop;
        }
        let mut terms: Vec<Term> = Vec::new();
        let mut token_it = tokenizer.tokenize(&input);
        let mut term_buffer = String::new();
        while token_it.read_one(&mut term_buffer) {
            terms.push(Term::from_field_text(&body_field, &term_buffer));
        }
        // let terms = keywords.iter().map(|s| Term::from_field_text(&body_field, &s));
        println!("Input: {:?}", input);
        println!("Keywords {:?}", terms);
        let start = PreciseTime::now();
        let num_docs = count_docs(&searcher, &terms);
        let stop = PreciseTime::now();
        println!("Elasped time {:?}", start.to(stop));
        println!("Num_docs {:?}", num_docs);
    }

}
