use clap::ArgMatches;
use iron::mime::Mime;
use iron::prelude::*;
use iron::status;
use iron::typemap::Key;
use mount::Mount;
use persistent::Read;
use rustc_serialize::json::as_pretty_json;
use std::convert::From;
use std::path::Path;
use std::path::PathBuf;
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
use tantivy::schema::Schema;
use tantivy::schema::NamedFieldDocument;
use urlencoded::UrlEncodedQuery;
use std::str::FromStr;
use std::fmt::{self, Debug};
use std::error::Error;

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
    timings: Vec<Timing>,
}

#[derive(RustcEncodable)]
struct Hit {
    doc: NamedFieldDocument,
    explain: Option<Explanation>,
}

#[derive(RustcEncodable)]
struct Timing {
    name: String,
    duration: i64,
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
        let body_field = schema.get_field("body").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let query_parser = QueryParser::new(schema.clone(), vec!(body_field, title_field));
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

        {
            let mut chained_collector = collector::chain()
                    .add(&mut top_collector)
                    .add(&mut count_collector);
            try!(query.search(&searcher, &mut chained_collector));
        }
        let hits: Vec<Hit> = top_collector.docs()
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
                .collect();
        Ok(Serp {
            q: q,
            hits: hits,
            num_hits: count_collector.count(),
            timings: Vec::new(),
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

