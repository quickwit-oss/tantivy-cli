/// This tantivy command starts a http server (by default on port 3000)
/// 
/// Currently the only entrypoint is /api/
/// and it takes the following query string argument
/// 
/// - `q=` :    your query
//  - `nhits`:  the number of hits that should be returned. (default to 10)  
/// - `explain=` : if true returns some information about the score.  
///
///
/// For instance, the following call should return the 20 most relevant
/// hits for fulmicoton.
///
///     http://localhost:3000/api/?q=fulmicoton&explain=false&nhits=20
///


use clap::ArgMatches;
use iron::mime::Mime;
use iron::prelude::*;
use iron::status;
use iron::typemap::Key;
use mount::Mount;
use persistent::Read;
use rustc_serialize::json::as_pretty_json;
use std::convert::From;
use std::error::Error;
use std::fmt::{self, Debug};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use tantivy;
use tantivy::collector;
use tantivy::collector::CountCollector;
use tantivy::collector::TopCollector;
use tantivy::Document;
use tantivy::Index;
use tantivy::query::Explanation;
use tantivy::query::Query;
use tantivy::query::QueryParser;
use tantivy::Result;
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::schema::NamedFieldDocument;
use tantivy::schema::Schema;
use tantivy::TimerTree;
use urlencoded::UrlEncodedQuery;

pub fn run_serve_cli(matches: &ArgMatches) -> tantivy::Result<()> {
    let index_directory = PathBuf::from(matches.value_of("index").unwrap());
    let port = value_t!(matches, "port", u16).unwrap_or(3000u16);
    let host_str = matches.value_of("host").unwrap_or("localhost");
    let host = format!("{}:{}", host_str, port);
    run_serve(index_directory, &host)   
}


#[derive(RustcEncodable)]
struct Serp {
    q: String,
    num_hits: usize,
    hits: Vec<Hit>,
    timings: TimerTree,
}

#[derive(RustcEncodable)]
struct Hit {
    doc: NamedFieldDocument,
    explain: Option<Explanation>,
}

struct IndexServer {
    index: Index,
    query_parser: QueryParser,
    schema: Schema,
}

impl IndexServer {
    
    fn load(path: &Path) -> IndexServer {
        let index = Index::open(path).unwrap();
        let schema = index.schema();
        let default_fields: Vec<Field> = schema
            .fields()
            .iter()
            .enumerate()
            .filter(
                |&(_, ref field_entry)| {
                    match *field_entry.field_type() {
                        FieldType::Str(_) => true,
                        FieldType::U32(_) => false
                    }
                }
            )
            .map(|(i, _)| Field(i as u8))
            .collect();
        let query_parser = QueryParser::new(schema.clone(), default_fields);
        IndexServer {
            index: index,
            query_parser: query_parser,
            schema: schema,
        }
    }

    fn create_hit(&self, doc: &Document, explain: Option<Explanation>) -> Hit {
        Hit {
            doc: self.schema.to_named_doc(&doc),
            explain: explain,
        }
    }
    
    fn search(&self, q: String, num_hits: usize, explain:  bool) -> Result<Serp> {
        let query = self.query_parser.parse_query(&q).unwrap();
        let searcher = self.index.searcher().unwrap();
        let mut count_collector = CountCollector::new();
        let mut top_collector = TopCollector::with_limit(num_hits);
        let mut timer_tree = TimerTree::new();
        {
            let _search_timer = timer_tree.open("search");
            let mut chained_collector = collector::chain()
                .add(&mut top_collector)
                .add(&mut count_collector);
            try!(query.search(&searcher, &mut chained_collector));
        }
        let hits: Vec<Hit> = {
            let _fetching_timer = timer_tree.open("fetching docs");
            top_collector.docs()
                .iter()
                .map(|doc_address| {
                    let doc: Document = searcher.doc(doc_address).unwrap();
                    let explanation;
                    if explain {
                        explanation = Some(query.explain(&searcher, doc_address).unwrap());
                    }
                    else {
                        explanation = None;
                    }
                    self.create_hit(&doc, explanation)
                })
                .collect()
        };
        Ok(Serp {
            q: q,
            num_hits: count_collector.count(),
            hits: hits,
            timings: timer_tree,
        })
    }
}

impl Key for IndexServer {
    type Value = IndexServer;
}

#[derive(Debug)]
struct StringError(String);

impl fmt::Display for StringError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for StringError {
    fn description(&self) -> &str { &*self.0 }
}

fn search(req: &mut Request) -> IronResult<Response> {
    let index_server = req.get::<Read<IndexServer>>().unwrap();
    req.get_ref::<UrlEncodedQuery>()
        .map_err(|_| IronError::new(StringError(String::from("Failed to decode error")), status::BadRequest))
        .and_then(|ref qs_map| {
            let num_hits: usize = qs_map
                .get("nhits")
                .and_then(|nhits_str| usize::from_str(&nhits_str[0]).ok())
                .unwrap_or(10);
            let explain: bool = qs_map
                .get("explain")
                .map(|s| &s[0] == &"true")
                .unwrap_or(false);
            let query = try!(qs_map
                .get("q")
                .ok_or_else(|| IronError::new(StringError(String::from("Parameter q is missing from the query")), status::BadRequest)))[0].clone();
            let serp = index_server.search(query, num_hits, explain).unwrap();
            let resp_json = as_pretty_json(&serp).indent(4);
            let content_type = "application/json".parse::<Mime>().unwrap();
            Ok(Response::with((content_type, status::Ok, format!("{}", resp_json))))
        })
        
}



fn run_serve(directory: PathBuf, host: &str) -> tantivy::Result<()> {
    let mut mount = Mount::new();
    let server = IndexServer::load(&directory);
    
    mount.mount("/api", search);
    
    let mut middleware = Chain::new(mount);
    middleware.link(Read::<IndexServer>::both(server));
    
    println!("listening on http://{}", host);
    Iron::new(middleware).http(host).unwrap();
    Ok(())
}

