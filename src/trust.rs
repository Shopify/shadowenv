use crate::{get_current_dir_or_exit, loader};
use anyhow::Error;
use ed25519_dalek::{Signature, Signer, SigningKey};
use rand::rngs::OsRng;
use std::{
    convert::TryInto,
    env,
    fmt::Display,
    fs::{self, OpenOptions, Permissions},
    io::{prelude::*, ErrorKind},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
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
    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match read_signature(&path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };

    match r_o_bytes? {
        None => Ok(false),
        // A signature file of the wrong length can't be valid: treat it as no signature rather
        // than panicking on every prompt.
        Some(bytes) => match <[u8; 64]>::try_from(bytes.as_slice()) {
            Ok(bytes) => Ok(signer.verify(msg, &Signature::from_bytes(&bytes)).is_ok()),
            Err(_) => Ok(false),
        },
    }
}

/// Reads a signature file from a directory that may not be trusted yet, so its contents are the
/// attacker's: refuse anything but a regular file (a symlink to one is fine; one to `/dev/zero`
/// or a FIFO is not, and the FIFO mustn't block) and read no more than a signature's worth.
fn read_signature(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(65);
    file.take(65).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

fn load_or_generate_signer() -> Result<SigningKey, Error> {
    let path = format!("{}/.config/shadowenv/trust-key-v2", env::var("HOME")?);

    let r_o_bytes: Result<Option<Vec<u8>>, Error> = match fs::read(Path::new(&path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    };
    match r_o_bytes? {
        Some(bytes) => {
            // Older versions created the key with the umask's mode, usually world-readable, and
            // anyone who can read it can sign trust for their own directories. If this fails, we
            // can't change it anyway (someone else owns the file), and the key still works.
            if fs::metadata(&path).is_ok_and(|m| m.permissions().mode() & 0o077 != 0) {
                let _ = fs::set_permissions(&path, Permissions::from_mode(0o600));
            }
            // We used to write the entire keypair to the file, but now we only write the private key.
            // So it's important to take only the first 32 bytes here.
            let key_bytes: [u8; 32] = bytes
                .get(..32)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid key length"))?;
            Ok(SigningKey::from_bytes(&key_bytes))
        }
        None => {
            let mut csprng = OsRng {};
            let key = SigningKey::generate(&mut csprng);

            fs::create_dir_all(Path::new(&path).to_path_buf().parent().unwrap())?;
            // Only the user may read it; `create_new` won't follow a link planted at the path.
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;

            // Write out just the 32-byte private key.
            file.write_all(&key.to_bytes())?;
            Ok(key)
        }
    }
}

/// Trust the closest parent shadowenv root to the current working dir and create a new signature file.
pub fn run() -> Result<(), Error> {
    let dir = get_current_dir_or_exit();
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

    // Replace rather than overwrite: `create_new` won't follow a symlink planted at this path.
    let path = trust_file(root, fingerprint);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;

    write_gitignore(root)?;

    Ok(file.write_all(&sig.to_bytes())?)
}

fn write_gitignore(root: &Path) -> Result<(), Error> {
    let path = root.join(".gitignore");

    // Never write through a symlink planted here; a .gitignore that isn't a plain file is
    // someone's own arrangement, so leave it alone.
    if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
        return Ok(());
    }
    let r: Result<String, Error> = match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok("".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_files_are_checked_not_trusted_blindly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(loader::SHADOWENV_DIR_NAME);
        fs::create_dir(&root).unwrap();
        let signer = SigningKey::from_bytes(&[7; 32]);
        let fingerprint = hex::encode(&signer.verifying_key().as_bytes()[0..4]);

        assert!(!is_dir_trusted(&signer, &root).unwrap());
        trust_dir(&signer, &root).unwrap();
        assert!(is_dir_trusted(&signer, &root).unwrap());

        // A malformed signature file (e.g. shipped in an untrusted checkout) means untrusted.
        for junk in [&b"short"[..], &[0; 64][..], &[0; 65][..]] {
            fs::write(trust_file(&root, fingerprint.clone()), junk).unwrap();
            assert!(!is_dir_trusted(&signer, &root).unwrap());
        }
    }

    #[test]
    fn test_trust_files_are_never_written_through_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(loader::SHADOWENV_DIR_NAME);
        fs::create_dir(&root).unwrap();
        let signer = SigningKey::from_bytes(&[7; 32]);
        let fingerprint = hex::encode(&signer.verifying_key().as_bytes()[0..4]);
        let signature = trust_file(&root, fingerprint);

        // An unbounded or blocking "signature" must not hang the prompt.
        symlink("/dev/zero", &signature).unwrap();
        assert!(!is_dir_trusted(&signer, &root).unwrap());
        fs::remove_file(&signature).unwrap();

        // Trusting replaces a planted link instead of writing to its target.
        let victim = dir.path().join("victim");
        fs::write(&victim, "precious").unwrap();
        symlink(&victim, &signature).unwrap();
        trust_dir(&signer, &root).unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
        assert!(is_dir_trusted(&signer, &root).unwrap());

        // A symlink to a real signature still counts, as it always has.
        let elsewhere = dir.path().join("signature");
        fs::rename(&signature, &elsewhere).unwrap();
        symlink(&elsewhere, &signature).unwrap();
        assert!(is_dir_trusted(&signer, &root).unwrap());

        // A .gitignore that is a symlink is left alone, not written through.
        fs::remove_file(root.join(".gitignore")).unwrap();
        symlink(&victim, root.join(".gitignore")).unwrap();
        trust_dir(&signer, &root).unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
    }
}
