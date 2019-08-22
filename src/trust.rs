use crate::loader;

use failure::Error;
use signatory::{ed25519, Ed25519Signature, PublicKeyed};
use signatory_dalek::{Ed25519Signer, Ed25519Verifier};

use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{prelude::*, ErrorKind};
use std::path::{Path, PathBuf};

#[derive(Fail, Debug)]
#[fail(display = "no shadowenv found")]
pub struct NoShadowenv;

#[derive(Fail, Debug)]
#[fail(
    display = "directory contains untrusted shadowenv program: `shadowenv help trust` to learn more."
)]
pub struct NotTrusted;

pub fn is_dir_trusted(dir: &PathBuf) -> Result<bool, Error> {
    let signer = load_or_generate_signer().unwrap();

    let root = match loader::find_root(dir.to_path_buf(), loader::DEFAULT_RELATIVE_COMPONENT)? {
        None => return Err(NoShadowenv {}.into()),
        Some(r) => r,
    };

    let pubkey = signer.public_key().unwrap();
    let fingerprint = hex::encode(&pubkey.as_bytes()[0..4]);

    let d = root.display().to_string();
    let msg = d.as_bytes();

    let path = trust_file(&root, fingerprint);
    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };

    match r_o_bytes? {
        None => Ok(false),
        Some(bytes) => {
            let sig = Ed25519Signature::new(from_vec(bytes));
            let verifier = Ed25519Verifier::from(&pubkey);
            Ok(ed25519::verify(&verifier, msg, &sig).is_ok())
        }
    }
}

fn load_or_generate_signer() -> Result<Ed25519Signer, Error> {
    let path = format!("{}/.config/shadowenv/trust-key", std::env::var("HOME")?);

    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match fs::read(Path::new(&path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };
    match r_o_bytes? {
        Some(bytes) => {
            let seed = ed25519::Seed::from_bytes(bytes)?;
            Ok(Ed25519Signer::from(&seed))
        }
        None => {
            let seed = ed25519::Seed::generate();

            std::fs::create_dir_all(Path::new(&path).to_path_buf().parent().unwrap())?;
            let mut file = match File::create(OsString::from(&path)) {
                // TODO: error type
                Err(why) => panic!("couldn''t write to {}: {}", path, why),
                Ok(f) => f,
            };

            file.write(seed.as_secret_slice())?;

            Ok(Ed25519Signer::from(&seed))
        }
    }
}

/// Trust this directory: create a new signature file.
pub fn run() -> Result<(), Error> {
    let signer = load_or_generate_signer().unwrap();

    let root = match loader::find_root(env::current_dir()?, loader::DEFAULT_RELATIVE_COMPONENT)? {
        None => return Err(NoShadowenv {}.into()),
        Some(r) => r,
    };

    let d = root.display().to_string();
    let msg = d.as_bytes();
    let sig = ed25519::sign(&signer, msg).unwrap();

    let pubkey = signer.public_key().unwrap();
    let fingerprint = hex::encode(&pubkey.as_bytes()[0..4]);

    let path = trust_file(&root, fingerprint);

    let mut file = match File::create(OsString::from(&path)) {
        // TODO: error type
        Err(why) => panic!("couldn't create {:?}: {}", path, why),
        Ok(file) => file,
    };

    write_gitignore(root)?;

    match file.write_all(sig.as_bytes()) {
        // TODO: error type
        Err(why) => panic!("couldn't write to {:?}: {}", path, why),
        Ok(_) => Ok(()),
    }
}

fn from_vec(vec: Vec<u8>) -> [u8; 64] {
    let bytes: &[u8] = &vec;
    let mut array = [0; 64];
    let bytes = &bytes[..array.len()]; // panics if not enough data
    array.copy_from_slice(bytes);
    array
}

fn write_gitignore(root: PathBuf) -> Result<(), Error> {
    let path = root.join(".gitignore");

    let r: Result<String, Error> = match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok("".to_string()),
        Err(e) => Err(e.into()),
    };
    let gitignore = r?;

    // is there a line reading one of: "*" "/*" ".*" "/.*" ?
    // If the latter, we likely wrote this file already, but we also encourage users to gitignore
    // *, so there's no need to clobber their changes if they've done so.
    let re = regex::Regex::new(r"(?m)^/?\.?\*$").unwrap();
    if !re.is_match(&gitignore) {
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        // ignore all .*, except for .gitignore
        file.write_all(b"/.*\n!/.gitignore\n")?;
    }

    Ok(())
}

fn trust_file(root: &PathBuf, fingerprint: String) -> PathBuf {
    root.join(format!(".trust-{}", fingerprint))
}
