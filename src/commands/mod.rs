mod bench;
mod index;
mod merge;
mod new;
mod search;
mod serve;

pub use self::bench::run_bench_cli;
pub use self::index::run_index_cli;
pub use self::merge::run_merge_cli;
pub use self::new::run_new_cli;
pub use self::search::run_search_cli;
pub use self::serve::run_serve_cli;
