use std::io::Write;

use clap::{App, AppSettings, Arg};
mod commands;
pub mod timer;
use self::commands::*;

fn main() {
    env_logger::init();

    let index_arg = Arg::with_name("index")
        .short('i')
        .long("index")
        .value_name("directory")
        .about("Tantivy index directory filepath")
        .required(true);

    let cli_options = App::new("Tantivy")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version(env!("CARGO_PKG_VERSION"))
        .author("Paul Masurel <paul.masurel@gmail.com>")
        .about("Tantivy Search Engine's command line interface.")
        .subcommand(
            App::new("new")
                .about("Create a new index. The schema will be populated with a simple example schema")
                .arg(index_arg.clone())
        )
        .subcommand(
            App::new("serve")
                .about("Start a server")
                .arg(index_arg.clone())
                .arg(Arg::with_name("host")
                    .long("host")
                    .value_name("host")
                    .about("host to listen to")
                )
                .arg(Arg::with_name("port")
                    .short('p')
                    .long("port")
                    .value_name("port")
                    .about("Port")
                    .default_value("localhost")
                )
        )
        .subcommand(
            App::new("index")
                .about("Index files")
                .arg(index_arg.clone())
                .arg(Arg::with_name("file")
                    .short('f')
                    .long("file")
                    .value_name("file")
                    .about("File containing the documents to index."))
                .arg(Arg::with_name("num_threads")
                    .short('t')
                    .long("num_threads")
                    .value_name("num_threads")
                    .about("Number of indexing threads. By default num cores - 1 will be used")
                    .default_value("3"))
                .arg(Arg::with_name("memory_size")
                    .short('m')
                    .long("memory_size")
                    .value_name("memory_size")
                    .about("Total memory_size in bytes. It will be split for the different threads.")
                    .default_value("1000000000"))
                .arg(Arg::with_name("nomerge")
                    .long("nomerge")
                    .about("Do not merge segments"))
        )
        .subcommand(
            App::new("search")
                .about("Search an index.")
                .arg(index_arg.clone())
                .arg(Arg::with_name("query")
                    .short('q')
                    .long("query")
                    .value_name("query")
                    .about("Query")
                    .required(true))
        )
        .subcommand(
            App::new("bench")
                .about("Run a benchmark on your index")
                .arg(index_arg.clone())
                .arg(Arg::with_name("queries")
                    .short('q')
                    .long("queries")
                    .value_name("queries")
                    .about("File containing queries (one per line) to run in the benchmark.")
                    .required(true))
                .arg(Arg::with_name("num_repeat")
                    .short('n')
                    .long("num_repeat")
                    .value_name("num_repeat")
                    .about("Number of times to repeat the benchmark.")
                    .default_value("1"))
        )
        .subcommand(
            App::new("merge")
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
        _ => panic!("Subcommand {} is unknown", subcommand),
    };

    if let Err(ref e) = run_cli(options) {
        let stderr = &mut std::io::stderr();
        let errmsg = "Error writing ot stderr";
        writeln!(stderr, "{}", e).expect(errmsg);
        std::process::exit(1);
    }
}
