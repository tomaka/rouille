use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use super::ServiceAccess;
use super::StaticServices;

use rustc_serialize::Encodable;
use mustache;
use mustache::Template as MustacheTemplate;

pub struct TemplatesCache {
    path: Option<PathBuf>,
    cache: Mutex<HashMap<String, Arc<MustacheTemplate>>>,
}

impl Default for TemplatesCache {
    fn default() -> TemplatesCache {
        TemplatesCache {
            path: None,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl TemplatesCache {
    pub fn new<P>(templates_path: P) -> TemplatesCache where P: Into<PathBuf> {
        TemplatesCache {
            path: Some(templates_path.into()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    // TODO: proper error
    pub fn render<W, E>(&self, name: &str, mut output: W, data: &E) -> Result<(), ()>
                        where W: Write, E: Encodable
    {
        let entry = {
            let mut cache = self.cache.lock().unwrap();
            cache.entry(name.to_string()).or_insert_with(|| {
                let template = mustache::compile_str("hello world");        // FIXME: 
                Arc::new(template)
            }).clone()
        };

        entry.render(&mut output, data).map_err(|e| { println!("Error while rendering template: {:?}", e); })       // TODO: 
    }
}
