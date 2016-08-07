use rustc_serialize::json;
use rustc_serialize::json::DecodeResult;
use std::convert::From;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use tantivy; 
use tantivy::Index;
use tantivy::schema::*;
use time::PreciseTime;
use clap::ArgMatches;

use serialize::json;

fn doc_from_json(schema: Schema, doc_json: &str) -> Document {
    let json_it = json::from_str(doc_json).unwrap();
    let json_obj = json_it.as_object().unwrap();
    
    println!()    
}

enum DocumentSource {
    FromPipe,
    FromFile(PathBuf),
}

pub fn run_index_cli(argmatch: &ArgMatches) -> tantivy::Result<()> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    let document_source = {
        match argmatch.value_of("file") {
            Some(path) => {
                DocumentSource::FromFile(PathBuf::from(path))
            }
            None => DocumentSource::FromPipe,
        }
    };
    run_index(index_directory, document_source)    
}

fn run_index(directory: PathBuf, document_source: DocumentSource) -> tantivy::Result<()> {
    
    let index = try!(Index::open(&directory));
    let schema = index.schema();
    let mut index_writer = index.writer_with_num_threads(8).unwrap();
    
    let articles = try!(document_source.read());
    
    let mut num_docs = 0;
    let mut cur = PreciseTime::now();
    let group_count = 10000;
    
    let title = schema.get_field("title").unwrap();
    let url = schema.get_field("url").unwrap();
    let body = schema.get_field("body").unwrap();
    
    for article_line_res in articles.lines() {
        let article_line = article_line_res.unwrap();
        let article_res: DecodeResult<WikiArticle> = json::decode(&article_line);
        match article_res {
            Ok(article) => {
                let mut doc = Document::new();
                doc.add_text(title, &article.title);
                doc.add_text(body, &article.body);
                doc.add_text(url, &article.url);
                index_writer.add_document(doc).unwrap();
            }
            Err(_) => {}
        }

        if num_docs > 0 && (num_docs % group_count == 0) {
            println!("{} Docs", num_docs);
            let new = PreciseTime::now();
            let elapsed = cur.to(new);
            println!("{:?} docs / hour", group_count * 3600 * 1e6 as u64 / (elapsed.num_microseconds().unwrap() as u64));
            cur = new;
        }

        num_docs += 1;

    }
    index_writer.wait()
}


#[derive(Clone,Debug,RustcDecodable,RustcEncodable)]
pub struct WikiArticle {
    pub url: String,
    pub title: String,
    pub body: String,
}


impl DocumentSource {
    fn read(&self,) -> io::Result<BufReader<Box<Read>>> {
        Ok(match self {
            &DocumentSource::FromPipe => {
                BufReader::new(Box::new(io::stdin()))
            } 
            &DocumentSource::FromFile(ref filepath) => {
                let read_file = try!(File::open(&filepath));
                BufReader::new(Box::new(read_file))
            }
        })
    }
}

