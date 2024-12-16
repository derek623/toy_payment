use crate::models::Transaction;
use std::fs::File;
use std::io::BufReader;
use tokio::sync::mpsc::Sender;
use tracing::error;

pub struct CsvParser {
    path: String,
    tx: Sender<Transaction>,
}

impl CsvParser {
    pub fn new(path: String, tx: Sender<Transaction>) -> Self {
        Self { path, tx }
    }

    pub async fn run(&mut self) {
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open csv file: {e:?}");
                return;
            }
        };

        //Here I just use the default 8 KB buffer. If we want to change the buffer size, we can use with_capacity instead
        let reader = BufReader::new(file);
        let mut rdr = csv::Reader::from_reader(reader);
        for result in rdr.deserialize::<Transaction>() {
            match result {
                Ok(r) => {
                    if let Err(e) = self.tx.send(r).await {
                        error!("Failed to send transaction to engine: {e}");
                    }
                }
                Err(e) => error!("Failed to parse: {e}"),
            }
        }
    }
}
