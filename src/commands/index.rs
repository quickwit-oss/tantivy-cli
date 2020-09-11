use chan;
use clap::value_t;
use clap::ArgMatches;
use std::cmp;
use std::convert::From;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use tantivy::merge_policy::NoMergePolicy;
use tantivy::Document;
use tantivy::Index;
use tantivy::IndexWriter;
use tantivy::{self, directory::MmapDirectory, slog::error, slog::info, slog::o, slog::Logger};
use time::PreciseTime;

pub fn run_index_cli(argmatch: &ArgMatches, logger: &Logger) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.value_of("index").unwrap());
    let document_source = argmatch
        .value_of("file")
        .map(|path| DocumentSource::FromFile(PathBuf::from(path)))
        .unwrap_or(DocumentSource::FromPipe);
    let no_merge = argmatch.is_present("nomerge");
    let mut num_threads = value_t!(argmatch, "num_threads", usize)
        .map_err(|_| format!("Failed to read num_threads argument as an integer."))?;
    if num_threads == 0 {
        num_threads = 1;
    }
    let overall_heap_size_in_bytes= value_t!(argmatch, "memory_size", usize)
        .map_err(|_| format!("Failed to read the buffer size argument as an integer."))?;
    run_index(
        index_directory,
        document_source,
        overall_heap_size_in_bytes,
        num_threads,
        no_merge,
        logger,
    )
    .map_err(|e| format!("Indexing failed : {:?}", e))
}

fn run_index(
    directory: PathBuf,
    document_source: DocumentSource,
    overall_heap_size_in_bytes : usize,
    num_threads: usize,
    no_merge: bool,
    logger: &Logger,
) -> tantivy::Result<()> {
    let num_threads_to_parse_json = cmp::max(1, num_threads / 4);

    let index_logger = logger.new(o!("cmd"=>"index"));
    info!(&index_logger, "start-indexing-command"; 
         "num-threads-json" => num_threads_to_parse_json,
         "num-threads"=>num_threads,
         "no-merge"=>no_merge,
         "overall-heap-size-in-bytes"=>overall_heap_size_in_bytes);
    let mmap_directory = MmapDirectory::open_with_logger(&directory, index_logger.clone())?;
    let index = Index::open(mmap_directory)?;

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

    for thread_id in 0..num_threads_to_parse_json {
        let schema_clone = schema.clone();
        let doc_sender_clone = doc_sender.clone();
        let line_receiver_clone = line_receiver.clone();
        let logger = logger.new(o!("json-parse-thread-id"=>thread_id)); 
        thread::spawn(move || {
            for article_line in line_receiver_clone {
                match schema_clone.parse_document(&article_line) {
                    Ok(doc) => {
                        doc_sender_clone.send(doc);
                    }
                    Err(err) => {
                        tantivy::slog::error!(logger, "docparse-json-error"; "error"=>format!("{:?}", err));
                    }
                }
            }
        });
    }
    drop(doc_sender);

    let mut index_writer = if num_threads > 0 {
        index.writer_with_num_threads(num_threads, overall_heap_size_in_bytes)
    } else {
        index.writer(overall_heap_size_in_bytes)
    }?;

    let _ = futures::executor::block_on(index_writer.garbage_collect_files());

    if no_merge {
        index_writer.set_merge_policy(Box::new(NoMergePolicy));
    }

    let start_overall = PreciseTime::now();
    let index_result = index_documents(&mut index_writer, doc_receiver, logger);
    {
        let duration = start_overall.to(PreciseTime::now());
        info!(&index_logger, "index-finished"; "elapsed_s" => duration.num_seconds());
    }

    match index_result {
        Ok(opstamp) => {
            let commit_duration = start_overall.to(PreciseTime::now());
            info!(index_logger, "commit-success"; "opstamp" => opstamp, "commit-time-s"=>commit_duration.num_seconds());
            index_writer.wait_merging_threads()?;
                let overall_duration = start_overall.to(PreciseTime::now());
                info!(
                    index_logger,
                    "indexing-stop";
                    "overall-time-s" => overall_duration.num_seconds(),
                );
            Ok(())
        }
        Err(e) => {
            error!(logger, "commit-error");
            info!(logger, "rollback-start");
            index_writer.rollback()?;
            let _ = futures::executor::block_on(index_writer.garbage_collect_files());
            info!(logger, "rollback-success");
            Err(e)
        }
    }
}

fn index_documents(
    index_writer: &mut IndexWriter,
    doc_receiver: chan::Receiver<Document>,
    logger: &Logger
) -> tantivy::Result<u64> {
    let group_count = 100_000;
    let mut num_docs = 0;
    let mut cur = PreciseTime::now();
    for doc in doc_receiver {
        index_writer.add_document(doc);
        if num_docs > 0 && (num_docs % group_count == 0) {
            let new = PreciseTime::now();
            let elapsed = cur.to(new);
            let docs_per_hour = group_count * 3600 * 1_000_000 as u64
                    / (elapsed.num_microseconds().unwrap() as u64);
            info!(logger, "indexed"; "num_docs" => num_docs, "docs_per_hour" => docs_per_hour);
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
    fn read(&self) -> io::Result<BufReader<Box<dyn Read>>> {
        Ok(match self {
            &DocumentSource::FromPipe => BufReader::new(Box::new(io::stdin())),
            &DocumentSource::FromFile(ref filepath) => {
                let read_file = File::open(&filepath)?;
                BufReader::new(Box::new(read_file))
            }
        })
    }
}
