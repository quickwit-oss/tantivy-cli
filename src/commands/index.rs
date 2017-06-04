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
use commands;
use chan;
use std::thread;

pub fn run_index_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let start = PreciseTime::now();

    let do_optimize: bool = argmatch.is_present("optimize");
    info!("Optimized enabled");

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
    run_index(index_directory.clone(), document_source, buffer_size_per_thread, num_threads).map_err(|e| format!("Indexing failed : {:?}", e))?;
    let end = PreciseTime::now();
    let elapsed = start.to(end);
    println!("Overall {:?} seconds", (elapsed.num_microseconds().unwrap() as u64) / 1_000_000 );


    if !do_optimize {
        return Ok(());
    }


    commands::run_merge(index_directory)
        .map(|e| format!("Merge failed with error {:?}", e));
    Ok(())


}

fn run_index(directory: PathBuf, document_source: DocumentSource, buffer_size_per_thread: usize, num_threads: usize) -> tantivy::Result<()> {
    info!("Buffer per thread : {} MB", buffer_size_per_thread / 1_000_000);
    
    let index = try!(Index::open(&directory));
    let schema = index.schema();

    let mut index_writer = try!(
        if num_threads > 0 {
            index.writer_with_num_threads(num_threads, buffer_size_per_thread)
        }
        else {
            index.writer(buffer_size_per_thread)
        }
    );


    let group_count = 100_000;
    let mut num_docs = 0;
    let mut cur = PreciseTime::now();

    let articles = document_source.read().unwrap();
    for article_line_res in articles.lines() {
        let article_line = article_line_res.unwrap();
        match schema.parse_document(&article_line) {
            Ok(doc) => {
                index_writer.add_document(doc);
                if num_docs > 0 && (num_docs % group_count == 0) {
                    println!("{} Docs", num_docs);
                    let new = PreciseTime::now();
                    let elapsed = cur.to(new);
                    println!("{:?} docs / hour", group_count * 3600 * 1_000_000 as u64 / (elapsed.num_microseconds().unwrap() as u64));
                    cur = new;
                }
                num_docs += 1;
            }
            Err(err) => {
                println!("Failed to add document doc {:?}", err);
            }
        }
    }

    let index_result = index_writer.commit();    
    match index_result {
        Ok(docstamp) => {
            println!("Commit succeed, docstamp at {}", docstamp);
            println!("Waiting for merging threads");
            index_writer.wait_merging_threads()?;
            println!("Terminated successfully!");
            Ok(())
        }
        Err(e) => {
            println!("Error during indexing, rollbacking.");
            index_writer.rollback().unwrap();
            println!("Rollback succeeded");
            return Err(e)
        }
    }
}

fn index_documents(index_writer: &mut IndexWriter, doc_receiver: chan::Receiver<Document>) -> tantivy::Result<u64> {
    let group_count = 100_000;
    let mut num_docs = 0;
    let mut cur = PreciseTime::now();
    let mut doc_iter = doc_receiver.into_iter();
    loop {
        let doc_group: Vec<Document> = (&mut doc_iter).take(1_000).collect();
        let num_in_group = doc_group.len();
        if num_in_group > 0 {
            index_writer.add_documents(doc_group);
        }
        num_docs += num_in_group as u64;
        if num_docs > 0 && (num_docs % group_count == 0) {
            println!("{} Docs", num_docs);
            let new = PreciseTime::now();
            let elapsed = cur.to(new);
            println!("{:?} docs / hour", group_count * 3600 * 1_000_000 as u64 / (elapsed.num_microseconds().unwrap() as u64));
            cur = new;
        }
        if num_in_group < 100 {
            break;
        }
    }

    // for doc in doc_receiver {
    //     index_writer.add_document(doc);
        
    // }
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
