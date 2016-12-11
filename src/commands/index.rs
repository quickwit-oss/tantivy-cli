use std::convert::From;
use std::fs::File;
use std::io;
use std::cmp;
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
    let mut num_threads = try!(value_t!(argmatch, "num_threads", usize).map_err(|_|format!("Failed to read num_threads argument as an integer.")));
    if num_threads == 0 {
        num_threads = 1;
    }
    let buffer_size = try!(value_t!(argmatch, "memory_size", usize).map_err(|_|format!("Failed to read the buffer size argument as an integer.")));
    let buffer_size_per_thread = buffer_size / num_threads;
    run_index(index_directory, document_source, buffer_size_per_thread, num_threads).map_err(|e| format!("Indexing failed : {:?}", e))
}

fn run_index(directory: PathBuf, document_source: DocumentSource, buffer_size_per_thread: usize, num_threads: usize) -> tantivy::Result<()> {
    
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
    

    let num_threads_to_parse_json = cmp::max(1, num_threads / 2);
    info!("Using {} threads to parse json", num_threads_to_parse_json);
    for _ in 0..num_threads_to_parse_json {
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
            index.writer_with_num_threads(num_threads, buffer_size_per_thread)
        }
        else {
            index.writer(buffer_size_per_thread)
        }
    );


    let index_result = index_documents(&mut index_writer, doc_receiver);
    try!(match index_result {
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
    });
    
    index_writer.wait_merging_threads()
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


enum DocumentSource {
    FromPipe,
    FromFile(PathBuf),
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
