use std::fmt;

use failure::{Error, Fail};

use crate::undo;

#[derive(Fail, Debug)]
#[fail(display = "Could not output changes")]
pub struct ChangesError;

#[derive(PartialEq, Eq, Debug)]
pub struct Changes {
    added: usize,
    removed: usize,
    updated: usize,
}

impl fmt::Display for Changes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "+{added} -{removed} ^{updated}",
            added = self.added,
            removed = self.removed,
            updated = self.updated
        )
    }
}

#[derive(Debug)]
enum ChangeType {
    Added,
    Removed,
    Updated,
    Unchanged,
}

/// How has this environment variable changed?
fn change_type(v: &undo::Scalar) -> ChangeType {
    match (v.original.as_ref(), v.current.as_ref()) {
        (None, None) => ChangeType::Unchanged,
        (None, Some(_)) => ChangeType::Added,
        (Some(_), None) => ChangeType::Removed,
        (Some(a), Some(b)) if a == b => ChangeType::Unchanged,
        (Some(a), Some(b)) if a == "" && b != "" => ChangeType::Added,
        (Some(a), Some(b)) if a != "" && b == "" => ChangeType::Removed,
        (Some(a), Some(b)) if a != b => ChangeType::Updated,
        (Some(_), Some(_)) => ChangeType::Updated,
    }
}

/// Output a simple summary of the changes
pub fn run(shadowenv_data: String) -> Result<Changes, Error> {
    let mut parts = shadowenv_data.splitn(2, ':');
    let _prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");
    let shadowenv_data = undo::Data::from_str(json_data).unwrap();
    let mut changes = Changes {
        added: 0,
        removed: 0,
        updated: 0,
    };
    for s in shadowenv_data.scalars {
        match change_type(&s) {
            ChangeType::Added => changes.added += 1,
            ChangeType::Updated => changes.updated += 1,
            ChangeType::Removed => changes.removed += 1,
            ChangeType::Unchanged => {}
        }
    }
    for l in shadowenv_data.lists {
        if l.additions.len() != 0 || l.deletions.len() != 0 {
            changes.updated += 1;
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_test() {
        let changes = Changes {
            added: 1,
            updated: 2,
            removed: 3,
        };
        assert_eq!(changes.to_string(), "+1 -3 ^2",);
    }

    #[test]
    fn nominal_test() {
        let data = r#"d6508595be775af2:{"scalars":[{"name":"CLICOLOR","original":"1","current":"0"},{"name":"DEBUG","original":null,"current":"1"},{"name":"GOPATH","original":"/Users/rami/Go","current":""},{"name":"INFOPATH","original":"/opt/homebrew/share/info:","current":""},{"name":"NODE_PATH","original":"/Users/rami/.npm/packages/lib/node_modules:","current":""}],"lists":[{"name":"PATH","additions":["/usr/libexec/bin"],"deletions":[]}]}"#;
        let result = run(data.to_string());

        let expected: Changes = Changes {
            added: 1,
            updated: 2,
            removed: 3,
        };
        assert_eq!(result.unwrap(), expected);
    }
}
