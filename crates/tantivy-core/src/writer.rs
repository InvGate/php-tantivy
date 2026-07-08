use tantivy::{TantivyDocument, TantivyError, Term};

use crate::registry::IndexState;

/// Prefijo estable y neutro (no depende del idioma del mensaje) que marca el caso "el writer lock
/// exclusivo del índice está tomado por otro proceso" (p. ej. un rebuild en curso). Los bindings PHP
/// lo mapean a `Tantivy\IndexBusyException` para que los consumidores chequeen el TIPO, no el texto.
/// Es un contrato: si se cambia acá, actualizar el cliente (ExtClient / TantivyException::forOperation).
pub const WRITER_LOCKED_PREFIX: &str = "index_locked:";

/// Obtiene (o crea) el IndexWriter cacheado del estado. tantivy permite un solo writer por
/// directorio; si el lock está tomado, `writer()` devuelve `LockFailure` — lo marcamos con
/// WRITER_LOCKED_PREFIX en vez de tratarlo como un error genérico.
fn ensure_writer(state: &mut IndexState) -> Result<&mut tantivy::IndexWriter, String> {
    if state.writer.is_none() {
        let w = match state.index.writer(state.writer_heap_bytes) {
            Ok(w) => w,
            Err(TantivyError::LockFailure(_, _)) => {
                return Err(format!(
                    "{WRITER_LOCKED_PREFIX} writer lock ocupado (¿rebuild en curso?)"
                ));
            }
            Err(e) => return Err(format!("no se pudo abrir el writer: {e}")),
        };
        state.writer = Some(w);
    }
    Ok(state.writer.as_mut().unwrap())
}

/// Parsea el doc JSON (objeto plano campo->string) a un TantivyDocument segun el schema.
fn parse_doc(state: &IndexState, doc_json: &str) -> Result<TantivyDocument, String> {
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(doc_json).map_err(|e| format!("doc JSON inválido: {e}"))?;
    let mut doc = TantivyDocument::default();
    for (name, value) in map {
        let field = match state.schema.get_field(&name) {
            Ok(f) => f,
            Err(_) => continue, // campo desconocido -> ignorar
        };
        let text = match value {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        doc.add_text(field, &text);
    }
    Ok(doc)
}

// NOTA (NRT): add/update/delete NUNCA commitean. Una escritura sólo la aplica al writer
// (en su buffer en memoria); recién `commit()` la hace durable y visible al reader. El commit
// de tantivy es caro (fsync + segmento nuevo), así que el llamador lo agenda explícitamente
// (commit cada N escrituras, y en los flushes del rebuild/update). Es la misma semántica
// near-real-time de Elasticsearch.
//
// CONTRATO DE DURABILIDAD: cerrar el índice (close() manual, __destruct del cliente, o el Drop
// nativo) NO commitea — sólo libera el estado, y al dropearse el IndexWriter tantivy DESCARTA su
// buffer no commiteado. No hay "commit de red de seguridad" al destruir: todo lo escrito desde el
// último commit() se pierde. El llamador DEBE commitear explícitamente antes de soltar el cliente
// si quiere persistir esas escrituras.

pub fn add_document(state: &mut IndexState, doc_json: &str) -> Result<(), String> {
    let doc = parse_doc(state, doc_json)?;
    ensure_writer(state)?
        .add_document(doc)
        .map_err(|e| format!("add_document falló: {e}"))?;
    Ok(())
}

pub fn delete_by_id(
    state: &mut IndexState,
    key_field: &str,
    key_value: &str,
) -> Result<(), String> {
    let field = state
        .schema
        .get_field(key_field)
        .map_err(|_| format!("campo clave '{key_field}' no existe"))?;
    let term = Term::from_field_text(field, key_value);
    ensure_writer(state)?.delete_term(term);
    Ok(())
}

pub fn update_document(
    state: &mut IndexState,
    key_field: &str,
    key_value: &str,
    doc_json: &str,
) -> Result<(), String> {
    let field = state
        .schema
        .get_field(key_field)
        .map_err(|_| format!("campo clave '{key_field}' no existe"))?;
    let term = Term::from_field_text(field, key_value);
    let doc = parse_doc(state, doc_json)?;
    let w = ensure_writer(state)?;
    w.delete_term(term);
    w.add_document(doc)
        .map_err(|e| format!("add en update falló: {e}"))?;
    Ok(())
}

pub fn commit(state: &mut IndexState) -> Result<(), String> {
    if let Some(w) = state.writer.as_mut() {
        w.commit().map_err(|e| format!("commit falló: {e}"))?;
    }
    state
        .reader
        .reload()
        .map_err(|e| format!("reload falló: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{close, open_or_create, with_state};
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
    fn writes_are_invisible_until_commit() {
        let dir = std::env::temp_dir().join(format!("tv_w_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();

        // add NO commitea: el doc queda en el buffer del writer, invisible al reader.
        with_state(h, |s| {
            add_document(s, r#"{"id_key":"42","title":"hola mundo"}"#)
        })
        .unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 0);

        // recién el commit explícito lo hace visible.
        with_state(h, commit).unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 1);

        // idem para delete: aplicado pero invisible hasta commit.
        with_state(h, |s| delete_by_id(s, "id_key", "42")).unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 1);
        with_state(h, commit).unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 0);

        close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_replaces_the_document_for_a_key() {
        let dir = std::env::temp_dir().join(format!("tv_w_{}_update", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();

        with_state(h, |s| {
            add_document(s, r#"{"id_key":"7","title":"titulo viejo"}"#)
        })
        .unwrap();
        with_state(h, commit).unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 1);

        // update = delete-by-key + add en el mismo batch: sigue habiendo un solo doc para la clave.
        with_state(h, |s| {
            update_document(s, "id_key", "7", r#"{"id_key":"7","title":"titulo nuevo"}"#)
        })
        .unwrap();
        with_state(h, commit).unwrap();
        assert_eq!(with_state(h, |s| s.doc_count()).unwrap(), 1);

        let out = with_state(h, |s| {
            crate::query::search(s, r#"{"text":"nuevo","text_fields":["title"],"limit":5}"#)
        })
        .unwrap();
        assert!(out.contains("\"id_key\":\"7\""));
        // el término viejo ya no debe matchear.
        let old = with_state(h, |s| {
            crate::query::search(s, r#"{"text":"viejo","text_fields":["title"],"limit":5}"#)
        })
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&old).unwrap();
        assert_eq!(parsed["hits"].as_array().unwrap().len(), 0);

        close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contended_writer_lock_is_marked_with_the_busy_prefix() {
        // tantivy permite un solo writer por directorio (file-lock exclusivo). Un segundo handle
        // sobre el mismo dir que intente escribir debe recibir un error prefijado con
        // WRITER_LOCKED_PREFIX, que los bindings mapean a IndexBusyException. Es la feature de
        // degradación por contención — hasta ahora sin cobertura.
        let dir = std::env::temp_dir().join(format!("tv_w_{}_locked", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let h1 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        // fuerza la creación del writer en h1: toma el lock exclusivo del directorio.
        with_state(h1, |s| add_document(s, r#"{"id_key":"1","title":"a"}"#)).unwrap();

        let h2 = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        let err = with_state(h2, |s| add_document(s, r#"{"id_key":"2","title":"b"}"#)).unwrap_err();
        assert!(
            err.starts_with(WRITER_LOCKED_PREFIX),
            "esperaba prefijo '{WRITER_LOCKED_PREFIX}', obtuve: {err}"
        );

        close(h1);
        close(h2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
