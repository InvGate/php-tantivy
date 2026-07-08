// ext-php-rs uses the `vectorcall` calling convention to call back into PHP internals on Windows.
// On recent nightlies that ABI is feature-gated, so the consuming crate must opt in. Windows is
// built with nightly (see the Windows CI job); Linux/macOS build on stable and never see this
// attribute (cfg(windows) is false there), so the stable build is unaffected.
#![cfg_attr(windows, feature(abi_vectorcall))]

use ext_php_rs::prelude::*;
use ext_php_rs::exception::PhpException;
use tantivy_core::schema::IndexConfig;
use tantivy_core::{query, registry, writer};

/// Ejecuta una operación del core dentro de un `catch_unwind` y traduce el resultado a `PhpResult`:
/// un `Err(String)` se lanza como excepción de PHP, y un PANIC del core (p. ej. un assert interno de
/// tantivy ante entrada inesperada) se atrapa acá y también se convierte en excepción. Esto es
/// CRÍTICO: las funciones que ext-php-rs genera para llamar a estos métodos son `extern "C"` y NO
/// pueden desenrollar; un panic que llegara a ese frame abortaría el proceso entero ("panic in a
/// function that cannot unwind"), matando al worker PHP-FPM. Atrapándolo antes, un request tóxico
/// falla con una excepción en vez de tumbar el worker. `AssertUnwindSafe` es correcto porque el
/// único estado compartido es la tabla global del registro, cuyo lock ya se recupera de
/// envenenamiento (ver registry::lock_table).
fn guard<T>(f: impl FnOnce() -> Result<T, String>) -> PhpResult<T> {
    catch_core(f).map_err(PhpException::default)
}

/// Núcleo de `guard` sin dependencia de PHP: corre `f` bajo `catch_unwind` y baja un panic a
/// `Err(String)`. Separado para poder testearlo con `cargo test` (el binario de test no linkea los
/// símbolos de PHP, así que no puede construir `PhpException`).
fn catch_core<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => Err(format!("panic en el núcleo tantivy: {}", panic_detail(&payload))),
    }
}

/// Extrae el mensaje de un payload de panic (`&str` o `String`); si no, un genérico.
fn panic_detail(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "causa desconocida".to_string()
    }
}

fn parse_config(config_json: &str) -> Result<IndexConfig, String> {
    serde_json::from_str(config_json).map_err(|e| format!("config JSON inválido: {e}"))
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
        let handle = guard(|| registry::open_or_create(parse_config(&config_json)?))?;
        Ok(Self { handle })
    }

    #[php(name = "openReadOnly")]
    pub fn open_read_only(config_json: String) -> PhpResult<Self> {
        let handle = guard(|| registry::open_read_only(parse_config(&config_json)?))?;
        Ok(Self { handle })
    }

    #[php(name = "addDocument")]
    pub fn add_document(&self, doc_json: String) -> PhpResult<()> {
        guard(|| registry::with_state(self.handle, |s| writer::add_document(s, &doc_json)))
    }

    #[php(name = "updateDocument")]
    pub fn update_document(&self, key_field: String, key_value: String, doc_json: String) -> PhpResult<()> {
        guard(|| {
            registry::with_state(self.handle, |s| {
                writer::update_document(s, &key_field, &key_value, &doc_json)
            })
        })
    }

    #[php(name = "deleteDocument")]
    pub fn delete_document(&self, key_field: String, key_value: String) -> PhpResult<()> {
        guard(|| {
            registry::with_state(self.handle, |s| writer::delete_by_id(s, &key_field, &key_value))
        })
    }

    #[php(name = "commit")]
    pub fn commit(&self) -> PhpResult<()> {
        guard(|| registry::with_state(self.handle, writer::commit))
    }

    #[php(name = "optimize")]
    pub fn optimize(&self) -> PhpResult<()> {
        // v1: no-op (el merge se agenda en el plan de rebuild). Nunca falla.
        Ok(())
    }

    #[php(name = "docCount")]
    pub fn doc_count(&self) -> PhpResult<i64> {
        guard(|| registry::with_state(self.handle, |s| s.doc_count().map(|n| n as i64)))
    }

    #[php(name = "search")]
    pub fn search(&self, query_json: String) -> PhpResult<String> {
        guard(|| registry::with_state(self.handle, |s| query::search(s, &query_json)))
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
    fn catch_core_converts_a_panic_into_an_error_instead_of_aborting() {
        // En el borde real de la extensión, un panic que cruza el frame extern "C" aborta el worker
        // ("panic in a function that cannot unwind"). catch_core lo atrapa ANTES de llegar a ese
        // frame y lo baja a Err (que guard lanza como excepción de PHP). Silenciamos el hook para no
        // ensuciar la salida con el backtrace esperado.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = catch_core(|| -> Result<i64, String> { panic!("boom del core") });
        std::panic::set_hook(prev);
        assert!(result.is_err(), "el panic debía convertirse en Err, no propagarse/abortar");
        assert!(result.unwrap_err().contains("panic en el núcleo"));
    }

    #[test]
    fn catch_core_passes_through_ok_and_err_unchanged() {
        assert_eq!(catch_core(|| -> Result<i64, String> { Ok(7) }).unwrap(), 7);
        assert!(catch_core(|| -> Result<i64, String> { Err("nope".into()) }).is_err());
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
