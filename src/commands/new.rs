use ansi_term::Colour::{Blue, Green, Red};
use ansi_term::Style;
use clap::ArgMatches;
use std::cmp::Ordering;
use std::convert::From;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use tantivy::schema::*;
use tantivy::Index;

use super::infer_schema::{infer_schema_from_ndjson, InferredField, InferredType};

pub fn run_new_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.get_one::<String>("index").unwrap());
    if let Some(ndjson_file) = matches.get_one::<String>("infer_from_ndjson") {
        let sample_size = *matches.get_one::<usize>("sample_size").unwrap();
        run_new_with_inferred_schema(index_directory, PathBuf::from(ndjson_file), sample_size)
    } else {
        run_new_interactive(index_directory).map_err(|e| format!("{:?}", e))
    }
}

fn prompt_input<P: Fn(&str) -> Result<(), String>>(prompt_text: &str, predicate: P) -> String {
    loop {
        print!(
            "{prompt_text:<width$} ? ",
            prompt_text = Style::new().bold().fg(Blue).paint(prompt_text),
            width = 40
        );
        io::stdout().flush().unwrap();
        let mut buffer = String::new();
        io::stdin()
            .read_line(&mut buffer)
            .expect("Failed to read line");
        let answer = buffer.trim_end().to_string();
        match predicate(&answer) {
            Ok(()) => {
                return answer;
            }
            Err(msg) => {
                println!("Error: {}", Style::new().bold().fg(Red).paint(msg));
            }
        }
    }
}

// TODO move into core tantivy
fn field_name_validate(field_name: &str) -> Result<(), String> {
    if is_valid_field_name(field_name) {
        Ok(())
    } else {
        Err(String::from(
            "Field name must match the pattern [_a-zA-Z0-9]+",
        ))
    }
}

fn prompt_options(msg: &str, codes: Vec<char>) -> char {
    let options_string: Vec<String> = codes.iter().map(|c| format!("{}", c)).collect();
    let options = options_string.join("/");
    let predicate = |entry: &str| {
        if entry.len() != 1 {
            return Err(format!("Invalid input. Options are ({})", options));
        }
        let c = entry.chars().next().unwrap().to_ascii_uppercase();
        if codes.contains(&c) {
            Ok(())
        } else {
            Err(format!("Invalid input. Options are ({})", options))
        }
    };
    let message = format!("{} ({})", msg, options);
    let entry = prompt_input(&message, predicate);
    entry.chars().next().unwrap().to_ascii_uppercase()
}

fn prompt_field_type(msg: &str, codes: Vec<&str>) -> tantivy::schema::Type {
    let options = codes.join("/");
    let predicate = |entry: &str| {
        // TODO make case-insensitive, currently has to match the options precisely
        if codes.contains(&entry) {
            Ok(())
        } else {
            Err(format!("Invalid input. Options are ({})", options))
        }
    };
    let message = format!("{} ({})", msg, options);
    let prompt_output = prompt_input(&message, predicate);
    match prompt_output.to_ascii_uppercase().as_ref() {
        "TEXT" => Type::Str,
        "BOOL" => Type::Bool,
        "U64" => Type::U64,
        "I64" => Type::I64,
        "F64" => Type::F64,
        "DATE" => Type::Date,
        "FACET" => Type::Facet,
        "BYTES" => Type::Bytes,
        "JSON" => Type::Json,
        "IPADDR" => Type::IpAddr,
        &_ => Type::Str, // shouldn't be here, the `predicate` fails before here
    }
}

fn prompt_yn(msg: &str) -> bool {
    prompt_options(msg, vec!['Y', 'N']) == 'Y'
}

fn prompt_tokenizer_or_default() -> String {
    let tokenizer = prompt_input("Tokenizer (enter for default)", |_| Ok(()));
    if tokenizer.trim().is_empty() {
        "default".to_string()
    } else {
        tokenizer
    }
}

