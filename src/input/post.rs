use rustc_serialize::Decoder;
use rustc_serialize::Decodable;

use Request;
use RouteError;

pub enum PostError {

}

impl From<PostError> for RouteError {
    fn from(err: PostError) -> RouteError {
        RouteError::WrongInput
    }
}

pub fn get_post_input<T>(request: &Request) -> Result<T, PostError> where T: Decodable {
    let decoder = PostDecoder::Start(request);

    unimplemented!();
}

enum PostDecoder<'a> {
    Start(&'a Request),

    ExpectsStructMember(&'a Request),


}
