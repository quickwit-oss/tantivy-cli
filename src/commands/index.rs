use std::convert::From;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use tantivy;
use tantivy::Index;
use tantivy::IndexWriter;
use tantivy::Document;
use time::PreciseTime;
use clap::ArgMatches;
use chan;
use std::thread;

pub fn run_index_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    let document_source = {
        match argmatch.value_of("file") {
            Some(path) => {
                DocumentSource::FromFile(PathBuf::from(path))
            }
            None => DocumentSource::FromPipe,
        }
    };
    let num_threads = try!(value_t!(argmatch, "num_threads", usize).map_err(|_|format!("Failed to read num_threads argument as an integer.")));
    run_index(index_directory, document_source, num_threads).map_err(|e| format!("Indexing failed : {:?}", e))
}

enum DocumentSource {
    FromPipe,
    FromFile(PathBuf),
}

fn run_index(directory: PathBuf, document_source: DocumentSource, num_threads: usize) -> tantivy::Result<()> {

    let index = try!(Index::open(&directory));
    let schema = index.schema();
    let (line_sender, line_receiver) = chan::sync(10_000);
    let (doc_sender, doc_receiver) = chan::sync(10_000);

    thread::spawn(move || {
        let articles = document_source.read().unwrap();
        for article_line_res in articles.lines() {
            let article_line = article_line_res.unwrap();
            line_sender.send(article_line);
        }
    });

    // using 3 threads to parse the json documents
    for _ in 0..3 {

        let schema_clone = schema.clone();
        let doc_sender_clone = doc_sender.clone();
        let line_receiver_clone = line_receiver.clone();

        thread::spawn(move || {
            for article_line in line_receiver_clone {
                match schema_clone.parse_document(&article_line) {
                    Ok(doc) => {
                        doc_sender_clone.send(doc);
                    }
                    Err(err) => {
                        println!("Failed to add document doc {:?}", err);
                    }
                }
            }
        });
    }
    drop(doc_sender);


    let mut index_writer = try!(
        if num_threads > 0 {
            index.writer_with_num_threads(num_threads)
        }
        else {
            index.writer()
        }
    );


    let index_result = index_documents(&mut index_writer, doc_receiver);
    match index_result {
        Ok(docstamp) => {
            println!("Commit succeed, docstamp at {}", docstamp);
            Ok(())
        }
        Err(e) => {
            println!("Error during indexing, rollbacking.");
            index_writer.rollback().unwrap();
            println!("Rollback succeeded");
            Err(e)
        }
    }
}

fn index_documents(index_writer: &mut IndexWriter, doc_receiver: chan::Receiver<Document>) -> tantivy::Result<u64> {
    let group_count = 100_000;
    let mut num_docs = 0;
    let mut cur = PreciseTime::now();
    for doc in doc_receiver {
        try!(index_writer.add_document(doc));
        if num_docs > 0 && (num_docs % group_count == 0) {
            println!("{} Docs", num_docs);
            let new = PreciseTime::now();
            let elapsed = cur.to(new);
            println!("{:?} docs / hour", group_count * 3600 * 1_000_000 as u64 / (elapsed.num_microseconds().unwrap() as u64));
            cur = new;
        }
        num_docs += 1;
    }
    index_writer.commit()
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