fn ask_add_field_text(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut text_options = TextOptions::default();
    if prompt_yn("Should the field be stored") {
        text_options = text_options.set_stored();
    }
    if prompt_yn("Should the field be fast") {
        text_options = text_options.set_fast(None);
    }

    if prompt_yn("Should the field be indexed") {
        let mut text_indexing_options = TextFieldIndexing::default()
            .set_index_option(IndexRecordOption::Basic)
            .set_tokenizer("default");
        let tokenizer = prompt_tokenizer_or_default();

        if prompt_yn("Should the term be tokenized?") {
            text_indexing_options = text_indexing_options.set_tokenizer(&tokenizer);
            if prompt_yn("Should the term frequencies (per doc) be in the index") {
                if prompt_yn("Should the term positions (per doc) be in the index") {
                    text_indexing_options = text_indexing_options
                        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
                } else {
                    text_indexing_options =
                        text_indexing_options.set_index_option(IndexRecordOption::WithFreqs);
                }
            }
        } else {
            text_indexing_options = text_indexing_options.set_tokenizer("raw");
        }

        text_options = text_options.set_indexing_options(text_indexing_options);
    }

    schema_builder.add_text_field(field_name, text_options);
}

fn ask_add_num_field_with_options(
    field_name: &str,
    field_type: Type,
    schema_builder: &mut SchemaBuilder,
) {
    let mut int_options = NumericOptions::default();
    if prompt_yn("Should the field be stored") {
        int_options = int_options.set_stored();
    }
    if prompt_yn("Should the field be fast") {
        int_options = int_options.set_fast();
    }
    if prompt_yn("Should the field be indexed") {
        int_options = int_options.set_indexed();
    }
    match field_type {
        Type::U64 => {
            schema_builder.add_u64_field(field_name, int_options);
        }
        Type::F64 => {
            schema_builder.add_f64_field(field_name, int_options);
        }
        Type::I64 => {
            schema_builder.add_i64_field(field_name, int_options);
        }
        Type::Bool => {
            schema_builder.add_bool_field(field_name, int_options);
        }
        _ => {
            // We only pass to this function if the field type is numeric
            unreachable!();
        }
    }
}

fn ask_add_field_json(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut json_options = JsonObjectOptions::default();
    if prompt_yn("Should the field be stored") {
        let stored: JsonObjectOptions = STORED.into();
        json_options = json_options | stored;
    }

    if prompt_yn("Should the field be indexed") {
        let with_positions = prompt_yn("Should the indexed json keep positions");
        let tokenizer = prompt_tokenizer_or_default();
        let index_option = if with_positions {
            IndexRecordOption::WithFreqsAndPositions
        } else {
            IndexRecordOption::Basic
        };
        let json_indexing = TextFieldIndexing::default()
            .set_tokenizer(&tokenizer)
            .set_index_option(index_option);
        json_options = json_options.set_indexing_options(json_indexing);
    }

    schema_builder.add_json_field(field_name, json_options);
}

fn ask_add_field_bytes(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut bytes_options = BytesOptions::default();
    if prompt_yn("Should the field be stored") {
        bytes_options = bytes_options.set_stored();
    }

    if prompt_yn("Should the field be indexed") {
        bytes_options = bytes_options.set_indexed();
    }

    schema_builder.add_bytes_field(field_name, bytes_options);
}

fn ask_add_field_date(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut date_options = DateOptions::default();
    if prompt_yn("Should the field be stored") {
        date_options = date_options.set_stored();
    }

    if prompt_yn("Should the field be fast") {
        date_options = date_options.set_fast();
    }

    if prompt_yn("Should the field be indexed") {
        date_options = date_options.set_indexed();
    }

    schema_builder.add_date_field(field_name, date_options);
}

fn ask_add_field_ip(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut ip_addr_options = IpAddrOptions::default();
    if prompt_yn("Should the field be stored") {
        ip_addr_options = ip_addr_options.set_stored();
    }

    if prompt_yn("Should the field be fast") {
        ip_addr_options = ip_addr_options.set_fast();
    }

    if prompt_yn("Should the field be indexed") {
        ip_addr_options = ip_addr_options.set_indexed();
    }

    schema_builder.add_ip_addr_field(field_name, ip_addr_options);
}

