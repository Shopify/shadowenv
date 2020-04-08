use crate::undo;

use std::collections::HashMap;
use std::env;

/// print a diff of the env
pub fn run(verbose: bool, color: bool, shadowenv_data: String) -> i32 {
    let mut parts = shadowenv_data.splitn(2, ":");
    let _prev_hash = parts.next();
    let json_data = parts.next().unwrap_or("{}");
    let shadowenv_data = undo::Data::from_str(json_data).unwrap();
    let mut scalars = HashMap::new();
    for scalar in shadowenv_data.scalars {
        scalars.insert(scalar.name.clone(), scalar);
    }
    let mut lists = HashMap::new();
    for list in shadowenv_data.lists {
        lists.insert(list.name.clone(), list);
    }
    for (name, value) in env::vars() {
        if let Some(scalar) = scalars.remove(&name) {
            diff_scalar(&scalar, color)
        } else if let Some(list) = lists.remove(&name) {
            diff_list(&list, &value, color)
        } else if verbose {
            print_verbose(&name, &value)
        }
    }
    for (_name, scalar) in &scalars {
        diff_scalar(&scalar, color)
    }
    for (_name, list) in &lists {
        diff_list(&list, &"".to_string(), color)
    }
    0
}

fn diff_list(list: &undo::List, current: &str, color: bool) {
    let formatted_deletions: Vec<String> = if color {
        list.deletions
            .iter()
            .map(|x| "\x1b[48;5;52m".to_string() + &x + &"\x1b[0;91m".to_string())
            .collect()
    } else {
        list.deletions.clone()
    };
    let mut prefix = formatted_deletions.join(":");
    let items = current.split(":").collect::<Vec<&str>>();
    let items = items
        .into_iter()
        .skip_while(|x| list.additions.contains(&x.to_string()));
    let items: Vec<&str> = items.collect();
    let suffix = items.join(":");
    if suffix != "" && prefix != "" {
        prefix += ":";
    }
    diff_remove(&list.name, &(prefix + &suffix), color);

    let items = current.split(":").collect::<Vec<&str>>();
    let items = items.into_iter().map(|x| {
        if list.additions.contains(&x.to_string()) && color {
            "\x1b[48;5;22m".to_string() + &x + &("\x1b[0;92m".to_string())
        } else {
            x.to_string()
        }
    });
    let items: Vec<String> = items.collect();
    let newline = items.join(":");
    diff_add(&list.name, &newline, color);
}

fn diff_scalar(scalar: &undo::Scalar, color: bool) {
    if let Some(value) = &scalar.original {
        diff_remove(&scalar.name, &value, color);
    }
    if let Some(value) = &scalar.current {
        diff_add(&scalar.name, &value, color);
    }
}

fn diff_add(name: &str, value: &str, color: bool) {
    if color {
        // Clearing to EOL with \x1b[K prevents a weird issue where a wrapped line uses the last
        // non-null background color for the newline character, filling the rest of the space in the
        // line.
        println!("\x1b[92m+ {}={}\x1b[0m\x1b[K", name, value);
    } else {
        println!("+ {}={}", name, value);
    }
}

fn diff_remove(name: &str, value: &str, color: bool) {
    if color {
        // Clearing to EOL with \x1b[K prevents a weird issue where a wrapped line uses the last
        // non-null background colour for the newline character, filling the rest of the space in the
        // line.
        println!("\x1b[91m- {}={}\x1b[0m\x1b[K", name, value);
    } else {
        println!("- {}={}", name, value);
    }
}

fn print_verbose(name: &str, value: &str) {
    println!("  {}={}", name, value)
}
