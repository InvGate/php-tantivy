// ext-php-rs uses the `vectorcall` calling convention to call back into PHP internals on Windows.
// On recent nightlies that ABI is feature-gated, so the consuming crate must opt in. Windows is
// built with nightly (see the Windows CI job); Linux/macOS build on stable and never see this
// attribute (cfg(windows) is false there), so the stable build is unaffected.
#![cfg_attr(windows, feature(abi_vectorcall))]

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

/// RAII: cuando PHP libera el objeto `Tantivy\Native\Index` (refcount a 0 / GC), ext-php-rs dropea
/// este struct y liberamos el estado del registro. Sin esto, cada open sin un close() explícito fuga
/// un IndexState (reader mmap + FDs de segmentos, y hasta el heap del writer) por toda la vida del
/// proceso — crítico en un worker PHP-FPM de larga vida que abre un índice por request. close() es
/// idempotente, así que un close() manual previo más este Drop no se pisan.
impl Drop for Index {
    fn drop(&mut self) {
        registry::close(self.handle);
    }
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
        // v1: no-op (el merge se agenda en el plan de rebuild). Nunca falla.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy_core::schema::{FieldsDescriptor, IndexConfig};

    fn cfg(path: &str) -> IndexConfig {
        IndexConfig {
            path: path.to_string(),
            id_field: "id_key".into(),
            fields: FieldsDescriptor {
                text: vec!["title".into()],
                keys: vec!["id_key".into()],
                attributes: vec![],
            },
            writer_heap_bytes: 15_000_000,
        }
    }

    #[test]
    fn dropping_index_releases_the_registry_handle() {
        let dir = std::env::temp_dir().join(format!("tv_ext_drop_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let handle = registry::open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        let index = Index { handle };
        drop(index);

        // Si Drop cerró el handle, close() acá devuelve false (ya no está). Sin Drop, el estado
        // seguiría en el registro (fuga) y close() devolvería true.
        assert!(
            !registry::close(handle),
            "Drop debía liberar el handle del registro"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
