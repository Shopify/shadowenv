use std::collections::HashMap;
use std::env;

struct TestCase {
    initial_env: Vec<String>,
    program: Option<String>,
}

impl TestCase {
    fn run(&self) {
        with_initial_env(&self.initial_env, || {
            assert_eq!(6, 7);
        });
    }
}

#[test]
fn test_a() {
    TestCase {
        initial_env: vec!["A=b".to_string()],
    }
    .run()
}

fn with_initial_env<F>(vars: &Vec<String>, f: F)
where
    F: Fn(),
{
    let mut prev: HashMap<String, String> = HashMap::new();

    for (var, val) in std::env::vars() {
        prev.insert(var.to_string(), val.to_string());
        std::env::remove_var(var);
    }

    for var in vars {
        let mut parts = var.splitn(2, "=");
        let name = parts.next().unwrap();
        let val = parts.next().unwrap();
        std::env::set_var(name, val);
    }

    // note: we don't correctly restore on panic or assertion failure.
    f();

    env::vars().for_each(|(var, _)| env::remove_var(var));
    prev.iter().for_each(|(var, val)| env::set_var(var, val));
}
