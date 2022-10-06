use crate::undo;

use std::collections::BTreeMap;
use std::env;

trait Logger {
    fn print(&mut self, value: String);
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
            diff_scalar(logger, &scalar, color)
        } else if let Some(list) = lists.remove(&name) {
            diff_list(logger, &list, &value, color)
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
    fn nominal_test() {
        let mut logger = DummyLogger::default();

        let env_vars = vec![(
            "MANPATH".to_string(),
            "/opt/homebrew/share/man:/opt/homebrew/share/man".to_string(),
        )];

        let data = r#"8a3f0f24bc3fd12d:{"scalars":[{"name":"REDIS_URL","original":null,"current":"redis://web.railgun:6379/0"},{"name":"CPPFLAGS","original":null,"current":"-DPNG_ARM_NEON_OPT=0"},{"name":"NVM_BIN","original":null,"current":"/Users/xade/.nvm/versions/node/v16.14.2/bin"},{"name":"NVM_PATH","original":null,"current":"/Users/xade/.nvm/versions/node/v16.14.2/lib/node"},{"name":"NVM_DIR","original":null,"current":"/Users/xade/.nvm"},{"name":"NGINX_HOST","original":null,"current":"web.railgun"},{"name":"HOST_BIND_IP","original":null,"current":"192.168.64.1"},{"name":"REDIS_PORT","original":null,"current":"6379"},{"name":"REDIS_HOST","original":null,"current":"web.railgun"},{"name":"NGINX_PORT","original":null,"current":"80"},{"name":"HOST_WEBPACK_IP","original":null,"current":"192.168.64.254"}],"lists":[{"name":"MANPATH","additions":["/Users/xade/.nvm/versions/node/v16.14.2/share/man"],"deletions":[]},{"name":"PATH","additions":["/opt/homebrew/Caskroom/google-cloud-sdk/latest/google-cloud-sdk/bin","/Users/xade/.dev/yarn/1.22.15/bin","/Users/xade/.nvm/versions/node/v16.14.2/bin","/opt/homebrew/opt/python@3.10/bin"],"deletions":[]},{"name":"PKG_CONFIG_PATH","additions":["/opt/homebrew/lib/pkgconfig","/opt/homebrew/opt/zstd/lib/pkgconfig","/opt/homebrew/opt/xz/lib/pkgconfig","/opt/homebrew/opt/sqlite/lib/pkgconfig","/opt/homebrew/opt/readline/lib/pkgconfig","/opt/homebrew/opt/python@3.10/lib/pkgconfig","/opt/homebrew/opt/pcre2/lib/pkgconfig","/opt/homebrew/opt/openssl@1.1/lib/pkgconfig","/opt/homebrew/opt/lz4/lib/pkgconfig","/opt/homebrew/opt/libsodium/lib/pkgconfig","/opt/homebrew/opt/libevent/lib/pkgconfig","/opt/homebrew/opt/icu4c/lib/pkgconfig","/opt/homebrew/opt/glog/lib/pkgconfig","/opt/homebrew/opt/gflags/lib/pkgconfig","/opt/homebrew/opt/folly/lib/pkgconfig","/opt/homebrew/opt/fmt/lib/pkgconfig","/usr/lib/pkgconfig"],"deletions":[]}]}"#;
        let result = run_with_logger(&mut logger, env_vars, true, false, data.to_string());

        let expected: Vec<_> = vec![
            "- MANPATH=/opt/homebrew/share/man:/opt/homebrew/share/man",
            "+ MANPATH=/opt/homebrew/share/man:/opt/homebrew/share/man",
            "+ CPPFLAGS=-DPNG_ARM_NEON_OPT=0",
            "+ HOST_BIND_IP=192.168.64.1",
            "+ HOST_WEBPACK_IP=192.168.64.254",
            "+ NGINX_HOST=web.railgun",
            "+ NGINX_PORT=80",
            "+ NVM_BIN=/Users/xade/.nvm/versions/node/v16.14.2/bin",
            "+ NVM_DIR=/Users/xade/.nvm",
            "+ NVM_PATH=/Users/xade/.nvm/versions/node/v16.14.2/lib/node",
            "+ REDIS_HOST=web.railgun",
            "+ REDIS_PORT=6379",
            "+ REDIS_URL=redis://web.railgun:6379/0",
            "- PATH=",
            "+ PATH=",
            "- PKG_CONFIG_PATH=",
            "+ PKG_CONFIG_PATH=",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(result, 0);
        assert_eq!(logger.0, expected);
    }
}
