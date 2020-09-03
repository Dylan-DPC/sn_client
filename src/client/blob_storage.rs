// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{Client, CoreError};
use async_trait::async_trait;
use log::trace;
use safe_nd::{Blob, BlobAddress, PrivateBlob, PublicBlob};
use self_encryption::{Storage, StorageError};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use xor_name::{XorName, XOR_NAME_LEN};

/// Network storage is the concrete type which self_encryption crate will use
/// to put or get data from the network.
#[derive(Clone)]
pub struct BlobStorage {
    client: Client,
    published: bool,
}

impl BlobStorage {
    /// Create a new BlobStorage instance.
    pub fn new(client: Client, published: bool) -> Self {
        Self { client, published }
    }
}

#[async_trait]
impl Storage for BlobStorage {
    type Error = BlobStorageError;

    async fn get(&mut self, name: &[u8]) -> Result<Vec<u8>, Self::Error> {
        trace!("Self encrypt invoked GetBlob.");

        if name.len() != XOR_NAME_LEN {
            let err = CoreError::Unexpected("Requested `name` is incorrect size.".to_owned());
            let err = BlobStorageError::from(err);
            return Err(err);
        }

        let name = {
            let mut temp = [0_u8; XOR_NAME_LEN];
            temp.clone_from_slice(name);
            XorName(temp)
        };

        let address = if self.published {
            BlobAddress::Public(name)
        } else {
            BlobAddress::Private(name)
        };

        match self.client.get_blob(address, None, None).await {
            Ok(data) => Ok(data.value().clone()),
            Err(error) => Err(BlobStorageError::from(error)),
        }
    }

    async fn put(&mut self, _: Vec<u8>, data: Vec<u8>) -> Result<(), Self::Error> {
        trace!("Self encrypt invoked PutBlob.");
        let blob: Blob = if self.published {
            PublicBlob::new(data).into()
        } else {
            PrivateBlob::new(data, self.client.public_key().await).into()
        };
        match self.client.store_blob(blob).await {
            Ok(_r) => Ok(()),
            Err(error) => Err(BlobStorageError::from(error)),
        }
    }

    async fn generate_address(&self, data: &[u8]) -> Vec<u8> {
        let blob: Blob = if self.published {
            PublicBlob::new(data.to_vec()).into()
        } else {
            PrivateBlob::new(data.to_vec(), self.client.public_key().await).into()
        };
        blob.name().0.to_vec()
    }
}

/// Errors arising from storage object being used by self_encryptors.
#[derive(Debug)]
pub struct BlobStorageError(pub Box<CoreError>);

impl Display for BlobStorageError {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl Error for BlobStorageError {
    fn cause(&self) -> Option<&dyn Error> {
        self.0.source()
    }
}

impl From<CoreError> for BlobStorageError {
    fn from(error: CoreError) -> Self {
        Self(Box::new(error))
    }
}

impl StorageError for BlobStorageError {}

/// Network storage is the concrete type which self_encryption crate will use
/// to put or get data from the network.
#[derive(Clone)]
pub struct BlobStorageDryRun {
    client: Client,
    published: bool,
}

impl BlobStorageDryRun {
    /// Create a new BlobStorage instance.
    pub fn new(client: Client, published: bool) -> Self {
        Self { client, published }
    }
}

#[async_trait]
impl Storage for BlobStorageDryRun {
    type Error = BlobStorageError;

    async fn get(&mut self, _name: &[u8]) -> Result<Vec<u8>, Self::Error> {
        trace!("Self encrypt invoked GetBlob dry run.");
        Err(BlobStorageError::from(CoreError::Unexpected(
            "Cannot get from storage since it's a dry run.".to_owned(),
        )))
    }

    async fn put(&mut self, _: Vec<u8>, _data: Vec<u8>) -> Result<(), Self::Error> {
        trace!("Self encrypt invoked PutBlob dry run.");
        // We do nothing here just return ok so self_encrpytion can finish
        // and generate chunk addresses and datamap if required
        Ok(())
    }

    async fn generate_address(&self, data: &[u8]) -> Vec<u8> {
        let blob: Blob = if self.published {
            PublicBlob::new(data.to_vec()).into()
        } else {
            PrivateBlob::new(data.to_vec(), self.client.public_key().await).into()
        };
        blob.name().0.to_vec()
    }
}