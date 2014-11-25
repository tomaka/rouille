use mime::{Mime, TopLevel, SubLevel};

use serialize::json;

use std::cell::RefCell;

use std::collections::HashMap;


/// Guess the MIME type of the `Path` by its extension.
///
/// **Guess** is the operative word here, as the contents of a file
/// may not or may not match its MIME type/extension.
pub fn guess_mime_type(path: &Path) -> Mime {
    let ext = path.extension_str().unwrap_or("");
    
    get_mime_type(ext)
}

pub fn guess_mime_type_filename(filename: &str) -> Mime {
    let path = Path::new(filename);
    
    guess_mime_type(&path)    
}

local_data_key!(mime_types_cache: RefCell<HashMap<String, Mime>>)

/// Get the MIME type associated with a file extension
/// If there is no association for the extension,
/// `application/octet-stream` is assumed.
pub fn get_mime_type(ext: &str) -> Mime {
    if ext.is_empty() { return octet_stream(); }

    let ext = ext.into_string();
   
    // MIME Types are cached in a task-local heap
    let cache = if let Some(cache) = mime_types_cache.get() { cache }
    else {
        mime_types_cache.replace(Some(RefCell::new(HashMap::new())));
        mime_types_cache.get().unwrap()   
    };

    if let Some(mime_type) = cache.borrow().get(&ext) {
        return mime_type.clone();   
    }

    let mime_type = find_mime_type(&*ext);

    cache.borrow_mut().insert(ext, mime_type.clone());

    mime_type  
}

const MIME_TYPES: &'static str = include_str!("../mime_types.json");

/// Load the MIME_TYPES as JSON and try to locate `ext`
fn find_mime_type(ext: &str) -> Mime {
    json::from_str(MIME_TYPES).unwrap()
        .find(ext).and_then(|j| j.as_string())
        .and_then(from_str::<Mime>)
        .unwrap_or_else(octet_stream)
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


