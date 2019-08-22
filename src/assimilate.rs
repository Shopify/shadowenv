use failure::Error;

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
        Types::dotenv         => not_implemented(typ),
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
