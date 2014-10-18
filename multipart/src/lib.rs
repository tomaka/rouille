#![feature(if_let, slicing_syntax, struct_variant, default_type_params)]
extern crate hyper;
extern crate mime;
extern crate serialize;

pub mod client;
pub mod server;

#[cfg(test)]
mod test {
   
    #[test]
    fn client_api_test() {
       
    }
       
}
