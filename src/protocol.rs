use ring::{
    self,
    aead::{Aad, BoundKey, NonceSequence, OpeningKey, SealingKey, UnboundKey},
    agreement::{EphemeralPrivateKey, UnparsedPublicKey},
    hkdf::{Prk, Salt},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub const PROTOCOL_VERSION: u32 = 1;
const KEY_INFO: &'static [u8] = b"pneumatic-key";

trait Session {
    type Response;
}

#[derive(Serialize, Deserialize)]
enum GreetingResponse {
    ProtocolOk,
    UnsupportedProtocol,
}

impl Session for Greeting {
    type Response = GreetingResponse;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Greeting {
    magic: &'static str,
    protocol_version: u32,
    public_key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Greeting { protocol_version: u32 },
    Disconnect,
}

impl Message {}

pub struct Connection {
    pub stream: TcpStream,
    keys: Keys,
}

#[derive(Debug, Clone, Copy)]
pub enum Relationship {
    IAmServer,
    IAmClient,
}

pub struct NonceCounter(u64);

impl NonceCounter {
    pub fn new() -> Self {
        NonceCounter(1)
    }
}

impl NonceSequence for NonceCounter {
    fn advance(&mut self) -> Result<ring::aead::Nonce, ring::error::Unspecified> {
        self.0 += 1;

        // TODO: Use full 96 bits?
        let mut nonce = [0u8; 96 / 8];
        nonce[0..8].copy_from_slice(&self.0.to_le_bytes());

        Ok(ring::aead::Nonce::assume_unique_for_key(nonce))
    }
}

struct Salts {
    encrypt_salt: Salt,
    decrypt_salt: Salt,
}

struct Keys {
    encrypt_key: SealingKey<NonceCounter>,
    decrypt_key: OpeningKey<NonceCounter>,
}

impl Connection {
    async fn exchange_salt(stream: &mut TcpStream, rng: &impl ring::rand::SecureRandom) -> Salts {
        let mut my_salt = vec![0u8; 32];
        rng.fill(&mut my_salt).unwrap();
        stream.write_all(&my_salt).await.unwrap();

        let mut other_salt = vec![0u8; 32];
        stream.read_exact(&mut other_salt).await.unwrap();

        let encrypt_salt = Salt::new(ring::hkdf::HKDF_SHA256, &my_salt);
        let decrypt_salt = Salt::new(ring::hkdf::HKDF_SHA256, &other_salt);

        Salts {
            encrypt_salt,
            decrypt_salt,
        }
    }

    fn expand_key(prk: Prk) -> [u8; 32] {
        let mut key = [0u8; 32];
        let okm = prk.expand(&[KEY_INFO], ring::hkdf::HKDF_SHA256).unwrap();
        okm.fill(&mut key).unwrap();
        key
    }

    fn bind_key<B: BoundKey<NonceCounter>>(key: [u8; 32]) -> B {
        let unbound_key = UnboundKey::new(&ring::aead::AES_256_GCM, &key).unwrap();
        let nonce_sequence = NonceCounter::new();
        B::new(unbound_key, nonce_sequence)
    }

    fn create_keys(
        private_key: EphemeralPrivateKey,
        peer_public_key: UnparsedPublicKey<Vec<u8>>,
        salts: Salts,
    ) -> Keys {
        let (encrypt_prk, decrypt_prk) = ring::agreement::agree_ephemeral(
            private_key,
            &peer_public_key,
            ring::error::Unspecified,
            |secret| {
                Ok((
                    salts.encrypt_salt.extract(&secret),
                    salts.decrypt_salt.extract(&secret),
                ))
            },
        )
        .unwrap();

        let encrypt_key = Self::expand_key(encrypt_prk);
        let decrypt_key = Self::expand_key(decrypt_prk);

        Keys {
            encrypt_key: Self::bind_key(encrypt_key),
            decrypt_key: Self::bind_key(decrypt_key),
        }
    }

    pub async fn new_encrypted(mut stream: TcpStream) -> Self {
        let rng = ring::rand::SystemRandom::new();

        let my_private_key =
            ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &rng).unwrap();
        let my_public_key = my_private_key.compute_public_key().unwrap();

        assert_eq!(my_public_key.as_ref().len(), 32);

        // Send public key
        let my_public_key_bytes: &[u8] = my_public_key.as_ref();
        stream.write_all(my_public_key_bytes).await.unwrap();

        // Read peer public key
        let mut peer_public_key_bytes = vec![0u8; 32];
        stream.read_exact(&mut peer_public_key_bytes).await.unwrap();

        let peer_public_key = ring::agreement::UnparsedPublicKey::new(
            &ring::agreement::X25519,
            peer_public_key_bytes,
        );

        let salts = Self::exchange_salt(&mut stream, &rng).await;
        let keys = Self::create_keys(my_private_key, peer_public_key, salts);

        Connection { stream, keys }
    }

    pub async fn send_message<M: Serialize>(&mut self, message: M) {
        let mut serialized = bincode::serialize(&message).unwrap();

        self.keys
            .encrypt_key
            .seal_in_place_append_tag(Aad::empty(), &mut serialized)
            .unwrap();

        self.stream
            .write_u32(serialized.len() as u32)
            .await
            .unwrap();

        self.stream.write_all(&serialized).await.unwrap();
    }

    pub async fn read_message<M: DeserializeOwned>(&mut self) -> M {
        let length = self.stream.read_u32().await.unwrap();
        let mut buffer = vec![0u8; length as usize];

        self.stream.read_exact(&mut buffer).await.unwrap();

        self.keys
            .decrypt_key
            .open_in_place(Aad::empty(), &mut buffer)
            .unwrap();

        bincode::deserialize(&buffer).unwrap()
    }
}
