use sn_data_types::{PublicBlob, PrivateBlob, Blob, Keypair};
use sn_client::{Error, ErrorMessage, Client};
use tokio::time::{sleep, Duration};
use rand::prelude::Distribution;
use rand::distributions::Standard;
use rand::rngs::OsRng;
use rand::Rng;
use anyhow::{bail, Context, anyhow, Result};
use std::fs::File;
use dirs_next::home_dir;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Once;
use tracing_subscriber::{fmt, EnvFilter};
use std::io::BufReader;

const GENESIS_CONN_INFO_FILEPATH: &str = ".safe/node/node_connection_info.config";

async fn gen_data_then_create_and_retrieve(size: usize, public: bool) -> Result<()> {
    let raw_data = generate_random_vector(size);

    let client = create_test_client_with(None).await?;

    // gen address without putting to the network (public and unencrypted)
    let blob = if public {
        Blob::Public(PublicBlob::new(raw_data.clone()))
    } else {
        Blob::Private(PrivateBlob::new(
            raw_data.clone(),
            client.public_key().await,
        ))
    };

    let address_before = blob.address();

    // attempt to retrieve it with generated address (it should error)
    let res = client.read_blob(*address_before, None, None).await;
    match res {
        Err(Error::ErrorMessage {
                source: ErrorMessage::DataNotFound(_),
                ..
            }) => (),
        Ok(_) => bail!("Blob unexpectedly retrieved using address generated by gen_data_map"),
        Err(_) => bail!(
                "Unexpected error when Blob retrieved using address generated by gen_data_map"
            ),
    };

    let address = if public {
        client.store_public_blob(&raw_data).await?
    } else {
        client.store_private_blob(&raw_data).await?
    };

    let mut fetch_result;
    // now that it was put to the network we should be able to retrieve it
    fetch_result = client.read_blob(address, None, None).await;

    while fetch_result.is_err() {
        sleep(Duration::from_millis(200)).await;

        fetch_result = client.read_blob(address, None, None).await;
    }

    // then the content should be what we put
    assert_eq!(fetch_result?, raw_data);

    // now let's test Blob data map generation utility returns the correct Blob address
    let privately_owned = if public {
        None
    } else {
        Some(client.public_key().await)
    };
    let (_, blob_address) = Client::blob_data_map(raw_data, privately_owned).await?;
    assert_eq!(blob_address, address);

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let size = 1024 * 1024 * 10;
    gen_data_then_create_and_retrieve(size, true).await?;

    Ok(())
}

/// Generates a random vector using provided `length`.
pub fn generate_random_vector<T>(length: usize) -> Vec<T>
    where
        Standard: Distribution<T>,
{
    let mut rng = OsRng;
    ::std::iter::repeat(())
        .map(|()| rng.gen::<T>())
        .take(length)
        .collect()
}

pub async fn create_test_client_with(optional_keypair: Option<Keypair>) -> Result<Client> {
    init_logger();
    let contact_info = read_network_conn_info()?;
    let client = Client::new(optional_keypair, None, Some(contact_info)).await?;
    Ok(client)
}

static INIT: Once = Once::new();

/// Initialise logger for tests, this is run only once, even if called multiple times.
pub fn init_logger() {
    INIT.call_once(|| {
        fmt()
            // NOTE: comment out this line for more compact (but less readable) log output.
            .pretty()
            .with_thread_names(true)
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .init()
    });
}

pub fn read_network_conn_info() -> Result<HashSet<SocketAddr>> {
    let user_dir = home_dir().ok_or_else(|| anyhow!("Could not fetch home directory"))?;
    let conn_info_path = user_dir.join(Path::new(GENESIS_CONN_INFO_FILEPATH));

    let file = File::open(&conn_info_path).with_context(|| {
        format!(
            "Failed to open node connection information file at '{}'",
            conn_info_path.display(),
        )
    })?;
    let reader = BufReader::new(file);
    let contacts: HashSet<SocketAddr> = serde_json::from_reader(reader).with_context(|| {
        format!(
            "Failed to parse content of node connection information file at '{}'",
            conn_info_path.display(),
        )
    })?;

    Ok(contacts)
}
