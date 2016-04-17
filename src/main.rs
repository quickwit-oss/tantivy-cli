extern crate tantivy;
extern crate time;
extern crate urlencoded;
#[macro_use]
extern crate lazy_static;
extern crate rustc_serialize;
extern crate iron;
extern crate staticfile;
extern crate mount;

use tantivy::collector::{CountCollector, FirstNCollector, MultiCollector};
use tantivy::schema::{TextField, Term};
use tantivy::Index;
use std::convert::From;
use time::PreciseTime;
use urlencoded::UrlEncodedQuery;
use tantivy::analyzer::SimpleTokenizer;
use iron::status;
use tantivy::analyzer::StreamingIterator;
use rustc_serialize::json::as_pretty_json;
use std::path::Path;
use staticfile::Static;
use iron::mime::Mime;
use mount::Mount;
use iron::prelude::*;

#[derive(RustcDecodable, RustcEncodable)]
struct Serp {
    query: String,
    num_hits: usize,
    hits: Vec<Hit>,
    timings: Vec<Timing>,
}

#[derive(RustcDecodable, RustcEncodable)]
struct Hit {
    title: String,
    body: String,
}

lazy_static! {
    static ref INDEX: Index = {
        Index::open(&Path::new("/Users/pmasurel/wiki-index/")).unwrap()
    };
}

fn parse_query(q: &String, field: &TextField) -> Vec<Term> {
    let tokenizer = SimpleTokenizer::new();
    let mut token_it = tokenizer.tokenize(&q);
    let mut terms = Vec::new();
    loop {
        match token_it.next() {
            Some(token) => {
                terms.push(Term::from_field_text(field, &token));
            }
            None => { break; }
        }
    }
    terms
}


struct TimingStarted {
    name: String,
    start: PreciseTime,
}

impl TimingStarted {
    fn new(name: &str) -> TimingStarted {
        TimingStarted {
            name: String::from(name),
            start: PreciseTime::now(),
        }
    }

    fn stop(self) -> Timing {
        let stop = PreciseTime::now();
        Timing {
            name: self.name,
            duration: self.start.to(stop).num_microseconds().unwrap(),
        }
    }
}

#[derive(RustcDecodable, RustcEncodable)]
struct Timing {
    name: String,
    duration: i64,
}


fn search(req: &mut Request) -> IronResult<Response> {
    let mut timings = Vec::new();
    match req.get_ref::<UrlEncodedQuery>() {
        Ok(ref qs_map) => {
            match qs_map.get("q") {
                Some(qs) => {
                    let query = qs[0].clone();
                    let search_timing = TimingStarted::new("search");
                    let searcher = INDEX.searcher().unwrap();
                    let schema = INDEX.schema();
                    let title_field = schema.text_field("title");
                    let body_field = schema.text_field("body");
                    let terms = parse_query(&query, &body_field);
                    let mut count_collector = CountCollector::new();
                    let mut first_collector = FirstNCollector::with_limit(10);
                    {
                        let mut multi_collector = MultiCollector::from(vec!(&mut count_collector, &mut first_collector));
                        let timings = searcher.search(&terms, &mut multi_collector).unwrap();
                        println!("{:?}", timings);
                    }
                    timings.push(search_timing.stop());
                    let storage_timing = TimingStarted::new("store");
                    let hits: Vec<Hit> = first_collector
                        .docs()
                        .iter()
                        .map(|doc_address| searcher.doc(doc_address).unwrap())
                        .map(|doc|
                            Hit {
                                title: doc.get_first_text(&title_field).unwrap().clone(),
                                body: doc.get_first_text(&body_field).unwrap().clone(),
                        })
                        .collect();
                    timings.push(storage_timing.stop());
                    let response = Serp {
                        query: query,
                        hits: hits,
                        num_hits: count_collector.count(),
                        timings: timings,
                    };
                    let resp_json = as_pretty_json(&response).indent(4);
                    let content_type = "application/json".parse::<Mime>().unwrap();
                    Ok(
                        Response::with((content_type, status::Ok, format!("{}", resp_json)))
                    )
                }
                None => {
                    Ok(Response::with((status::BadRequest, "Query not defined")))
                }
            }
        }
        Err(_) => Ok(Response::with((status::BadRequest, "Failed to parse query string")))
    }
}

fn main() {
        let mut mount = Mount::new();
        mount.mount("/api", search);
        mount.mount("/", Static::new(Path::new("static/")));
        println!("Running on 3000");
        Iron::new(mount).http("127.0.0.1:3000").unwrap();
}