fn ask_add_field(schema_builder: &mut SchemaBuilder) {
    println!("\n\n");
    let field_name = prompt_input("New field name ", field_name_validate);

    // Manually iterate over tantivy::schema::Type and make strings out of them
    // Can introduce a dependency to do it automatically, but this should be easier
    let possible_field_types = vec![
        "Text", "u64", "i64", "f64", "Date", "Facet", "Bytes", "Json", "bool", "IpAddr",
    ];
    let field_type = prompt_field_type("Choose Field Type", possible_field_types);
    match field_type {
        Type::Str => {
            ask_add_field_text(&field_name, schema_builder);
        }
        Type::U64 | Type::F64 | Type::I64 | Type::Bool => {
            ask_add_num_field_with_options(&field_name, field_type, schema_builder);
        }
        Type::Date => {
            ask_add_field_date(&field_name, schema_builder);
        }
        Type::Facet => {
            schema_builder.add_facet_field(&field_name, tantivy::schema::INDEXED);
        }
        Type::Bytes => {
            ask_add_field_bytes(&field_name, schema_builder);
        }
        Type::Json => {
            ask_add_field_json(&field_name, schema_builder);
        }
        Type::IpAddr => {
            ask_add_field_ip(&field_name, schema_builder);
        }
    }
}

fn inferred_type_label(inferred_type: InferredType) -> &'static str {
    match inferred_type {
        InferredType::Text => "Text",
        InferredType::Bool => "bool",
        InferredType::U64 => "u64",
        InferredType::I64 => "i64",
        InferredType::F64 => "f64",
        InferredType::Date => "Date",
        InferredType::IpAddr => "IpAddr",
        InferredType::Json => "Json",
    }
}

fn compare_inferred_fields(left: &InferredField, right: &InferredField) -> Ordering {
    left.name.cmp(&right.name)
}

fn add_inferred_text_field(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut text_options = TextOptions::default().set_stored();
    if prompt_yn("Should the field be fast") {
        text_options = text_options.set_fast(None);
    }

    if prompt_yn("Should the field be indexed") {
        let with_positions = prompt_yn("Should the indexed text keep positions");
        let tokenizer = prompt_tokenizer_or_default();
        let index_option = if with_positions {
            IndexRecordOption::WithFreqsAndPositions
        } else {
            IndexRecordOption::Basic
        };
        let text_indexing_options = TextFieldIndexing::default()
            .set_tokenizer(&tokenizer)
            .set_index_option(index_option);
        text_options = text_options.set_indexing_options(text_indexing_options);
    }

    schema_builder.add_text_field(field_name, text_options);
}

fn add_inferred_numeric_field(
    field_name: &str,
    inferred_type: InferredType,
    schema_builder: &mut SchemaBuilder,
) {
    let mut options = NumericOptions::default().set_stored();
    if prompt_yn("Should the field be fast") {
        options = options.set_fast();
    }
    if prompt_yn("Should the field be indexed") {
        options = options.set_indexed();
    }

    match inferred_type {
        InferredType::U64 => {
            schema_builder.add_u64_field(field_name, options);
        }
        InferredType::I64 => {
            schema_builder.add_i64_field(field_name, options);
        }
        InferredType::F64 => {
            schema_builder.add_f64_field(field_name, options);
        }
        InferredType::Bool => {
            schema_builder.add_bool_field(field_name, options);
        }
        _ => unreachable!("invalid inferred numeric type"),
    }
}

fn add_inferred_date_field(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut options = DateOptions::default().set_stored();
    if prompt_yn("Should the field be fast") {
        options = options.set_fast();
    }
    if prompt_yn("Should the field be indexed") {
        options = options.set_indexed();
    }
    schema_builder.add_date_field(field_name, options);
}

fn add_inferred_ip_field(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut options = IpAddrOptions::default().set_stored();
    if prompt_yn("Should the field be fast") {
        options = options.set_fast();
    }
    if prompt_yn("Should the field be indexed") {
        options = options.set_indexed();
    }
    schema_builder.add_ip_addr_field(field_name, options);
}

