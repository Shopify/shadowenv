use std::io;
use std::process::Command;
use failure::Error;
use regex::Regex;


arg_enum!{
    #[derive(Debug)]
    #[allow(non_camel_case_types)]
    pub enum Types {
        dotenv,
        direnv,
        nix_shell_hook,
        ruby_version,
        nvmrc,
    }
}

pub fn run(mode: Option<Types>) -> Result<(), Error> {
    match mode {
        Some(typ) => convert_type(typ),
        None      => automatic(),
    }
}

fn convert_type(typ: Types) -> Result<(), Error> {
    match &typ {
        Types::dotenv         => convert_dotenv(),
        Types::direnv         => convert_direnv(),
        Types::nix_shell_hook => not_implemented(typ),
        Types::ruby_version   => not_implemented(typ),
        Types::nvmrc          => not_implemented(typ),
    }
}

fn automatic() -> Result<(), Error> {
    return Err(format_err!("automatic assimilation not yet implemented"))
}

fn not_implemented(typ: Types) -> Result<(), Error> {
    return Err(format_err!("assimilation type not yet implemented: {:?}", typ))
}

fn convert_direnv() -> Result<(), Error> {
    let output = Command::new("direnv")
        .arg("export")
        .arg("json")
        .output().unwrap_or_else(|e| {
            panic!("failed to execute process: {}", e)
    });

    if !output.status.success() {
        println!("direnv failed");
    }

    let s = String::from_utf8_lossy(&output.stdout);
    let json: std::collections::HashMap<String, String> =
        serde_json::from_str(&s).expect("JSON was not well-formatted");

    for (key, value) in json {
        if key.starts_with("DIRENV_") {
            continue;
        }
        println!("(env/set \"{}\" \"{}\")", key, value);
    }

    Ok(())
}

fn convert_dotenv() -> Result<(), Error> {
    let ignore = Regex::new("^\\s*($|#)").unwrap();

    let mut line = String::new();
    while io::stdin().read_line(&mut line).unwrap() > 0 {
        if ignore.is_match(&line) {
            line.clear();
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, "=").collect();
        if parts.len() != 2 {
            panic!("invalid line in dotenv: {:?}", line);
        }
        let key = parts[0].trim();
        let mut value = parts[1].trim();
        if value.starts_with("'") && value.ends_with("'") {
            value = &value[1..value.len() - 1];
        }
        if value.starts_with("\"") && value.ends_with("\"") {
            println!("(env/set \"{}\" {})", key, value);
        } else {
            println!("(env/set \"{}\" {:?})", key, value);
        }
        line.clear();
    }
    Ok(())
}
