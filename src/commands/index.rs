use rustc_serialize::json;
use rustc_serialize::json::Json;
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


#[derive(Debug)]
enum DocMappingError {
    NotJSON(json::ParserError),
    NotJSONObject(String),
    MappingError(String, String),
    OverflowError(String),
    NoSuchFieldInSchema(String),
}

impl From<json::ParserError> for DocMappingError {
    fn from(err: json::ParserError) -> DocMappingError {
        DocMappingError::NotJSON(err)
    }
}

fn doc_from_json(schema: &Schema, doc_json: &str) -> Result<Document, DocMappingError> {
    let json_node = try!(Json::from_str(doc_json));
    let some_json_obj = json_node.as_object();
    if !some_json_obj.is_some() {
        let doc_json_sample: String;
        if doc_json.len() < 20 {
            doc_json_sample = String::from(doc_json);
        }
        else {
            doc_json_sample = format!("{:?}...", &doc_json[0..20]);
        }
        return Err(DocMappingError::NotJSONObject(doc_json_sample))
    }
    let json_obj = some_json_obj.unwrap();
    let mut doc = Document::new();
    for (field_name, field_value) in json_obj.iter() {
        match schema.get_field(field_name) {
            Some(field) => {
                let field_entry = schema.get_field_entry(field);
                match field_value {
                    &Json::String(ref field_text) => {
                        match field_entry {
                            &FieldEntry::Text(_, _) => {
                                doc.add_text(field, field_text);
                            }
                            _ => {
                                return Err(DocMappingError::MappingError(field_name.clone(), format!("Expected a string, got {:?}", field_value)));
                            }
                        }
                    }
                    &Json::U64(ref field_val_u64) => {
                        match field_entry {
                            &FieldEntry::U32(_, _) => {
                                if *field_val_u64 > (u32::max_value() as u64) {
                                    return Err(DocMappingError::OverflowError(field_name.clone()));
                                }
                                doc.add_u32(field, *field_val_u64 as u32);
                            }
                            _ => {
                                return Err(DocMappingError::MappingError(field_name.clone(), format!("Expected a string, got {:?}", field_value)));
                            }
                        }
                    },
                    _ => {
                        return Err(DocMappingError::MappingError(field_name.clone(), String::from("Value is neither u32, nor text.")));
                    }
                }
            }
            None => {
                return Err(DocMappingError::NoSuchFieldInSchema(field_name.clone()))
            }
        }
    }
    Ok(doc)    
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
    
    for article_line_res in articles.lines() {
        let article_line = article_line_res.unwrap(); // TODO
        match doc_from_json(&schema, &article_line) {
            Ok(doc) => {
                index_writer.add_document(doc).unwrap();
            }
            Err(err) => {
                println!("Failed to add document doc {:?}", err);
            }
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
    index_writer.wait().unwrap(); // TODO
    Ok(())
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

