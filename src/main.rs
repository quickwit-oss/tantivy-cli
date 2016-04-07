extern crate tantivy;
extern crate time;
extern crate urlencoded;

use tantivy::collector::{CountCollector, FirstNCollector, MultiCollector};
use tantivy::schema::*;
use tantivy::Searcher;
use tantivy::Directory;
use std::io;
use std::convert::From;
use std::path::PathBuf;
use std::io::BufRead;
use time::PreciseTime;
use urlencoded::UrlEncodedQuery;
use iron::status;

extern crate iron;
extern crate staticfile;
extern crate mount;

// This example serves the docs from target/doc/staticfile at /doc/
//
// Run `cargo doc && cargo test && ./target/doc_server`, then
// point your browser to http://127.0.0.1:3000/doc/

use std::path::Path;

use staticfile::Static;
use mount::Mount;
use iron::prelude::*;

fn handle_query(searcher: &Searcher, terms: &Vec<Term>, print_fields: &Vec<TextField>) -> usize {
    let mut count_collector = CountCollector::new();
    let mut first_3_collector = FirstNCollector::with_limit(3);
    {
        let mut multi_collector = MultiCollector::from(vec!(&mut count_collector, &mut first_3_collector));
        searcher.search(&terms, &mut multi_collector);
    }
    let mut num_docs = 0;
    for doc_address in first_3_collector.docs().iter() {
        let doc = searcher.doc(doc_address).unwrap();
        for print_field in print_fields.iter() {
            for txt in doc.get_texts(print_field) {
                println!("  - txt: {:?}", txt);
            }
        }
    }
    count_collector.count()
}

// fn hello_world(_: &mut Request) -> IronResult<Response> {
//     Ok(Response::with((iron::status::Ok, "Hello World")))
// }


fn search(req: &mut Request) -> IronResult<Response> {
    // Extract the decoded data as hashmap, using the UrlEncodedQuery plugin.
    match req.get_ref::<UrlEncodedQuery>() {
        Ok(ref qs_map) => {
            println!("Parsed GET request query string:\n {:?}", qs_map);
            println!("{:?}", qs_map.get("q"));
            match qs_map.get("q") {
                Some(qs) => {
                    Ok(Response::with((status::Ok, format!("Hello!, {:?}", qs)) ))
                }
                None => {
                    Ok(Response::with((status::BadRequest, "Query not defined")))
                }
            }
        }
        Err(ref e) => Ok(Response::with((status::BadRequest, "Failed to parse query string")))
    }
}

fn main() {
    // let directory = Directory::open(&PathBuf::from("/data/wiki-index/")).unwrap();
    // let schema = directory.schema();
    // let url_field = schema.field("url").unwrap();
    // let title_field = schema.field("title").unwrap();
    // let body_field = schema.field("body").unwrap();
    // let print_fields = vec!(title_field, url_field);
    //
    // let mut directory = Directory::open(&PathBuf::from("/data/wiki-index/")).unwrap();
    // let searcher = Searcher::for_directory(directory);
    // let tokenizer = SimpleTokenizer::new();
    //
    // println!("Ready");
    // let stdin = io::stdin();
    // loop {
    //     let mut input = String::new();
    //     print!("> ");
    //     stdin.read_line(&mut input);
    //     if input == "exit\n" {
    //         break;
    //     }
    //     let mut terms: Vec<Term> = Vec::new();
    //     let mut token_it = tokenizer.tokenize(&input);
    //     loop {
    //         match token_it.next() {
    //             Some(token) => {
    //                 terms.push(Term::from_field_text(&body_field, &token));
    //             }
    //             None => { break; }
    //         }
    //     }
    //     println!("Input: {:?}", input);
    //     println!("Keywords {:?}", terms);
    //     let start = PreciseTime::now();
    //     let num_docs = handle_query(&searcher, &terms, &print_fields);
    //     let stop = PreciseTime::now();
    //     println!("Elasped time {:?} microseconds", start.to(stop).num_microseconds().unwrap());
    //     println!("Num_docs {:?}", num_docs);

        let mut mount = Mount::new();
        mount.mount("/", search);
        mount.mount("/static/", Static::new(Path::new("static/")));
        Iron::new(mount).http("127.0.0.1:3000").unwrap();
}
