use std::io;
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
        Types::direnv         => not_implemented(typ),
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


fn convert_dotenv() -> Result<(), Error> {
    let ignore = Regex::new("^\\s*($|#)").unwrap();

    let mut line = String::new();
    while io::stdin().read_line(&mut line).unwrap() > 0 {
        if ignore.is_match(&line) {
            line.clear();
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, "=").collect();
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
