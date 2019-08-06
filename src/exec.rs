use failure::Error;
use std::vec::Vec;

#[derive(Fail, Debug)]
#[fail(display = "exec is not implemented")]
pub struct NotImplemented;

pub fn run(data: Option<&str>, argv: Vec<&str>) -> Result<(), Error> {
    eprintln!("data: {:?}; argv: {:?}", data, argv);
    Err(NotImplemented.into())
}
