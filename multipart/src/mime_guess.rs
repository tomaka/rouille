use mime::{Mime, TopLevel, SubLevel};

use serialize::json;

use std::collections::HashMap;

/// Guess the MIME type of the `Path` by its extension.
///
/// **Guess** is the operative word here, as the contents of a file
/// may not or may not match its MIME type/extension.
pub fn guess_mime_type(path: &Path) -> Mime {
    let ext = path.extension_str().unwrap_or("");
    
    get_mime_type(ext)
}

/// Extract the extensioin of `filename` and guess its MIME type.
/// If there is no extension, or the extension has no known MIME association,
/// `applicaton/octet-stream` is assumed.
pub fn guess_mime_type_filename(filename: &str) -> Mime {
    let path = Path::new(filename);
    
    guess_mime_type(&path)    
}

const MIME_TYPES: &'static str = include_str!("../mime_types.json");

// Lazily initialized task-local hashmap
// TODO: Make this global since it's read-only
thread_local!(static MIMES: HashMap<String, Mime> = load_mime_types())

/// Get the MIME type associated with a file extension
/// If there is no association for the extension, or `ext` is empty,
/// `application/octet-stream` is returned.
pub fn get_mime_type(ext: &str) -> Mime {
    if ext.is_empty() { return octet_stream(); }
 
    MIMES.with(|cache| cache.get(ext).cloned()).unwrap_or_else(octet_stream) 
}

/// Load the known mime types from the MIME_TYPES json 
fn load_mime_types() -> HashMap<String, Mime> {
    let map = if let json::Object(map) = json::from_str(MIME_TYPES).unwrap() { map }
    else { unreachable!("MIME types should be supplied as a map!"); };
    
    map.into_iter().filter_map(to_mime_mapping).collect()
}

fn to_mime_mapping(val: (String, json::Json)) -> Option<(String, Mime)> {
    if let (st, json::String(mime)) = val {
        if st.char_at(0) == '_' { return None; }

        if let Some(mime) = from_str::<Mime>(&*mime) { 
            return Some((st, mime))
        }
    }
    
    None    
}

/// Get the `Mime` type for `application/octet-stream` (generic binary stream)
pub fn octet_stream() -> Mime {
    Mime(TopLevel::Application, SubLevel::Ext("octet-stream".into_string()), Vec::new())   
}

#[test]
fn test_mime_type_guessing() {
    assert!(get_mime_type("gif").to_string() == "image/gif".to_string());
    assert!(get_mime_type("txt").to_string() == "text/plain".to_string());
    assert!(get_mime_type("blahblah").to_string() == "application/octet-stream".to_string());     
}


