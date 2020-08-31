use ring::{
    aead::{Aad, BoundKey, NonceSequence, OpeningKey, SealingKey, UnboundKey},
    agreement::{EphemeralPrivateKey, UnparsedPublicKey},
    hkdf::{Prk, Salt},
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const KEY_INFO: &'static [u8] = b"pneumatic-key";

struct Salts {
    encrypt_salt: Salt,
    decrypt_salt: Salt,
}

struct InitialKeys {
    my_private_key: EphemeralPrivateKey,
    peer_public_key: UnparsedPublicKey<Vec<u8>>,
}

struct Keys {
    encrypt_key: SealingKey<NonceCounter>,
    decrypt_key: OpeningKey<NonceCounter>,
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

async fn exchange_keys(stream: &mut TcpStream, rng: &impl ring::rand::SecureRandom) -> InitialKeys {
    let my_private_key =
        ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, rng).unwrap();
    let my_public_key = my_private_key.compute_public_key().unwrap();

    assert_eq!(my_public_key.as_ref().len(), 32);

    // Send public key
    let my_public_key_bytes: &[u8] = my_public_key.as_ref();
    stream.write_all(my_public_key_bytes).await.unwrap();

    // Read peer public key
    let mut peer_public_key_bytes = vec![0u8; 32];
    stream.read_exact(&mut peer_public_key_bytes).await.unwrap();

    let peer_public_key =
        ring::agreement::UnparsedPublicKey::new(&ring::agreement::X25519, peer_public_key_bytes);

    InitialKeys {
        my_private_key,
        peer_public_key,
    }
}

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

fn derive_keys(initial_keys: InitialKeys, salts: Salts) -> Keys {
    let InitialKeys {
        my_private_key,
        peer_public_key,
    } = initial_keys;

    let (encrypt_prk, decrypt_prk) = ring::agreement::agree_ephemeral(
        my_private_key,
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

    let encrypt_key = expand_key(encrypt_prk);
    let decrypt_key = expand_key(decrypt_prk);

    Keys {
        encrypt_key: bind_key(encrypt_key),
        decrypt_key: bind_key(decrypt_key),
    }
}

pub struct EncryptedStream {
    stream: TcpStream,
    keys: Keys,
}

impl EncryptedStream {
    pub async fn send_buffer(&mut self, buffer: &mut Vec<u8>) {
        self.keys
            .encrypt_key
            .seal_in_place_append_tag(Aad::empty(), buffer)
            .unwrap();

        self.stream.write_u32(buffer.len() as u32).await.unwrap();
        self.stream.write_all(buffer).await.unwrap();
    }

    pub async fn receive_buffer<'a>(&mut self, buffer: &'a mut Vec<u8>) -> &'a [u8] {
        let buffer_length = self.stream.read_u32().await.unwrap();
        buffer.resize_with(buffer_length as usize, Default::default);

        self.stream.read_exact(buffer).await.unwrap();

        self.keys
            .decrypt_key
            .open_in_place(Aad::empty(), buffer)
            .unwrap()
    }

    pub async fn send_bincode<S: Serialize>(&mut self, object: &S) {
        let mut buffer = bincode::serialize(object).unwrap();
        self.send_buffer(&mut buffer).await;
    }

    pub async fn receive_bincode<D: DeserializeOwned>(&mut self, buffer: &mut Vec<u8>) -> D {
        let decrypted = self.receive_buffer(buffer).await;
        bincode::deserialize(&decrypted).unwrap()
    }

    pub async fn new(mut stream: TcpStream) -> Self {
        let rng = ring::rand::SystemRandom::new();

        let keys = exchange_keys(&mut stream, &rng).await;
        let salts = exchange_salt(&mut stream, &rng).await;
        let keys = derive_keys(keys, salts);

        EncryptedStream { stream, keys }
    }
}
