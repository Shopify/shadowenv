use crate::undo;

use std::collections::BTreeMap;
use std::env;

trait Logger {
    fn print(&mut self, value: String);
}

#[test]
fn test_colored_output() {
    let mut logger = DummyLogger::default();

    let env_vars = vec![("VAR_A".to_string(), "/added:/existent".to_string())];

    let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[{"name":"VAR_A","additions":["/added"],"deletions":[]}]}"#;
    let result = run_with_logger(&mut logger, env_vars, false, true, data.to_string());

    let expected: Vec<_> = ["\x1b[92m+ VAR_A=/added:/existent\x1b[0m"]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(result, 0);
    assert_eq!(logger.0, expected);
}

#[test]
fn test_verbose_mode() {
    let mut logger = DummyLogger::default();

    let env_vars = vec![("VAR_A".to_string(), "/existent".to_string())];

    let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[]}"#;
    let result = run_with_logger(&mut logger, env_vars, true, false, data.to_string());

    let expected: Vec<_> = ["  VAR_A=/existent"]
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(result, 0);
    assert_eq!(logger.0, expected);
}

#[test]
fn test_empty_env_vars() {
    let mut logger = DummyLogger::default();

    let env_vars = vec![];

    let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[]}"#;
    let result = run_with_logger(&mut logger, env_vars, false, false, data.to_string());

    let expected: Vec<String> = vec![];
    assert_eq!(result, 0);
    assert_eq!(logger.0, expected);
}

#[test]
fn test_missing_shadowenv_data() {
    let mut logger = DummyLogger::default();

    let env_vars = vec![("VAR_A".to_string(), "/existent".to_string())];

    let data = r#""#;
    let result = run_with_logger(&mut logger, env_vars, false, false, data.to_string());

    let expected: Vec<String> = vec![];
    assert_eq!(result, 0);
    assert_eq!(logger.0, expected);
}
#[derive(Default)]
struct DummyLogger(Vec<String>);

impl Logger for DummyLogger {
    fn print(&mut self, value: String) {
        self.0.push(value);
    }
}

struct StdoutLogger;

impl Logger for StdoutLogger {
    fn print(&mut self, value: String) {
        println!("{}", value);
    }
}

/// print a diff of the env
pub fn run(verbose: bool, color: bool, shadowenv_data: String) -> i32 {
    run_with_logger(
        &mut StdoutLogger {},
        env::vars().collect(),
        verbose,
        color,
        shadowenv_data,
    )
}

fn run_with_logger(
    logger: &mut dyn Logger,
    env_vars: Vec<(String, String)>,
    verbose: bool,
    color: bool,
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
            diff_scalar(logger, scalar, color)
        } else if let Some(list) = lists.remove(&name) {
            diff_list(logger, list, &value, color)
        } else if verbose {
            print_verbose(logger, &name, &value)
        }
    }
    scalars
        .iter()
        .for_each(|(_name, scalar)| diff_scalar(logger, scalar, color));
    lists
        .iter()
        .for_each(|(_name, list)| diff_list(logger, list, "", color));
    0
}

fn diff_list(logger: &mut dyn Logger, list: &undo::List, current: &str, color: bool) {
    let formatted_deletions: Vec<String> = if color {
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
    diff_remove(logger, &list.name, &(prefix + &suffix), color);

    let items = current.split(':').map(|x| {
        if list.additions.contains(&x.to_string()) && color {
            "\x1b[48;5;22m".to_string() + x + "\x1b[0;92m"
        } else {
            x.to_string()
        }
    });
    let items: Vec<String> = items.collect();
    let newline = items.join(":");
    diff_add(logger, &list.name, &newline, color);
}

fn diff_scalar(logger: &mut dyn Logger, scalar: &undo::Scalar, color: bool) {
    if let Some(value) = &scalar.original {
        diff_remove(logger, &scalar.name, value, color);
    }
    if let Some(value) = &scalar.current {
        diff_add(logger, &scalar.name, value, color);
    }
}

fn diff_add(logger: &mut dyn Logger, name: &str, value: &str, color: bool) {
    if color {
        // Clearing to EOL with \x1b[K prevents a weird issue where a wrapped line uses the last
        // non-null background color for the newline character, filling the rest of the space in the
        // line.
        logger.print(format!("\x1b[92m+ {}={}\x1b[0m\x1b[K", name, value));
    } else {
        logger.print(format!("+ {}={}", name, value));
    }
}

fn diff_remove(logger: &mut dyn Logger, name: &str, value: &str, color: bool) {
    if color {
        // Clearing to EOL with \x1b[K prevents a weird issue where a wrapped line uses the last
        // non-null background colour for the newline character, filling the rest of the space in the
        // line.
        logger.print(format!("\x1b[91m- {}={}\x1b[0m\x1b[K", name, value));
    } else {
        logger.print(format!("- {}={}", name, value));
    }
}

fn print_verbose(logger: &mut dyn Logger, name: &str, value: &str) {
    logger.print(format!("  {}={}", name, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct DummyLogger(Vec<String>);
    impl Logger for DummyLogger {
        fn print(&mut self, value: String) {
            self.0.push(value);
        }
    }

    #[test]
    fn test_nominal() {
        let mut logger = DummyLogger::default();

        let env_vars = vec![
            ("VAR_A".to_string(), "/added:/existent".to_string()),
            ("VAR_B".to_string(), "/added".to_string()),
            ("VAR_C".to_string(), "/added:/existent".to_string()),
        ];

        let data = r#"62b0b9f86cda84d4:{"scalars":[],"lists":[{"name":"VAR_C","additions":["/added"],"deletions":["/removed"]},{"name":"VAR_B","additions":["/added"],"deletions":[]},{"name":"VAR_A","additions":["/added"],"deletions":[]}]}"#;
        let result = run_with_logger(&mut logger, env_vars, false, false, data.to_string());

        let expected: Vec<_> = [
            "- VAR_A=/existent",
            "+ VAR_A=/added:/existent",
            "- VAR_B=",
            "+ VAR_B=/added",
            "- VAR_C=/removed:/existent",
            "+ VAR_C=/added:/existent",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(result, 0);
        assert_eq!(logger.0, expected);
    }
}
