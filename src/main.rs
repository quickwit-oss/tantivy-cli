extern crate tantivy;
extern crate time;
extern crate urlencoded;
#[macro_use]
extern crate lazy_static;
extern crate rustc_serialize;
extern crate iron;
extern crate staticfile;
extern crate mount;

use tantivy::schema::Field;
use tantivy::collector::CountCollector;
use tantivy::Index;
use std::convert::From;
use time::PreciseTime;
use tantivy::collector;
use urlencoded::UrlEncodedQuery;
use iron::status;
use rustc_serialize::json::as_pretty_json;
use std::path::Path;
use staticfile::Static;
use iron::mime::Mime;
use mount::Mount;
use tantivy::query::Query;
use tantivy::query::QueryParser;
use tantivy::Document;
use tantivy::collector::TopCollector;
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
    static ref INDEX_SERVER: IndexServer = {
        IndexServer::load(&Path::new("/data/wiki-index/"))
    };
}

struct IndexServer {
    index: Index,
    query_parser: QueryParser,
    body_field: Field,
    title_field: Field,
}

impl IndexServer {
    fn load(path: &Path) -> IndexServer {
        let index = Index::open(path).unwrap();
        let schema = index.schema();
        let body_field = schema.get_field("body").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let query_parser = QueryParser::new(schema, vec!(body_field, title_field));
        IndexServer {
            index: index,
            query_parser: query_parser,
            title_field: title_field,
            body_field: body_field,
        }
    }

    fn create_hit(&self, doc: &Document) -> Hit {
        Hit {
            title: String::from(doc.get_first(self.title_field).unwrap().text()),
            body: String::from(doc.get_first(self.body_field).unwrap().text().clone()),

        }
    }
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
                    let parsed_query = INDEX_SERVER.query_parser.parse_query(&query).unwrap();
                    let search_timing = TimingStarted::new("search");
                    let searcher = INDEX_SERVER.index.searcher().unwrap();

                    let mut count_collector = CountCollector::new();
                    let mut top_collector = TopCollector::with_limit(30);

                    {
                        let mut chained_collector = collector::chain()
                                .add(&mut top_collector)
                                .add(&mut count_collector);
                        let timings = parsed_query.search(&searcher, &mut chained_collector).unwrap();
                        println!("{:?}", timings);
                    }
                    timings.push(search_timing.stop());
                    let storage_timing = TimingStarted::new("store");
                    for scored_doc in top_collector.score_docs() {
                        println!("{:?}", scored_doc);
                    }
                    let hits: Vec<Hit> = top_collector
                        .docs()
                        .iter()
                        .map(|doc_address| searcher.doc(doc_address).unwrap())
                        .map(|doc|INDEX_SERVER.create_hit(&doc) )
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
