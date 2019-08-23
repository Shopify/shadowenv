use crate::hash::Source;

use std::fs::{self, File};
use std::io::{prelude::*, ErrorKind};
use std::path::PathBuf;
use std::string::String;

use failure::Error;

pub const DEFAULT_RELATIVE_COMPONENT: &'static str = ".shadowenv.d";

pub fn find_root(at: PathBuf, relative_component: &str) -> Result<Option<PathBuf>, Error> {
    for curr in at.ancestors() {
        let dirpath = curr.join(relative_component);

        match fs::read_dir(&dirpath) {
            Ok(_) => return Ok(Some(std::fs::canonicalize(dirpath)?)),
            Err(ref e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    return Ok(None);
}

/// Load a shadowenv program from (generally) .shadowenv.d/*.lisp. The returned Hash's source simply
/// concatenates all the files in order, but the hashsum is also dependent on the filenames.
/// Note that this function assumes that the dirpath is trusted.
pub fn load(dirpath: PathBuf) -> Result<Option<Source>, Error> {
    let mut source = Source::new();

    for entry in fs::read_dir(dirpath)? {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                // TODO: there HAS to  be a better way to do this.
                let basename = path.file_name().unwrap().to_str().unwrap().to_string();
                if !basename.ends_with(".lisp") {
                    continue;
                }
                let mut file = File::open(&path)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                // TODO: surely  there's a better way to do this.
                source.add_file(basename, contents)?;
            }
        }
    }

    if source.files.len() == 0 {
        return Ok(None);
    }
    return Ok(Some(source));
}
