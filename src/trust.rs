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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_or_generate_signer() {
        let signer = load_or_generate_signer().unwrap();
        assert_eq!(signer.to_bytes().len(), 32);
    }

    #[test]
    fn test_is_dir_trusted() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        let signer = load_or_generate_signer().unwrap();
        let result = is_dir_trusted(&signer, &path);
        assert!(result.is_ok());
        assert!(!result.unwrap());

        trust_dir(&signer, &path).unwrap();
        let result = is_dir_trusted(&signer, &path);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_run_no_shadowenv() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let result = run(path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().downcast_ref::<NoShadowenv>(), Some(_)));
    }

    #[test]
    fn test_write_gitignore() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        write_gitignore(&path).unwrap();
        let gitignore_content = fs::read_to_string(path.join(".gitignore")).unwrap();
        assert!(gitignore_content.contains("/.*\n!/.gitignore\n"));
    }

    #[test]
    fn test_ensure_dir_tree_trusted() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        let result = ensure_dir_tree_trusted(&[path]);
        assert!(result.is_err());
    }

    #[test]
    fn test_trust_dir() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        let signer = load_or_generate_signer().unwrap();
        let result = trust_dir(&signer, &path);
        assert!(result.is_ok());
    }
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
            // We used to write the entire keypair to the file, but now we only write the private key.
            // So it's important to take only the first 32 bytes here.
            let key_bytes: [u8; 32] = bytes[..32].try_into()
                .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
            Ok(SigningKey::from_bytes(&key_bytes))
        }
        None => {
            let mut csprng = OsRng {};
            let key = SigningKey::generate(&mut csprng);

            fs::create_dir_all(Path::new(&path).to_path_buf().parent().unwrap())?;
            let mut file = match File::create(OsString::from(&path)) {
                // TODO: error type
                Err(why) => panic!("couldn''t write to {}: {}", path, why),
                Ok(f) => f,
            };

            // Write out just the 32-byte private key.
            file.write_all(&key.to_bytes())?;
            Ok(key)
        }
    }
}

/// Trust the closest parent shadowenv root to the current working dir and create a new signature file.
pub fn run(dir: PathBuf) -> Result<(), Error> {
    let signer = load_or_generate_signer().unwrap();

    let roots = loader::find_shadowenv_paths(&dir)?;
    if roots.is_empty() {
        return Err(NoShadowenv {}.into());
    }

    // `roots`: Closer roots to current dir have lower indices, so we take the first element here.
    // Unwrap is safe: We're checking `is_empty` above.
    trust_dir(&signer, roots.first().unwrap())?;
    Ok(())
}

/// Trust the shadowenv dir at `root`. Assumes `root` points to a valid shadowenv directory.
fn trust_dir(signer: &SigningKey, root: &Path) -> Result<(), Error> {
    let msg = root.to_string_lossy();
    let sig = signer.sign(msg.as_bytes());

    let pubkey = signer.verifying_key();
    let fingerprint = hex::encode(&pubkey.as_bytes()[0..4]);

    let path = trust_file(root, fingerprint);
    let mut file = File::create(&path)?;

    write_gitignore(root)?;

    Ok(file.write_all(&sig.to_bytes())?)
}

fn write_gitignore(root: &Path) -> Result<(), Error> {
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
