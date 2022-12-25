use crate::undo;

use std::collections::BTreeMap;
use std::env;
use std::io;
use std::io::Write;

enum ChangeType {
    Add,
    Remove,
}

trait Logger<'out> {
    fn with_output<W: Write>(o: &'out mut W, color: bool, json: bool) -> Self
    where
        Self: Sized;
    fn print(&mut self, value: String);
    fn pre_decoration(&self, change: Option<&ChangeType>) -> &str {
        match change {
            None => " ",
            Some(ChangeType::Add) => "+ ",
            Some(ChangeType::Remove) => "- ",
        }
    }
    fn post_decoration(&self, _change: Option<&ChangeType>) -> &str {
        ""
    }
    fn serialize(&self, name: &str, value: &str) -> String;
    fn format_name_value(&self, name: &str, value: &str, change: Option<&ChangeType>) -> String {
        format!(
            "{pre}{serialized}{post}",
            pre = self.pre_decoration(change),
            post = self.post_decoration(change),
            serialized = self.serialize(name, value)
        )
    }
}

struct OutputLogger<'out> {
    output: &'out mut dyn Write,
    color: bool,
    json: bool,
}

impl<'out> Logger<'out> for OutputLogger<'out> {
    fn with_output<W: Write>(o: &'out mut W, color: bool, json: bool) -> Self
    where
        Self: Sized,
    {
        Self {
            output: o,
            color,
            json,
        }
    }
    fn print(&mut self, value: String) {
        write!(self.output, "{}", value);
    }

    fn pre_decoration(&self, change: Option<&ChangeType>) -> &str {
        match (self.color, change) {
            (_, None) => "  ",
            (true, Some(ChangeType::Add)) => "\x1b[92m+ ",
            (true, Some(ChangeType::Remove)) => "\x1b[91m+ ",
            (false, Some(ChangeType::Add)) => " + ",
            (false, Some(ChangeType::Remove)) => " - ",
        }
    }

    fn post_decoration(&self, change: Option<&ChangeType>) -> &str {
        if self.color {
            // Clearing to EOL with \x1b[K prevents a weird issue where a wrapped line uses the last
            // non-null background color for the newline character, filling the rest of the space in the
            // line.
            "\x1b[0m\x1b[K"
        } else {
            ""
        }
    }

    fn serialize(&self, name: &str, value: &str) -> String {
        if self.json {
            format!("\"{}\":\"{}\"", name, value)
        } else {
            format!("{}={}", name, value)
        }
    }
}

/// print a diff of the env
pub fn run(verbose: bool, color: bool, json: bool, shadowenv_data: String) -> i32 {
    run_with_logger(
        &mut OutputLogger::with_output(&mut io::stdout().lock(), color, json),
        env::vars().collect(),
        verbose,
        shadowenv_data,
    )
}

fn run_with_logger(
    logger: &mut dyn Logger,
    env_vars: Vec<(String, String)>,
    verbose: bool,
    shadowenv_data: String,
) -> i32 {
    let mut parts = shadowenv_data.splitn(2, ':');
    let _prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");
    let shadowenv_data = undo::Data::from_str(json_data).unwrap();
    let mut scalars = shadowenv_data
        .scalars
        .iter()
        .map(|s| (s.name.clone(), s))
        .collect::<BTreeMap<_, _>>();
    let mut lists = shadowenv_data
        .lists
        .iter()
        .map(|s| (s.name.clone(), s))
        .collect::<BTreeMap<_, _>>();

    for (name, value) in env_vars {
        if let Some(scalar) = scalars.remove(&name) {
            diff_scalar(logger, &scalar)
        } else if let Some(list) = lists.remove(&name) {
            diff_list(logger, &list, &value)
        } else if verbose {
            print_verbose(logger, &name, &value)
        }
    }
    scalars
        .iter()
        .for_each(|(_name, scalar)| diff_scalar(logger, scalar));
    lists
        .iter()
        .for_each(|(_name, list)| diff_list(logger, list, ""));
    0
}

