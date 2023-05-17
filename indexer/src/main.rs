use std::collections::HashMap;
use std::str::FromStr;

use clap::Parser;
use indexer::collect_transactions;
use indexer::filter_outcomes;
use indexer::process_outcome;
use itertools::Itertools;
use near_lake_framework::near_indexer_primitives;
use near_lake_framework::LakeConfig;

use configs::{init_logging, Opts};
use tokio::sync::mpsc;

mod configs;

/// Assuming we want to watch for transactions where a receiver account id is one of the provided in a list
/// We pass the list of account ids (or contracts it is the same) via argument ``--accounts``
/// We want to catch all *successfull* transactions sent to one of the accounts from the list.
/// In the demo we'll just look for them and log them but it might and probably should be extended based on your needs.

#[tokio::main]
async fn main() -> Result<(), tokio::io::Error> {
    init_logging();

    let opts: Opts = Opts::parse();

    let config: LakeConfig = opts.clone().into();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is not set");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("Migration failed");

    let (_, stream) = near_lake_framework::streamer(config);

    let watching_list = opts
        .accounts
        .split(',')
        .map(|elem| {
            near_indexer_primitives::types::AccountId::from_str(elem).expect("AccountId is invalid")
        })
        .collect();

    listen_blocks(stream, watching_list, &pool).await;

    Ok(())
}

/// The main listener function the will be reading the stream of blocks `StreamerMessage`
/// and perform necessary checks
pub async fn listen_blocks(
    mut stream: mpsc::Receiver<near_indexer_primitives::StreamerMessage>,
    watching_list: Vec<near_indexer_primitives::types::AccountId>,
    pool: &sqlx::PgPool,
) {
    // This will be a map of correspondence between transactions and receipts
    let mut tx_receipt_ids = HashMap::new();
    // This will be a list of receipt ids we're following
    let engine = base64::engine::general_purpose::STANDARD;

    // Boilerplate code to listen the stream
    while let Some(streamer_message) = stream.recv().await {
        tx_receipt_ids.extend(collect_transactions(&streamer_message, &watching_list));
        let txs = filter_outcomes(&streamer_message, &mut tx_receipt_ids)
            .into_iter()
            .flat_map(|(outcome, tx_hash)| {
                process_outcome(outcome, tx_hash, &engine, &streamer_message.block)
            })
            .collect_vec();
        for tx in txs {
            tx.insert(&pool)
                .await
                .expect("Failed to insert transaction");
        }
    }
}