fn add_inferred_json_field(field_name: &str, schema_builder: &mut SchemaBuilder) {
    let mut json_options: JsonObjectOptions = STORED.into();
    if prompt_yn("Should the field be fast") {
        json_options = json_options.set_fast(None);
    }
    if prompt_yn("Should the field be indexed") {
        let with_positions = prompt_yn("Should the indexed json keep positions");
        let tokenizer = prompt_tokenizer_or_default();
        let index_option = if with_positions {
            IndexRecordOption::WithFreqsAndPositions
        } else {
            IndexRecordOption::Basic
        };
        let json_indexing = TextFieldIndexing::default()
            .set_tokenizer(&tokenizer)
            .set_index_option(index_option);
        json_options = json_options.set_indexing_options(json_indexing);
    }
    schema_builder.add_json_field(field_name, json_options);
}

fn ask_add_inferred_field(field: &InferredField, schema_builder: &mut SchemaBuilder) {
    println!(
        "\n{}",
        Style::new().bold().fg(Green).paint(format!(
            "Configure {} ({})",
            field.name,
            inferred_type_label(field.field_type)
        ))
    );
    match field.field_type {
        InferredType::Text => add_inferred_text_field(&field.name, schema_builder),
        InferredType::Bool | InferredType::U64 | InferredType::I64 | InferredType::F64 => {
            add_inferred_numeric_field(&field.name, field.field_type, schema_builder)
        }
        InferredType::Date => add_inferred_date_field(&field.name, schema_builder),
        InferredType::IpAddr => add_inferred_ip_field(&field.name, schema_builder),
        InferredType::Json => add_inferred_json_field(&field.name, schema_builder),
    }
}

fn create_index_with_schema(directory: PathBuf, schema: Schema) -> tantivy::Result<()> {
    let schema_json = serde_json::to_string_pretty(&schema).unwrap().to_string();
    println!("\n{}\n", Style::new().fg(Green).paint(schema_json));
    match fs::create_dir(&directory) {
        Ok(_) => (),
        // Proceed here; actual existence of index is checked in Index::create_in_dir
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => panic!("{:?}", e),
    };
    Index::create_in_dir(&directory, schema)?;
    Ok(())
}

fn run_new_with_inferred_schema(
    directory: PathBuf,
    ndjson_path: PathBuf,
    sample_size: usize,
) -> Result<(), String> {
    println!(
        "\n{} ",
        Style::new().bold().fg(Green).paint("Creating new index")
    );
    println!(
        "{} ",
        Style::new()
            .bold()
            .fg(Green)
            .paint("Inferring fields from ndjson")
    );

    let inferred_schema = infer_schema_from_ndjson(&ndjson_path, sample_size)?;
    println!(
        "{}",
        Style::new().fg(Green).paint(format!(
            "Inferred {} fields from {} docs",
            inferred_schema.fields.len(),
            inferred_schema.docs_analyzed
        ))
    );

    let mut inferred_fields = inferred_schema.fields;
    inferred_fields.sort_by(compare_inferred_fields);
    for field in &inferred_fields {
        println!(
            "  - {}: {}",
            field.name,
            inferred_type_label(field.field_type)
        );
    }

    let mut schema_builder = SchemaBuilder::default();
    for field in &inferred_fields {
        ask_add_inferred_field(field, &mut schema_builder);
    }
    let schema = schema_builder.build();
    create_index_with_schema(directory, schema).map_err(|e| format!("{:?}", e))
}

fn run_new_interactive(directory: PathBuf) -> tantivy::Result<()> {
    println!(
        "\n{} ",
        Style::new().bold().fg(Green).paint("Creating new index")
    );
    println!(
        "{} ",
        Style::new()
            .bold()
            .fg(Green)
            .paint("First define its schema!")
    );
    let mut schema_builder = SchemaBuilder::default();
    loop {
        ask_add_field(&mut schema_builder);
        if !prompt_yn("Add another field") {
            break;
        }
    }
    let schema = schema_builder.build();
    create_index_with_schema(directory, schema)
}
