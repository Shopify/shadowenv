use base64::Engine;
use crypto_box::{aead::Aead, Nonce, PublicKey, SalsaBox, SecretKey};
use ed25519_dalek::VerifyingKey;
use nom::{
    bytes::complete::{tag, take_till},
    character::complete::digit1,
    combinator::{map, map_res},
    IResult,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    fs::read,
    path::{Path, PathBuf},
    str::FromStr,
};

fn decode_ejson(ejson: &mut Map<String, Value>, private_key: &SecretKey) {
    decode_map(ejson, private_key);
}

fn decode_value(key: &str, value: &mut Value, private_key: &SecretKey) {
    match value {
        Value::String(s) if !key.starts_with("_") => {
            if let Some(decoded) = decode_ejson_string(s, private_key) {
                *value = Value::String(decoded);
            }
        }
        Value::Object(obj) => decode_map(obj, private_key),
        Value::Array(array) => array
            .iter_mut()
            .for_each(|elem| decode_value(key, elem, private_key)),
        _ => (),
    }
}

fn decode_map(map: &mut Map<String, Value>, private_key: &SecretKey) {
    for (key, value) in map.iter_mut() {
        decode_value(key, value, private_key);
    }
}

fn decode_ejson_string(s: &str, private_key: &SecretKey) -> Option<String> {
    let (_, parsed) = parse_ejson_box(&s).ok()?;
    let keybox = SalsaBox::new(&parsed.encrypter_public_key(), &private_key);
    let nonce = Nonce::from(parsed.nonce());

    let decrypted_plaintext = keybox
        .decrypt(&nonce, parsed.boxed_message().as_slice())
        .ok()?;

    String::from_utf8(decrypted_plaintext).ok()
}

#[derive(Debug)]
struct EJsonMessageBox<'input> {
    schema_version: u32,
    /// Base64-encoded key used for encryption,
    encrypter_key_b64: &'input str,
    /// Base64-encoded nonce used for encryption,
    nonce_b64: &'input str,
    /// The encrypted message.
    boxed_message_b64: &'input str,
}

impl<'input> EJsonMessageBox<'input> {
    fn encrypter_public_key(&self) -> PublicKey {
        let bytes: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(self.encrypter_key_b64)
            .unwrap()
            .try_into()
            .unwrap();

        PublicKey::from_bytes(bytes)
    }

    fn nonce(&self) -> [u8; 24] {
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(self.nonce_b64)
            .unwrap();

        if nonce_bytes.len() != 24 {
            panic!("Invalid nonce length: {}", nonce_bytes.len());
        }

        nonce_bytes.try_into().unwrap()
    }

    fn boxed_message(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(self.boxed_message_b64)
            .unwrap()
    }
}

fn parse_ejson_box<'input>(input: &'input str) -> IResult<&str, EJsonMessageBox<'input>> {
    let (input, _) = tag("EJ[")(input)?;
    let (input, schema_version) =
        map(take_till(|c| c == ':'), map_res(digit1, u32::from_str))(input)?;
    let (_, schema_version) = schema_version?;

    let (input, _) = tag(":")(input)?;
    let (input, encrypter_key_b64) = take_till(|c| c == ':')(input)?;

    let (input, _) = tag(":")(input)?;
    let (input, nonce_b64) = take_till(|c| c == ':')(input)?;

    let (input, _) = tag(":")(input)?;
    let (_input, boxed_message_b64) = take_till(|c| c == ']')(input)?;

    Ok((
        input,
        EJsonMessageBox {
            schema_version,
            encrypter_key_b64,
            nonce_b64,
            boxed_message_b64,
        },
    ))
}

#[derive(Deserialize, Debug)]
struct EJsonFile {
    _public_key: String,

    #[serde(flatten)]
    other: Map<String, Value>,
}

fn load_ejson_file(path: &Path) -> EJsonFile {
    let bytes = read(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn extract_pubkey(ejson: &EJsonFile) -> VerifyingKey {
    let decoded = hex::decode(&ejson._public_key).unwrap();
    let key_bytes: [u8; 32] = decoded[..32].try_into().unwrap();

    VerifyingKey::from_bytes(&key_bytes).unwrap()
}

fn find_private_key(hexed_key: &str) -> SecretKey {
    let path = PathBuf::from_str(&format!("/opt/ejson/keys/{hexed_key}")).unwrap();

    let private_key_bytes = String::from_utf8(read(path).unwrap())
        .unwrap()
        .trim()
        .to_owned();

    let decoded = hex::decode(private_key_bytes).unwrap();
    let key_bytes: [u8; 32] = decoded[..32].try_into().unwrap();

    SecretKey::from_bytes(key_bytes)
}
