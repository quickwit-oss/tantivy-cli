/// NOT IMPLEMENTED YET
///
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
///     http://localhost:3000/api/?q=fulmicoton&nhits=20
///
use clap::ArgMatches;
use std::convert::From;
use std::path::PathBuf;

pub fn run_serve_cli(matches: &ArgMatches) -> Result<(), String> {
    let _index_directory = PathBuf::from(matches.get_one::<String>("index").unwrap());
    let port = ArgMatches::get_one(matches, "port").unwrap_or(&3000usize);
    let fallback = "localhost".to_string();
    let host_str = matches.get_one::<String>("host").unwrap_or(&fallback);
    let _host = format!("{host_str}:{port}");
    // TODO
    unimplemented!("Serve command is not implemented yet");
}
