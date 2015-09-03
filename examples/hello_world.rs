#[macro_use]
extern crate rouille;

fn main() {

    let server = rouille::Server::start();

    for request in server {
        router!(request,
            GET (/) => (|| {
                println!("test qsdf");
            }),

            GET (/{id}) => (|id: u32| {
                println!("u32 {:?}", id);
            }),

            GET (/{id}) => (|id: String| {
                println!("String {:?}", id);
            })
        );

    }
}
