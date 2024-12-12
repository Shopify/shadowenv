//! File TODO: Too many `map_err`, can be beautified.
use base64::Engine;
use crypto_box::{aead::Aead, Nonce, PublicKey, SalsaBox, SecretKey};
use nom::{
    bytes::complete::{tag, take_till},
    character::complete::digit1,
    combinator::{map, map_res},
    IResult,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    fs::{read, read_to_string},
    io,
    path::Path,
    str::FromStr,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EJsonError {
    #[error("Invalid EJSON: {}", .0)]
    InvalidJson(#[from] serde_json::Error),

    #[error(transparent)]
    IoErrorr(#[from] io::Error),

    /// Generic parsing error.
    #[error("{}", .0)]
    BoxParseError(String),
}

#[derive(Deserialize, Debug)]
struct EJsonFile {
    /// An EJSON file must have a public key associated, otherwise it's invalid.
    #[serde(rename = "_public_key")]
    pub public_key: String,

    /// All other key-value pairs contained in the file.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// Attempts to load an ejson file from the given path. Decodes all values in the
/// file using the public key specified in the file. Keys stay unchanged (no `_` removal).
///
/// Returns the entire parsed & decoded JSON file, minus the `_public_key` root field.
pub fn load_ejson_file(path: &Path) -> Result<Map<String, Value>, EJsonError> {
    let bytes = read(path)?;
    let mut parsed_file: EJsonFile = serde_json::from_slice(&bytes)?;

    let priv_key = find_private_key(&parsed_file.public_key)?;
    decode_map(&mut parsed_file.other, &priv_key)?;

    Ok(parsed_file.other)
}

fn decode_value(key: &str, value: &mut Value, private_key: &SecretKey) -> Result<(), EJsonError> {
    match value {
        Value::Object(obj) => decode_map(obj, private_key)?,
        Value::String(s) if !key.starts_with("_") => {
            *value = Value::String(decode_ejson_string(s, private_key)?);
        }
        Value::Array(array) => {
            for elem in array {
                decode_value(key, elem, private_key)?;
            }
        }
        _ => (),
    };

    Ok(())
}

fn decode_map(map: &mut Map<String, Value>, private_key: &SecretKey) -> Result<(), EJsonError> {
    for (key, value) in map.iter_mut() {
        decode_value(key, value, private_key)?;
    }

    Ok(())
}

fn decode_ejson_string(s: &str, private_key: &SecretKey) -> Result<String, EJsonError> {
    let (_, parsed) =
        parse_ejson_box(&s).map_err(|err| EJsonError::BoxParseError(err.to_string()))?;

    let keybox = SalsaBox::new(&parsed.encrypter_public_key()?, &private_key);
    let nonce = Nonce::from(parsed.nonce()?);
    let decrypted_plaintext = keybox
        .decrypt(&nonce, parsed.boxed_message()?.as_slice())
        .map_err(|err| {
            EJsonError::BoxParseError(format!("Unable to decrypt secret box `{s}`: {}", err))
        })?;

    String::from_utf8(decrypted_plaintext).map_err(|err| {
        EJsonError::BoxParseError(format!(
            "Decrypted message value for secret box `{s}` contains invalid UTF-8: {err}."
        ))
    })
}

#[derive(Debug)]
struct EJsonMessageBox<'input> {
    _schema_version: u32,
    /// Base64-encoded key used for encryption,
    encrypter_key_b64: &'input str,
    /// Base64-encoded nonce used for encryption,
    nonce_b64: &'input str,
    /// The encrypted message.
    boxed_message_b64: &'input str,
}

impl<'input> EJsonMessageBox<'input> {
    fn encrypter_public_key(&self) -> Result<PublicKey, EJsonError> {
        let pk_bytes = base64::engine::general_purpose::STANDARD
            .decode(self.encrypter_key_b64)
            .map_err(|_err| {
                EJsonError::BoxParseError("Encrypter public key is invalid base64".to_owned())
            })?;

        let pk_bytes: [u8; 32] = pk_bytes.try_into().map_err(|pk_bytes: Vec<u8>| {
            EJsonError::BoxParseError(format!(
                "Invalid nonce length: Found {}, must be 24",
                pk_bytes.len()
            ))
        })?;

        Ok(PublicKey::from_bytes(pk_bytes))
    }

    fn nonce(&self) -> Result<[u8; 24], EJsonError> {
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(self.nonce_b64)
            .map_err(|_err| EJsonError::BoxParseError("Nonce is invalid base64".to_owned()))?;

        nonce_bytes.try_into().map_err(|nonce_bytes: Vec<u8>| {
            EJsonError::BoxParseError(format!(
                "Invalid nonce length: Found {}, must be 24",
                nonce_bytes.len()
            ))
        })
    }

    fn boxed_message(&self) -> Result<Vec<u8>, EJsonError> {
        base64::engine::general_purpose::STANDARD
            .decode(self.boxed_message_b64)
            .map_err(|_err| EJsonError::BoxParseError("Boxed message is invalid base64".to_owned()))
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
            _schema_version: schema_version,
            encrypter_key_b64,
            nonce_b64,
            boxed_message_b64,
        },
    ))
}

// fn extract_pubkey(ejson: &EJsonFile) -> VerifyingKey {
//     let decoded = hex::decode(&ejson._public_key).unwrap();
//     let key_bytes: [u8; 32] = decoded[..32].try_into().unwrap();

//     VerifyingKey::from_bytes(&key_bytes).unwrap()
// }

fn find_private_key(hexed_key: &str) -> Result<SecretKey, EJsonError> {
    let hexed_private_key_bytes = read_to_string(format!("/opt/ejson/keys/{hexed_key}"))?;
    let decoded_bytes = hex::decode(hexed_private_key_bytes)
        .map_err(|_err| EJsonError::BoxParseError("Key is invalid base64".to_owned()))?;

    let key_bytes: [u8; 32] = decoded_bytes[..32].try_into().map_err(|_err| {
        EJsonError::BoxParseError("Invalid key length, must be 32 bytes".to_owned())
    })?;

    Ok(SecretKey::from_bytes(key_bytes))
}