// TODO: fix!
fn diff_list(logger: &mut dyn Logger, list: &undo::List, current: &str) {
    let formatted_deletions: Vec<String> = if false {
        list.deletions
            .iter()
            .map(|x| "\x1b[48;5;52m".to_string() + x + "\x1b[0;91m")
            .collect()
    } else {
        list.deletions.clone()
    };
    let mut prefix = formatted_deletions.join(":");

    let items = current
        .split(':')
        .skip_while(|x| list.additions.contains(&x.to_string()));
    let items: Vec<&str> = items.collect();
    let suffix = items.join(":");
    if !suffix.is_empty() && !prefix.is_empty() {
        prefix += ":";
    }
    diff_remove(logger, &list.name, &(prefix + &suffix));

    let items = current.split(':').map(|x| {
        if list.additions.contains(&x.to_string()) && false {
            "\x1b[48;5;22m".to_string() + x + "\x1b[0;92m"
        } else {
            x.to_string()
        }
    });
    let items: Vec<String> = items.collect();
    let newline = items.join(":");
    diff_add(logger, &list.name, &newline);
}

fn diff_scalar(logger: &mut dyn Logger, scalar: &undo::Scalar) {
    if let Some(value) = &scalar.original {
        diff_remove(logger, &scalar.name, value);
    }
    if let Some(value) = &scalar.current {
        diff_add(logger, &scalar.name, value);
    }
}

fn diff_add(logger: &mut dyn Logger, name: &str, value: &str) {
    logger.print(logger.format_name_value(name, value, Some(&ChangeType::Add)))
}

fn diff_remove(logger: &mut dyn Logger, name: &str, value: &str) {
    logger.print(logger.format_name_value(name, value, Some(&ChangeType::Remove)))
}

fn print_verbose(logger: &mut dyn Logger, name: &str, value: &str) {
    logger.print(logger.format_name_value(name, value, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_test() {
        let mut out: Vec<u8> = vec![];
        let mut logger = OutputLogger::with_output(&mut out, true, false);
        let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[{"name":"VAR_C","additions":["/added"],"deletions":["/removed"]},{"name":"VAR_B","additions":["/added"],"deletions":[]},{"name":"VAR_A","additions":["/added"],"deletions":[]}]}"#;
        let env_vars = vec![
            ("VAR_A".to_string(), "/added:/existent".to_string()),
            ("VAR_B".to_string(), "/added".to_string()),
            ("VAR_C".to_string(), "/added:/existent".to_string()),
        ];

        let result = run_with_logger(&mut logger, env_vars, false, data.to_string());

        let expected: String = vec![
            "- VAR_A=/existent",
            "+ VAR_A=/added:/existent",
            "- VAR_B=",
            "+ VAR_B=/added",
            "- VAR_C=/removed:/existent",
            "+ VAR_C=/added:/existent",
        ]
        .join("");
        assert_eq!(result, 0);
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn json_test() {
        let mut out: Vec<u8> = vec![];
        let mut logger = OutputLogger::with_output(&mut out, false, true);
        let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[{"name":"VAR_C","additions":["/added"],"deletions":["/removed"]},{"name":"VAR_B","additions":["/added"],"deletions":[]},{"name":"VAR_A","additions":["/added"],"deletions":[]}]}"#;
        let env_vars = vec![
            ("VAR_A".to_string(), "/added:/existent".to_string()),
            ("VAR_B".to_string(), "/added".to_string()),
            ("VAR_C".to_string(), "/added:/existent".to_string()),
        ];

        let result = run_with_logger(&mut logger, env_vars, false, data.to_string());

        let expected: String = vec![
            "- VAR_A=/existent",
            "+ VAR_A=/added:/existent",
            "- VAR_B=",
            "+ VAR_B=/added",
            "- VAR_C=/removed:/existent",
            "+ VAR_C=/added:/existent",
        ]
        .join("");
        assert_eq!(result, 0);
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn nominal_test() {
        let mut out: Vec<u8> = vec![];
        let mut logger = OutputLogger::with_output(&mut out, false, false);

        let env_vars = vec![
            ("VAR_A".to_string(), "/added:/existent".to_string()),
            ("VAR_B".to_string(), "/added".to_string()),
            ("VAR_C".to_string(), "/added:/existent".to_string()),
        ];

        let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[{"name":"VAR_C","additions":["/added"],"deletions":["/removed"]},{"name":"VAR_B","additions":["/added"],"deletions":[]},{"name":"VAR_A","additions":["/added"],"deletions":[]}]}"#;
        let result = run_with_logger(&mut logger, env_vars, false, data.to_string());

        let expected: String = vec![
            "- VAR_A=/existent",
            "+ VAR_A=/added:/existent",
            "- VAR_B=",
            "+ VAR_B=/added",
            "- VAR_C=/removed:/existent",
            "+ VAR_C=/added:/existent",
        ]
        .join("");
        assert_eq!(result, 0);
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }
}
