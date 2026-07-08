use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tantivy::schema::Schema;
use tantivy::{Index, IndexReader, ReloadPolicy};

use crate::schema::{build_schema, IndexConfig};

pub struct IndexState {
    pub index: Index,
    pub schema: Schema,
    pub reader: IndexReader,
    pub writer_heap_bytes: usize,
    pub writer: Option<tantivy::IndexWriter>,
}

impl IndexState {
    pub fn doc_count(&self) -> Result<u64, String> {
        let searcher = self.reader.searcher();
        Ok(searcher.num_docs())
    }
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn table() -> &'static Mutex<HashMap<u64, IndexState>> {
    static TABLE: std::sync::OnceLock<Mutex<HashMap<u64, IndexState>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Abre un índice existente o lo crea en `cfg.path`. Devuelve un handle opaco.
pub fn open_or_create(cfg: IndexConfig) -> Result<u64, String> {
    std::fs::create_dir_all(&cfg.path).map_err(|e| format!("no se pudo crear el dir: {e}"))?;
    let schema = build_schema(&cfg.fields);

    let dir = tantivy::directory::MmapDirectory::open(&cfg.path)
        .map_err(|e| format!("no se pudo abrir el dir: {e}"))?;
    let index = Index::builder()
        .schema(schema.clone())
        .open_or_create(dir)
        .map_err(|e| format!("no se pudo abrir/crear el índice: {e}"))?;

    register_state(index, &cfg.id_field, cfg.writer_heap_bytes)
}

/// Abre un índice EXISTENTE en modo solo-lectura (para búsquedas). Falla si no existe.
/// No crea el directorio ni el índice, y no abre writer: una búsqueda nunca debe crear
/// ni mutar el índice (así no requiere permisos de escritura, y un índice no construido
/// falla de forma explícita en vez de crear uno vacío que devolvería "0 resultados").
pub fn open_read_only(cfg: IndexConfig) -> Result<u64, String> {
    let index = Index::open_in_dir(&cfg.path)
        .map_err(|e| format!("no se pudo abrir el índice (solo-lectura) en '{}': {e}", cfg.path))?;
    register_state(index, &cfg.id_field, cfg.writer_heap_bytes)
}

/// Construye el reader, valida el id_field contra el schema en disco, registra el estado
/// en la tabla y devuelve el handle opaco. Compartido por open_or_create y open_read_only.
fn register_state(index: Index, id_field: &str, writer_heap_bytes: usize) -> Result<u64, String> {
    // El schema/id_field se resuelven desde el índice efectivamente en disco.
    let schema = index.schema();
    schema
        .get_field(id_field)
        .map_err(|_| format!("id_field '{}' no existe en el schema", id_field))?;

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()
        .map_err(|e| format!("no se pudo crear el reader: {e}"))?;

    let state = IndexState {
        index,
        schema,
        reader,
        writer_heap_bytes,
        writer: None,
    };

    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    table().lock().unwrap().insert(handle, state);
    Ok(handle)
}

/// Ejecuta `f` con acceso mutable al estado del handle.
pub fn with_state<T>(
    handle: u64,
    f: impl FnOnce(&mut IndexState) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = table().lock().unwrap();
    let state = guard
        .get_mut(&handle)
        .ok_or_else(|| format!("handle inválido: {handle}"))?;
    f(state)
}

/// Cierra y descarta el índice. Devuelve true si existía.
pub fn close(handle: u64) -> bool {
    table().lock().unwrap().remove(&handle).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FieldsDescriptor, IndexConfig};

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
    fn opens_creates_and_counts_zero() {
        let dir = std::env::temp_dir().join(format!("tv_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        let n = with_state(h, |s| s.doc_count()).unwrap();
        assert_eq!(n, 0);
        assert!(close(h));
        assert!(!close(h));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reopens_existing_index_consistently() {
        let dir = std::env::temp_dir().join(format!("tv_test_{}_reopen", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let h1 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        assert!(close(h1));

        let h2 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        let n = with_state(h2, |s| s.doc_count()).unwrap();
        assert_eq!(n, 0);
        assert!(close(h2));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_incompatible_schema_on_reopen() {
        let dir =
            std::env::temp_dir().join(format!("tv_test_{}_incompatible", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let h1 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        assert!(close(h1));

        let mismatched_cfg = IndexConfig {
            path: dir.to_str().unwrap().to_string(),
            id_field: "id_key".into(),
            fields: FieldsDescriptor {
                text: vec!["title".into(), "extra".into()],
                keys: vec!["id_key".into()],
                attributes: vec![],
            },
            writer_heap_bytes: 15_000_000,
        };
        let result = open_or_create(mismatched_cfg);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_read_only_fails_on_missing_index_and_opens_existing() {
        let dir = std::env::temp_dir().join(format!("tv_test_{}_readonly", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // índice inexistente: read-only NO debe crearlo, debe fallar explícitamente.
        let missing = open_read_only(cfg(dir.to_str().unwrap()));
        assert!(missing.is_err());
        assert!(!dir.exists(), "read-only no debe crear el directorio del índice");

        // creamos el índice con open_or_create, luego lo abrimos read-only.
        let h1 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        assert!(close(h1));

        let h2 = open_read_only(cfg(dir.to_str().unwrap())).unwrap();
        assert_eq!(with_state(h2, |s| s.doc_count()).unwrap(), 0);
        assert!(close(h2));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
