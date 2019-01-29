use crate::hash::{Source};

use std::fs::{self, File};
use std::io::{prelude::*, ErrorKind};
use std::path::PathBuf;
use std::string::String;

use failure::Error;

/// Load a shadowenv program from (generally) .shadowenv.d/*.scm. The returned Hash's source simply
/// concatenates all the files in order, but the hashsum is also dependent on the filenames.
pub fn load(at: PathBuf, relative_component: &str) -> Result<Option<Source>, Error> {
    let mut source = Source::new();

    for curr in at.ancestors() {
        let dirpath = curr.join(relative_component);

        match fs::read_dir(dirpath) {
            Ok(ref mut read_dir) => {
                for entry in read_dir {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_file() {
                            let basename = path.file_name().unwrap().to_str().unwrap().to_string();
                            if !basename.ends_with(".scm") {
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
            },
            Err(ref e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => { return Err(e.into()); },
        }
    }
    if source.files.len() == 0 {
        return Ok(None);
    }
    return Ok(Some(source));
}
