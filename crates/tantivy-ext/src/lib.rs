use ext_php_rs::prelude::*;
use ext_php_rs::exception::PhpException;
use tantivy_core::schema::IndexConfig;
use tantivy_core::{query, registry, writer};

/// Convierte un Result del core en un PhpResult: el Err se lanza como excepción de PHP.
/// `ExtClient` (userland) la atrapa y re-lanza `Tantivy\TantivyException`, así el tipo de
/// excepción es el mismo para ambos backends.
fn php<T>(r: Result<T, String>) -> PhpResult<T> {
    r.map_err(PhpException::default)
}

fn parse_config(config_json: &str) -> PhpResult<IndexConfig> {
    serde_json::from_str(config_json)
        .map_err(|e| PhpException::default(format!("config JSON inválido: {e}")))
}

#[php_class]
#[php(name = "Tantivy\\Native\\Index")]
pub struct Index {
    handle: u64,
}

#[php_impl]
impl Index {
    #[php(name = "openOrCreate")]
    pub fn open_or_create(config_json: String) -> PhpResult<Self> {
        let cfg = parse_config(&config_json)?;
        let handle = php(registry::open_or_create(cfg))?;
        Ok(Self { handle })
    }

    #[php(name = "openReadOnly")]
    pub fn open_read_only(config_json: String) -> PhpResult<Self> {
        let cfg = parse_config(&config_json)?;
        let handle = php(registry::open_read_only(cfg))?;
        Ok(Self { handle })
    }

    #[php(name = "addDocument")]
    pub fn add_document(&self, doc_json: String) -> PhpResult<()> {
        php(registry::with_state(self.handle, |s| writer::add_document(s, &doc_json)))
    }

    #[php(name = "updateDocument")]
    pub fn update_document(&self, key_field: String, key_value: String, doc_json: String) -> PhpResult<()> {
        php(registry::with_state(self.handle, |s| {
            writer::update_document(s, &key_field, &key_value, &doc_json)
        }))
    }

    #[php(name = "deleteDocument")]
    pub fn delete_document(&self, key_field: String, key_value: String) -> PhpResult<()> {
        php(registry::with_state(self.handle, |s| {
            writer::delete_by_id(s, &key_field, &key_value)
        }))
    }

    #[php(name = "commit")]
    pub fn commit(&self) -> PhpResult<()> {
        php(registry::with_state(self.handle, writer::commit))
    }

    #[php(name = "optimize")]
    pub fn optimize(&self) -> PhpResult<()> {
        // v1: no-op, igual que el path FFI (tv_optimize).
        Ok(())
    }

    #[php(name = "docCount")]
    pub fn doc_count(&self) -> PhpResult<i64> {
        let n = php(registry::with_state(self.handle, |s| s.doc_count()))?;
        Ok(n as i64)
    }

    #[php(name = "search")]
    pub fn search(&self, query_json: String) -> PhpResult<String> {
        php(registry::with_state(self.handle, |s| query::search(s, &query_json)))
    }

    #[php(name = "close")]
    pub fn close(&self) -> PhpResult<()> {
        registry::close(self.handle);
        Ok(())
    }
}

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    module.class::<Index>()
}
