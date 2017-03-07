use super::*;

use sodiumoxide;
use std::sync::{Once, ONCE_INIT};
use integer_encoding::VarInt;


static SODIUM_INITIALIZED: Once = ONCE_INIT;


pub struct Encryptor {
    pk: sodiumoxide::crypto::box_::PublicKey,
}

impl Encryptor {
    pub fn new(ek: &EncryptionKey) -> SdaClientResult<Encryptor> {
        // initialise Sodium per recommendations; 
        //  - documentation hints it's okay to do so more than once but we'll play it safe
        SODIUM_INITIALIZED.call_once(|| { sodiumoxide::init(); });

        match ek {

            &EncryptionKey::Sodium(raw_ek) => {
                let pk = sodiumoxide::crypto::box_::PublicKey::from_slice(&*raw_ek)
                    .ok_or("Failed to parse Sodium public key")?;
                Ok(Encryptor {
                    pk: pk,
                })
            },

        }
    }
}

impl ShareEncryptor for Encryptor {
    fn encrypt(&self, shares: &[Share]) -> SdaClientResult<Encryption> {
        // encode
        let mut encoded_shares = vec![];
        let mut buf = [0; 128];
        for &share in shares {
            let size = share.encode_var(&mut buf);
            encoded_shares.extend(&buf[0..size]);
        }
        // encrypt
        let raw_data = sodiumoxide::crypto::sealedbox::seal(&*encoded_shares, &self.pk);
        Ok(Encryption::Sodium(raw_data))
    }
}


pub struct Decryptor {
    pk: sodiumoxide::crypto::box_::PublicKey,
    sk: sodiumoxide::crypto::box_::SecretKey,
}

impl Decryptor {
    pub fn new<K: Store>(id: &EncryptionKeyId, keystore: &K) -> SdaClientResult<Decryptor> {
        match keystore.get(&id.stringify())? {

            Some((EncryptionKey::Sodium(raw_ek), DecryptionKey::Sodium(raw_dk))) => {

                let pk = sodiumoxide::crypto::box_::PublicKey::from_slice(&*raw_ek)
                    .ok_or("Failed to parse Sodium public key")?;

                let sk = sodiumoxide::crypto::box_::SecretKey::from_slice(&*raw_dk)
                    .ok_or("Failed to parse Sodium secret key")?;
                
                Ok(Decryptor {
                    pk: pk,
                    sk: sk,
                })
            },

            _ => Err("Wrong key type(s) for this decryptor")?,

        }
    }
}

impl ShareDecryptor for Decryptor {
    fn decrypt(&self, encryption: &Encryption) -> SdaClientResult<Vec<Share>> {
        let encryption = match encryption {
            &Encryption::Sodium(ref raw) => raw,
        };
        // decrypt
        let result = sodiumoxide::crypto::sealedbox::open(&encryption[..], &self.pk, &self.sk);
        // TODO better way of doing this?
        let raw_data = if result.is_ok() {
            Ok(result.unwrap())
        } else {
            Err("Sodium decryption failure")
        }?;
        // decode
        let mut reader = &raw_data[..];
        let mut decoded_shares = vec![];
        while reader.len() > 0 {
            let (i, size) = Share::decode_var(reader);
            decoded_shares.push(i);
            reader = &reader[size..];
        }
        Ok(decoded_shares)
    }
}


impl<K: Store> KeyGeneration<EncryptionKeyId> for CryptoModule<K> {
    fn new_key(&self) -> SdaClientResult<EncryptionKeyId> {
        // generate
        let (pk, sk) = sodiumoxide::crypto::box_::gen_keypair();
        let wrapped_ek = EncryptionKey::Sodium(pk.0.into());
        let wrapped_dk = DecryptionKey::Sodium(sk.0.into());
        
        // save
        let keypair = EncryptionKeypair { ek: wrapped_ek, dk: wrapped_dk };
        let id = EncryptionKeyId::random();
        self.keystore.put(&id.stringify(), &keypair)?;

        Ok(id)
    }
}