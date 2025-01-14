// Copyright 2021 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

/// `MapInfo` utilities.
pub mod map_info;

/// Map APIs
pub mod map_apis;

/// Blob APIs
pub mod blob_apis;

/// Safe Transfers wrapper, with token APIs
pub mod transfer_actor;

/// Sequence APIs
pub mod sequence_apis;

/// Blob storage for self encryption.
pub mod blob_storage;

// sn_transfers wrapper
pub use self::map_info::MapInfo;
pub use self::transfer_actor::SafeTransferActor;

pub use blob_storage::{BlobStorage, BlobStorageDryRun};

use crate::{
    config_handler::Config,
    connection_manager::{Session, Signer},
    errors::Error,
};
use crdts::Dot;
use futures::lock::Mutex;
use log::{debug, info, trace, warn};
use qp2p::Config as QuicP2pConfig;
use rand::rngs::OsRng;
use sn_data_types::{Keypair, PublicKey, SectionElders, Token};
use sn_messaging::{
    client::{Cmd, DataCmd, Message, Query, QueryResponse},
    MessageId,
};
use std::{
    path::Path,
    str::FromStr,
    {collections::HashSet, net::SocketAddr, sync::Arc},
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Elder size
pub const ELDER_SIZE: usize = 5;

/// Capacity of the immutable data cache.
pub const IMMUT_DATA_CACHE_SIZE: usize = 300;

/// Capacity of the Sequence CRDT local replica size.
pub const SEQUENCE_CRDT_REPLICA_SIZE: usize = 300;

/// Client object
#[derive(Clone)]
pub struct Client {
    keypair: Keypair,
    transfer_actor: Arc<Mutex<SafeTransferActor<Keypair>>>,
    simulated_farming_payout_dot: Dot<PublicKey>,
    session: Session,
    notification_receiver: Arc<Mutex<UnboundedReceiver<Error>>>,
}

/// Easily manage connections to/from The Safe Network with the client and its APIs.
/// Use a random client for read-only or one-time operations.
/// Supply an existing, SecretKey which holds a SafeCoin balance to be able to perform
/// write operations.
impl Client {
    /// Create a Safe Network client instance. Either for an existing SecretKey (in which case) the client will attempt
    /// to retrieve the history of the key's balance in order to be ready for any token operations. Or if no SecreteKey
    /// is passed, a random keypair will be used, which provides a client that can only perform Read operations (at
    /// least until the client's SecretKey receives some token).
    ///
    /// # Examples
    ///
    /// Create a random client
    /// ```no_run
    /// # extern crate tokio; use anyhow::Result;
    /// # use sn_client::utils::test_utils::read_network_conn_info;
    /// use sn_client::Client;
    ///
    /// # #[tokio::main] async fn main() { let _: Result<()> = futures::executor::block_on( async {
    ///
    /// # let bootstrap_contacts = Some(read_network_conn_info()?);
    /// let client = Client::new(None, None, bootstrap_contacts).await?;
    /// // Now for example you can perform read operations:
    /// let _some_balance = client.get_balance().await?;
    /// # Ok(()) } ); }
    /// ```
    pub async fn new(
        optional_keypair: Option<Keypair>,
        config_file_path: Option<&Path>,
        bootstrap_config: Option<HashSet<SocketAddr>>,
    ) -> Result<Self, Error> {
        let mut rng = OsRng;

        let (keypair, is_random_client) = match optional_keypair {
            Some(id) => {
                info!("Client started for specific pk: {:?}", id.public_key());
                (id, false)
            }
            None => {
                let keypair = Keypair::new_ed25519(&mut rng);
                info!(
                    "Client started for new randomly created pk: {:?}",
                    keypair.public_key()
                );
                (keypair, true)
            }
        };

        let (notification_sender, notification_receiver) = unbounded_channel::<Error>();

        let mut qp2p_config = Config::new(config_file_path, bootstrap_config).qp2p;
        // We use feature `no-igd` so this will use the echo service only
        qp2p_config.forward_port = true;

        // Create the connection manager
        let session = attempt_bootstrap(qp2p_config, keypair.clone(), notification_sender).await?;

        // random PK used for from payment
        let random_payment_id = Keypair::new_ed25519(&mut rng);
        let random_payment_pk = random_payment_id.public_key();

        let simulated_farming_payout_dot = Dot::new(random_payment_pk, 0);

        let elder_pk_set = session
            .section_key_set
            .lock()
            .await
            .clone()
            .ok_or(Error::NotBootstrapped)?;
        let elder_names = session.get_elder_names().await;
        let elders = SectionElders {
            prefix: session
                .section_prefix()
                .await
                .ok_or(Error::NoSectionPrefixKnown)?,
            names: elder_names,
            key_set: elder_pk_set,
        };

        let transfer_actor = Arc::new(Mutex::new(SafeTransferActor::new(keypair.clone(), elders)));

        let mut client = Self {
            session,
            keypair,
            transfer_actor,
            simulated_farming_payout_dot,
            notification_receiver: Arc::new(Mutex::new(notification_receiver)),
        };

        if cfg!(feature = "simulated-payouts") {
            // only trigger simulated payouts on new _random_ clients
            if is_random_client {
                debug!("Attempting to trigger simulated payout");
                // we're testing, and currently a lot of tests expect 10 token to start
                let _ = client
                    .trigger_simulated_farming_payout(Token::from_str("10")?)
                    .await?;
            } else {
                warn!("No automatic simulated payout occurs for clients created for pre-existing SecretKeys")
            }
        }

        match client.get_history().await {
            Ok(_) => {}
            Err(error) => {
                let err = error.to_string();
                warn!("{:?}", &err);
            }
        };

        Ok(client)
    }

    /// Return the client's FullId.
    ///
    /// Useful for retrieving the PublicKey or KeyPair in the event you need to _sign_ something
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio; use anyhow::Result;
    /// # use sn_client::utils::test_utils::read_network_conn_info;
    /// use sn_client::Client;
    /// # #[tokio::main] async fn main() { let _: Result<()> = futures::executor::block_on( async {
    /// # let bootstrap_contacts = Some(read_network_conn_info()?);
    /// let client = Client::new(None, None, bootstrap_contacts).await?;
    /// let _keypair = client.keypair().await;
    ///
    /// # Ok(()) } ); }
    /// ```
    pub async fn keypair(&self) -> Keypair {
        self.keypair.clone()
    }

    /// Return the client's PublicKey.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # extern crate tokio; use anyhow::Result;
    /// # use sn_client::utils::test_utils::read_network_conn_info;
    /// use sn_client::Client;
    /// # #[tokio::main] async fn main() { let _: Result<()> = futures::executor::block_on( async {
    /// # let bootstrap_contacts = Some(read_network_conn_info()?);
    /// let client = Client::new(None, None, bootstrap_contacts).await?;
    /// let _pk = client.public_key().await;
    /// # Ok(()) } ); }
    /// ```
    pub async fn public_key(&self) -> PublicKey {
        let id = self.keypair().await;

        id.public_key()
    }

    /// Send a Query to the network and await a response
    async fn send_query(&self, query: Query) -> Result<QueryResponse, Error> {
        // `sign` should be false for GETs on published data, true otherwise.

        debug!("Sending QueryRequest: {:?}", query);

        let message = self.create_query_message(query).await?;

        self.session.send_query(&message).await
    }

    // Build and sign Cmd Message Envelope
    pub(crate) async fn create_cmd_message(&self, msg_contents: Cmd) -> Result<Message, Error> {
        let id = MessageId::new();
        trace!("Creating cmd message with id: {:?}", id);

        let target_section_pk = Some(self.session.section_key().await?);

        Ok(Message::Cmd {
            cmd: msg_contents,
            id,
            target_section_pk,
        })
    }

    // Build a Query
    pub(crate) async fn create_query_message(&self, msg_contents: Query) -> Result<Message, Error> {
        let id = MessageId::new();
        trace!("Creating query message with id : {:?}", id);

        let target_section_pk = Some(self.session.section_key().await?);

        Ok(Message::Query {
            query: msg_contents,
            id,
            target_section_pk,
        })
    }

    // Private helper to obtain payment proof for a data command, send it to the network,
    // and also apply the payment to local replica actor.
    async fn pay_and_send_data_command(&self, cmd: DataCmd) -> Result<(), Error> {
        // Payment for PUT
        let payment_proof = self.create_write_payment_proof(&cmd).await?;

        // The _actual_ message
        let msg_contents = Cmd::Data {
            cmd,
            payment: payment_proof.clone(),
        };
        let message = self.create_cmd_message(msg_contents).await?;

        let _ = self.session.send_cmd(&message).await?;

        self.apply_write_payment_to_local_actor(payment_proof).await
    }
}

/// Utility function that bootstraps a client to the network. If there is a failure then it retries.
/// After a maximum of three attempts if the boostrap process still fails, then an error is returned.
pub async fn attempt_bootstrap(
    qp2p_config: QuicP2pConfig,
    keypair: Keypair,
    notifier: UnboundedSender<Error>,
) -> Result<Session, Error> {
    let mut attempts: u32 = 0;

    let signer = Signer::new(keypair);

    let mut session = Session::new(qp2p_config, signer, notifier)?;
    loop {
        let res = session.bootstrap().await;
        match res {
            Ok(()) => return Ok(session),
            Err(err) => {
                attempts += 1;
                if attempts < 3 {
                    trace!("Error connecting to network! Retrying... ({})", attempts);
                } else {
                    return Err(err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_utils::{create_test_client, create_test_client_with};
    use anyhow::Result;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    pub async fn client_creation() -> Result<()> {
        let _client = create_test_client().await?;

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    pub async fn client_nonsense_bootstrap_fails() -> Result<()> {
        let mut nonsense_bootstrap = HashSet::new();
        let _ = nonsense_bootstrap.insert(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            3033,
        ));
        //let setup = create_test_client_with(None, Some(nonsense_bootstrap)).await;
        //assert!(setup.is_err());
        Ok(())
    }

    #[tokio::test]
    pub async fn client_creation_with_existing_keypair() -> Result<()> {
        let mut rng = OsRng;
        let full_id = Keypair::new_ed25519(&mut rng);
        let pk = full_id.public_key();

        let client = create_test_client_with(Some(full_id)).await?;
        assert_eq!(pk, client.public_key().await);

        Ok(())
    }

    #[tokio::test]
    pub async fn long_lived_connection_survives() -> Result<()> {
        let client = create_test_client().await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(40)).await;
        let balance = client.get_balance().await?;
        assert_ne!(balance, Token::from_nano(0));

        Ok(())
    }
}
