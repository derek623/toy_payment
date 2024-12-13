use crate::parser::csv_parser::CsvParser;
use clap::Parser;
use futures_util::future::join_all;
use tokio::sync::mpsc;
use tranasction::transaction_engine::TransactionEngine;

mod models;
mod parser;
mod tranasction;

//channel size should be configured based on benchmarking
const CHANNEL_SIZE: usize = 10000;

#[derive(Parser)]
#[command(about, long_about = None)]
struct Args {
    /// csv file name
    input_file: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    tracing::info!("File is {}", args.input_file);
    let (tx, rx) = mpsc::channel(CHANNEL_SIZE);

    let mut parser = CsvParser::new(args.input_file, tx);
    let mut transaction_engine = TransactionEngine::new(rx);

    let mut handles = vec![];
    handles.push(tokio::spawn(async move {
        parser.run().await;
    }));
    handles.push(tokio::spawn(async move {
        transaction_engine.run().await;
    }));

    let _ = join_all(handles).await;
}
