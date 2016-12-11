#[macro_use]
extern crate clap;
#[macro_use]
extern crate rustc_serialize;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate tantivy;
extern crate time;
extern crate persistent;
extern crate urlencoded;
extern crate iron;
extern crate chan;
extern crate staticfile;
extern crate ansi_term;
extern crate mount;
extern crate bincode;
extern crate byteorder;

use clap::{AppSettings, Arg, App, SubCommand};
mod commands;
use self::commands::*;


fn main() {
    
    env_logger::init().unwrap();
    
    let index_arg = Arg::with_name("index")
                    .short("i")
                    .long("index")
                    .value_name("directory")
                    .help("Tantivy index directory filepath")
                    .required(true);

    let cli_options = App::new("Tantivy")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version("0.1")
        .author("Paul Masurel <paul.masurel@gmail.com>")
        .about("Tantivy Search Engine's command line interface.")
        .subcommand(
            SubCommand::with_name("new")
                .about("Create a new index. The schema will be populated with a simple example schema")
                .arg(index_arg.clone())
        )
        .subcommand(
            SubCommand::with_name("serve")
                .about("Start a server")
                .arg(index_arg.clone())
                .arg(Arg::with_name("host")
                    .long("host")
                    .value_name("host")
                    .help("host to listen to")
                )
                .arg(Arg::with_name("port")
                    .short("p")
                    .long("port")
                    .value_name("port")
                    .help("Port")
                    .default_value("localhost")
                )
        )
        .subcommand(
            SubCommand::with_name("index")
                .about("Index files")
                .arg(index_arg.clone())
                .arg(Arg::with_name("file")
                    .short("f")
                    .long("file")
                    .value_name("file")
                    .help("File containing the documents to index."))
                .arg(Arg::with_name("num_threads")
                    .short("t")
                    .long("num_threads")
                    .value_name("num_threads")
                    .help("Number of indexing threads. By default num cores - 1 will be used")
                    .default_value("3"))
                .arg(Arg::with_name("memory_size")
                    .short("m")
                    .long("memory_size")
                    .value_name("memory_size")
                    .help("Total memory_size in bytes. It will be split for the different threads.")
                    .default_value("1000000000"))
        )
        .subcommand(
            SubCommand::with_name("search")
                .about("Search an index.")
                .arg(index_arg.clone())
                .arg(Arg::with_name("query")
                    .short("q")
                    .long("query")
                    .value_name("query")
                    .help("Query")
                    .required(true))
        )
        .subcommand(
            SubCommand::with_name("bench")
                .about("Run a benchmark on your index")
                .arg(index_arg.clone())
                .arg(Arg::with_name("queries")
                    .short("q")
                    .long("queries")
                    .value_name("queries")
                    .help("File containing queries (one per line) to run in the benchmark.")
                    .required(true))
                .arg(Arg::with_name("num_repeat")
                    .short("n")
                    .long("num_repeat")
                    .value_name("num_repeat")
                    .help("Number of times to repeat the benchmark.")
                    .default_value("1"))
        )
        .subcommand(
            SubCommand::with_name("merge")
                .about("Merge all the segments of an index")
                .arg(index_arg.clone())
        )
        .get_matches();

    let (subcommand, some_options) = cli_options.subcommand();
    let options = some_options.unwrap();
    let run_cli = match subcommand {
        "new" => run_new_cli,
        "index" => run_index_cli,
        "serve" => run_serve_cli,
        "search" => run_search_cli,
        "merge" => run_merge_cli,
        "bench" => run_bench_cli,
        _ => panic!("Subcommand {} is unknown", subcommand)
    };
    run_cli(options).unwrap();
}
