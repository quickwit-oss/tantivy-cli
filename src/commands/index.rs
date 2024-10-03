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
use tantivy::TantivyDocument;
use time::Instant;

use crate::commands::merge::run_merge;

pub fn run_index_cli(argmatch: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(argmatch.get_one::<String>("index").unwrap());
    let document_source = argmatch
        .get_one::<String>("file")
        .map(|path| DocumentSource::FromFile(PathBuf::from(path)))
        .unwrap_or(DocumentSource::FromPipe);
    let no_merge = argmatch.contains_id("nomerge");
    let force_merge = argmatch.contains_id("forcemerge");
    let mut num_threads: usize = *ArgMatches::get_one(argmatch, "num_threads")
        .expect("Failed to read num_threads argument as an integer.");
    if num_threads == 0 {
        num_threads = 1;
    }
    let buffer_size: usize = *ArgMatches::get_one(argmatch, "memory_size")
        .expect("Failed to read the buffer size argument as an integer.");
    let buffer_size_per_thread = buffer_size / num_threads;
    run_index(
        index_directory,
        document_source,
        buffer_size_per_thread,
        num_threads,
        no_merge,
        force_merge,
    )
    .map_err(|e| format!("Indexing failed : {:?}", e))
}

//noinspection RsExternalLinter
fn run_index(
    directory: PathBuf,
    document_source: DocumentSource,
    buffer_size_per_thread: usize,
    num_threads: usize,
    no_merge: bool,
    force_merge: bool,
) -> tantivy::Result<()> {
    let index = Index::open_in_dir(&directory)?;
    let schema = index.schema();
    let (line_sender, line_receiver) = crossbeam_channel::bounded(100);
    let (doc_sender, doc_receiver) = crossbeam_channel::bounded(100);

    thread::spawn(move || {
        let articles = document_source.read().unwrap();
        for article_line_res in articles.lines() {
            let article_line = article_line_res.unwrap();
            line_sender.send(article_line).unwrap();
        }
    });

    let num_threads_to_parse_json = cmp::max(1, num_threads / 4);
    log::info!("Using {} threads to parse json", num_threads_to_parse_json);
    for _ in 0..num_threads_to_parse_json {
        let schema_clone = schema.clone();
        let doc_sender_clone = doc_sender.clone();
        let line_receiver_clone = line_receiver.clone();
        thread::spawn(move || {
            for doc_str in line_receiver_clone {
                match TantivyDocument::parse_json(&schema_clone, &doc_str) {
                    Ok(doc) => {
                        doc_sender_clone.send((doc, doc_str.len())).unwrap();
                    }
                    Err(err) => {
                        println!("Failed to add document doc {:?}", err);
                    }
                }
            }
        });
    }
    drop(doc_sender);

    let mut index_writer = if num_threads > 0 {
        index.writer_with_num_threads(num_threads, buffer_size_per_thread)
    } else {
        index.writer(buffer_size_per_thread)
    }?;

    if no_merge {
        index_writer.set_merge_policy(Box::new(NoMergePolicy));
    }

    let start_overall = Instant::now();
    let index_result = index_documents(&mut index_writer, doc_receiver);
    {
        let duration = start_overall - Instant::now();
        log::info!("Indexing the documents took {} s", duration.whole_seconds());
    }

    match index_result {
        Ok(res) => {
            println!("Commit succeed, docstamp at {}", res.docstamp);
            println!("Waiting for merging threads");

            let elapsed_before_merge = Instant::now() - start_overall;

            let doc_mb = res.num_docs_byte as f32 / 1_000_000_f32;
            let through_put = doc_mb / elapsed_before_merge.as_seconds_f32();
            println!("Total Nowait Merge: {:.2} Mb/s", through_put);

            index_writer.wait_merging_threads()?;

            if force_merge {
                println!("force_merge");
                run_merge(directory)?;
            }

            let elapsed_after_merge = Instant::now() - start_overall;

            let doc_mb = res.num_docs_byte as f32 / 1_000_000_f32;
            let through_put = doc_mb / elapsed_after_merge.as_seconds_f32();
            println!("Total Wait Merge: {:.2} Mb/s", through_put);

            println!("Terminated successfully!");
            {
                let duration = start_overall - Instant::now();
                log::info!(
                    "Indexing the documents took {} s overall (indexing + merge)",
                    duration.whole_seconds()
                );
            }
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

struct IndexResult {
    docstamp: u64,
    num_docs_byte: usize,
}

fn index_documents<D: Document>(
    index_writer: &mut IndexWriter<D>,
    doc_receiver: crossbeam_channel::Receiver<(D, usize)>,
) -> tantivy::Result<IndexResult> {
    let mut num_docs_total = 0;
    let mut num_docs = 0;
    let mut num_docs_byte = 0;
    let mut num_docs_byte_total = 0;

    let mut last_print = Instant::now();
    for (doc, doc_size) in doc_receiver {
        index_writer.add_document(doc)?;

        num_docs_total += 1;
        num_docs += 1;
        num_docs_byte += doc_size;
        num_docs_byte_total += doc_size;
        if num_docs % 128 == 0 {
            let new = Instant::now();
            let elapsed_since_last_print = new - last_print;
            if elapsed_since_last_print.as_seconds_f32() > 1.0 {
                println!("{} Docs", num_docs_total);
                let doc_mb = num_docs_byte as f32 / 1_000_000_f32;
                let through_put = doc_mb / elapsed_since_last_print.as_seconds_f32();
                println!(
                    "{:.0} docs / hour {:.2} Mb/s",
                    num_docs as f32 * 3600.0 * 1_000_000.0_f32
                        / (elapsed_since_last_print.whole_microseconds() as f32),
                    through_put
                );
                last_print = new;
                num_docs_byte = 0;
                num_docs = 0;
            }
        }
    }
    let res = index_writer.commit()?;

    Ok(IndexResult {
        docstamp: res,
        num_docs_byte: num_docs_byte_total,
    })
}

enum DocumentSource {
    FromPipe,
    FromFile(PathBuf),
}

impl DocumentSource {
    fn read(&self) -> io::Result<BufReader<Box<dyn Read>>> {
        Ok(match self {
            &DocumentSource::FromPipe => BufReader::new(Box::new(io::stdin())),
            DocumentSource::FromFile(filepath) => {
                let read_file = File::open(filepath)?;
                BufReader::new(Box::new(read_file))
            }
        })
    }
}
