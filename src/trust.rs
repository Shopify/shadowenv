use crate::loader;
use anyhow::Error;
use ed25519_dalek::{Signature, Signer, SigningKey};
use rand::rngs::OsRng;
use std::{
    convert::TryInto,
    env,
    ffi::OsString,
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::{prelude::*, ErrorKind},
    path::{Path, PathBuf},
};
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
#[error("no shadowenv found")]
pub struct NoShadowenv;

#[derive(ThisError, Debug)]
pub struct NotTrusted {
    pub untrusted_directories: Vec<String>,
}

impl Display for NotTrusted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.untrusted_directories.as_slice() {
            [single] => write!(f, "directory: '{}' contains untrusted shadowenv program: `shadowenv help trust` to learn more.", single)?,
            multi => {
                write!(f, "The following directories contain untrusted shadowenv programs (see `shadowenv help trust` to learn more):\n{}", multi.join("\n"))?
            },
        };

        Ok(())
    }
}

pub fn ensure_dir_tree_trusted(roots: &[PathBuf]) -> Result<(), Error> {
    let signer = load_or_generate_signer()?;

    let mut untrusted = vec![];
    for root in roots {
        if !is_dir_trusted(&signer, root)? {
            untrusted.push(root.to_string_lossy().to_string());
        }
    }

    if untrusted.is_empty() {
        Ok(())
    } else {
        Err(NotTrusted {
            untrusted_directories: untrusted,
        }
        .into())
    }
}

fn is_dir_trusted(signer: &SigningKey, root: &Path) -> Result<bool, Error> {
    let pubkey = signer.verifying_key();
    let fingerprint = hex::encode(&pubkey.as_bytes()[0..4]);

    let d = root.display().to_string();
    let msg = d.as_bytes();

    let path = trust_file(root, fingerprint);
    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };

    match r_o_bytes? {
        None => Ok(false),
        Some(bytes) => {
            let sig = Signature::from_bytes(&bytes.try_into().unwrap());
            Ok(signer.verify(msg, &sig).is_ok())
        }
    }
}

fn load_or_generate_signer() -> Result<SigningKey, Error> {
    let path = format!("{}/.config/shadowenv/trust-key-v2", env::var("HOME")?);

    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match fs::read(Path::new(&path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };
    match r_o_bytes? {
        Some(bytes) => {
            let key = SigningKey::from_keypair_bytes(&bytes.try_into().unwrap())?;
            Ok(key)
        }
        None => {
            let mut csprng = OsRng {};
            let seed = SigningKey::generate(&mut csprng);

            fs::create_dir_all(Path::new(&path).to_path_buf().parent().unwrap())?;
            let mut file = match File::create(OsString::from(&path)) {
                // TODO: error type
                Err(why) => panic!("couldn''t write to {}: {}", path, why),
                Ok(f) => f,
            };

            file.write_all(&seed.to_bytes())?;
            Ok(seed)
        }
    }
}

/// Trust the closest parent shadowenv root to the current working dir and create a new signature file.
pub fn run() -> Result<(), Error> {
    let signer = load_or_generate_signer().unwrap();

    let roots = loader::find_roots(&env::current_dir()?, loader::DEFAULT_RELATIVE_COMPONENT)?;
    if roots.is_empty() {
        return Err(NoShadowenv {}.into());
    }

    // `roots`: Closer roots to current dir have lower indices, so we take the first element here.
    // Unwrap is safe: We're checking `is_empty` above.
    trust_dir(&signer, roots.first().unwrap())?;
    Ok(())
}

/// Trust the shadowenv dir at `root`. Assumes `root` points to a valid shadowenv directory.
fn trust_dir(signer: &SigningKey, root: &PathBuf) -> Result<(), Error> {
    let msg = root.to_string_lossy();
    let sig = signer.sign(msg.as_bytes());

    let pubkey = signer.verifying_key();
    let fingerprint = hex::encode(&pubkey.as_bytes()[0..4]);

    let path = trust_file(&root, fingerprint);
    let mut file = File::create(&path)?;

    write_gitignore(root)?;

    Ok(file.write_all(&sig.to_bytes())?)
}

fn write_gitignore(root: &PathBuf) -> Result<(), Error> {
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

fn trust_file(root: &Path, fingerprint: String) -> PathBuf {
    root.join(format!(".trust-{}", fingerprint))
}
