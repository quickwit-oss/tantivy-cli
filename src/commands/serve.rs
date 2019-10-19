/// This tantivy command starts a http server (by default on port 3000)
///
/// Currently the only entrypoint is /api/
/// and it takes the following query string argument
///
/// - `q=` :    your query
///  - `nhits`:  the number of hits that should be returned. (default to 10)   
///
///
/// For instance, the following call should return the 20 most relevant
/// hits for fulmicoton.
///
///     http://localhost:3000/api/?q=fulmicoton&&nhits=20
///
use clap::ArgMatches;
use iron::mime::Mime;
use iron::prelude::*;
use iron::status;
use iron::typemap::Key;
use mount::Mount;
use persistent::Read;
use serde_json;
use std::convert::From;
use std::error::Error;
use std::fmt::{self, Debug};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use tantivy;
use tantivy::collector::{Count, TopDocs};
use tantivy::query::QueryParser;
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::schema::NamedFieldDocument;
use tantivy::schema::Schema;
use tantivy::tokenizer::*;
use tantivy::{DocAddress, Score};
use tantivy::Document;
use tantivy::Index;
use tantivy::IndexReader;
use crate::timer::TimerTree;
use urlencoded::UrlEncodedQuery;

pub fn run_serve_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.value_of("index").unwrap());
    let port = value_t!(matches, "port", u16).unwrap_or(3000u16);
    let host_str = matches.value_of("host").unwrap_or("localhost");
    let host = format!("{}:{}", host_str, port);
    run_serve(index_directory, &host).map_err(|e| format!("{:?}", e))
}

#[derive(Serialize)]
struct Serp {
    q: String,
    num_hits: usize,
    hits: Vec<Hit>,
    timings: TimerTree,
}

#[derive(Serialize)]
struct Hit {
    score: Score,
    doc: NamedFieldDocument,
    id: u32,
}

struct IndexServer {
    reader: IndexReader,
    query_parser: QueryParser,
    schema: Schema,
}

impl IndexServer {
    fn load(path: &Path) -> IndexServer {
        let index = Index::open_in_dir(path).unwrap();
        index.tokenizers().register(
            "commoncrawl",
            SimpleTokenizer
                .filter(RemoveLongFilter::limit(40))
                .filter(LowerCaser)
                .filter(AlphaNumOnlyFilter)
                .filter(Stemmer::new(Language::English)),
        );
        let schema = index.schema();
        let default_fields: Vec<Field> = schema
            .fields()
            .iter()
            .enumerate()
            .filter(|&(_, ref field_entry)| match *field_entry.field_type() {
                FieldType::Str(ref text_field_options) => {
                    text_field_options.get_indexing_options().is_some()
                }
                _ => false,
            })
            .map(|(i, _)| Field(i as u32))
            .collect();
        let query_parser =
            QueryParser::new(schema.clone(), default_fields, index.tokenizers().clone());
        let reader = index.reader().unwrap();
        IndexServer {
            reader,
            query_parser,
            schema,
        }
    }

    fn create_hit(&self, score: Score, doc: &Document, doc_address: &DocAddress) -> Hit {
        Hit {
            score,
            doc: self.schema.to_named_doc(&doc),
            id: doc_address.doc(),
        }
    }

    fn search(&self, q: String, num_hits: usize) -> tantivy::Result<Serp> {
        let query = self
            .query_parser
            .parse_query(&q)
            .expect("Parsing the query failed");
        let searcher = self.reader.searcher();
        let mut timer_tree = TimerTree::default();
        let (top_docs, num_hits) = {
            let _search_timer = timer_tree.open("search");
            searcher.search(&query, &(TopDocs::with_limit(num_hits), Count))?
        };
        let hits: Vec<Hit> = {
            let _fetching_timer = timer_tree.open("fetching docs");
            top_docs
                .iter()
                .map(|(score, doc_address)| {
                    let doc: Document = searcher.doc(*doc_address).unwrap();
                    self.create_hit(*score, &doc, doc_address)
                })
                .collect()
        };
        Ok(Serp {
            q,
            num_hits,
            hits,
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
    fn description(&self) -> &str {
        &*self.0
    }
}

fn search(req: &mut Request) -> IronResult<Response> {
    let index_server = req.get::<Read<IndexServer>>().unwrap();
    req.get_ref::<UrlEncodedQuery>()
        .map_err(|_| {
            IronError::new(
                StringError(String::from("Failed to decode error")),
                status::BadRequest,
            )
        })
        .and_then(|ref qs_map| {
            let num_hits: usize = qs_map
                .get("nhits")
                .and_then(|nhits_str| usize::from_str(&nhits_str[0]).ok())
                .unwrap_or(10);
            let query = qs_map.get("q").ok_or_else(|| {
                IronError::new(
                    StringError(String::from("Parameter q is missing from the query")),
                    status::BadRequest,
                )
            })?[0]
                .clone();
            let serp = index_server.search(query, num_hits).unwrap();
            let resp_json = serde_json::to_string_pretty(&serp).unwrap();
            let content_type = "application/json".parse::<Mime>().unwrap();
            Ok(Response::with((
                content_type,
                status::Ok,
                format!("{}", resp_json),
            )))
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
