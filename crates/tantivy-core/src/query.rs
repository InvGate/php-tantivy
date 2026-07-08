use serde::Deserialize;
use serde_json::json;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, Query, RegexQuery, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{TantivyDocument, Term};

use crate::registry::IndexState;

#[derive(Debug, Deserialize)]
struct WhereClause {
    field: String,
    value: String,
    #[serde(default)]
    occur: String, // "must" | "must_not" | "should"
}

#[derive(Debug, Deserialize)]
struct InClause {
    field: String,
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct QueryInput {
    #[serde(default)]
    text: String,
    #[serde(default)]
    text_fields: Vec<String>,
    #[serde(default)]
    r#where: Vec<WhereClause>,
    #[serde(default)]
    r#in: Vec<InClause>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    min_score: f32,
}

fn default_limit() -> usize {
    20
}

/// Techo defensivo para `limit`: el valor viene del JSON del caller y `TopDocs` reserva memoria
/// proporcional a él, así que un valor absurdo (o `usize::MAX`) podría agotar la memoria del worker.
const MAX_RESULT_LIMIT: usize = 10_000;

/// Techo de tokens de texto por consulta. `input.text` es del caller y por cada token se arman hasta
/// 3 sub-queries por campo (exacta + fuzzy + prefijo); un texto enorme (p. ej. un paste de miles de
/// palabras) generaría miles de leaf-queries y una búsqueda carísima. Acotamos y descartamos el resto.
const MAX_QUERY_TOKENS: usize = 32;

/// Acota los tokens de texto al techo, descartando el excedente.
fn cap_tokens(mut words: Vec<String>) -> Vec<String> {
    words.truncate(MAX_QUERY_TOKENS);
    words
}

/// Acota el `limit` pedido al techo defensivo.
fn capped_limit(requested: usize) -> usize {
    requested.min(MAX_RESULT_LIMIT)
}

/// distancia de edición aproximada segun el largo del término (parity funcional con similarity 0.6).
fn fuzzy_distance(term_len: usize) -> u8 {
    if term_len < 3 {
        0
    } else if term_len <= 5 {
        1
    } else {
        2
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

fn occur_of(s: &str) -> Occur {
    match s {
        "must_not" => Occur::MustNot,
        "should" => Occur::Should,
        _ => Occur::Must,
    }
}

pub fn search(state: &IndexState, query_json: &str) -> Result<String, String> {
    let input: QueryInput =
        serde_json::from_str(query_json).map_err(|e| format!("query JSON inválido: {e}"))?;

    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    // texto: por cada palabra, un OR de [exacto, fuzzy, prefijo] sobre todos los campos de texto; requerido (MUST).
    let all_words = tokenize(&input.text);
    let total_tokens = all_words.len();
    let words = cap_tokens(all_words);
    if total_tokens > words.len() {
        // Operacional: la query traía más tokens que el techo; usamos los primeros y descartamos el
        // resto. stderr → error log de PHP-FPM (con catch_workers_output). Raro salvo input patológico.
        eprintln!(
            "[tantivy] search: query had {total_tokens} tokens; using the first {} and discarding the rest (MAX_QUERY_TOKENS)",
            words.len()
        );
    }
    for word in &words {
        let mut per_word: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for fname in &input.text_fields {
            let field = match state.schema.get_field(fname) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let term = Term::from_field_text(field, word);
            per_word.push((
                Occur::Should,
                Box::new(TermQuery::new(term.clone(), IndexRecordOption::WithFreqs)),
            ));
            let dist = fuzzy_distance(word.chars().count());
            if dist > 0 {
                per_word.push((Occur::Should, Box::new(FuzzyTermQuery::new(term, dist, true))));
            }
            if word.chars().count() >= 2 {
                if let Ok(rq) =
                    RegexQuery::from_pattern(&format!("{}.*", regex_escape(word)), field)
                {
                    per_word.push((Occur::Should, Box::new(rq)));
                }
            }
        }
        if !per_word.is_empty() {
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(per_word))));
        }
    }

    // where -> term sobre <field>_key con su occur.
    for w in &input.r#where {
        let field = state
            .schema
            .get_field(&w.field)
            .map_err(|_| format!("campo where '{}' no existe", w.field))?;
        let term = Term::from_field_text(field, &w.value);
        clauses.push((
            occur_of(&w.occur),
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }

    // in -> grupo SHOULD de terms sobre <field>_key, requerido en el top.
    for clause in &input.r#in {
        let field = state
            .schema
            .get_field(&clause.field)
            .map_err(|_| format!("campo in '{}' no existe", clause.field))?;
        let shoulds: Vec<(Occur, Box<dyn Query>)> = clause
            .values
            .iter()
            .map(|v| {
                let t = Term::from_field_text(field, v);
                (
                    Occur::Should,
                    Box::new(TermQuery::new(t, IndexRecordOption::Basic)) as Box<dyn Query>,
                )
            })
            .collect();
        if !shoulds.is_empty() {
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(shoulds))));
        }
    }

    // sin cláusulas -> resultado vacío (equivale a "sin match").
    // `limit == 0` también corta acá: `TopDocs::with_limit(0)` paniquea (assert limit >= 1) y el
    // valor viene del caller, así que lo tratamos como "pedí 0 resultados" en vez de reventar.
    if clauses.is_empty() || input.limit == 0 {
        return Ok(json!({"hits": []}).to_string());
    }

    let query = BooleanQuery::new(clauses);
    let searcher = state.reader.searcher();
    let top = searcher
        .search(&query, &TopDocs::with_limit(capped_limit(input.limit)))
        .map_err(|e| format!("search falló: {e}"))?;

    let mut hits = Vec::new();
    for (score, addr) in top {
        if score < input.min_score {
            continue;
        }
        let doc: TantivyDocument = searcher
            .doc(addr)
            .map_err(|e| format!("no se pudo leer doc: {e}"))?;
        let mut fields = serde_json::Map::new();
        for (field, field_entry) in state.schema.fields() {
            if !field_entry.is_stored() {
                continue;
            }
            if let Some(v) = doc.get_first(field) {
                if let Some(s) = v.as_str() {
                    fields.insert(field_entry.name().to_string(), json!(s));
                }
            }
        }
        hits.push(json!({"score": score, "fields": fields}));
    }

    Ok(json!({"hits": hits}).to_string())
}

/// escapa metacaracteres regex para usar la palabra como prefijo literal.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{close, open_or_create, with_state};
    use crate::schema::{FieldsDescriptor, IndexConfig};
    use crate::writer::{add_document, commit};

    fn cfg(path: &str) -> IndexConfig {
        IndexConfig {
            path: path.to_string(),
            id_field: "id_key".into(),
            fields: FieldsDescriptor {
                text: vec!["title".into()],
                keys: vec!["id_key".into(), "visibility_type_key".into()],
                attributes: vec![],
            },
            writer_heap_bytes: 15_000_000,
        }
    }

    #[test]
    fn finds_fuzzy_and_filters_by_where() {
        let dir = std::env::temp_dir().join(format!("tv_q_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        with_state(h, |s| {
            add_document(
                s,
                r#"{"id_key":"1","title":"reset password","visibility_type_key":"public"}"#,
            )
        })
        .unwrap();
        with_state(h, |s| {
            add_document(
                s,
                r#"{"id_key":"2","title":"reset password","visibility_type_key":"private"}"#,
            )
        })
        .unwrap();
        // add ya no commitea (NRT): hay que commitear para que las búsquedas vean los docs.
        with_state(h, commit).unwrap();

        let q = r#"{"text":"pasword","text_fields":["title"],
            "where":[{"field":"visibility_type_key","value":"public","occur":"must"}],
            "limit":10}"#;
        let out = with_state(h, |s| search(s, q)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let hits = parsed["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["fields"]["id_key"], "1");

        close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limit_zero_returns_empty_without_panicking() {
        // `TopDocs::with_limit(0)` panics dentro de tantivy (assert limit >= 1). El límite viene
        // del JSON del caller, así que un `limit:0` explícito no debe reventar: devuelve vacío.
        let dir = std::env::temp_dir().join(format!("tv_q_{}_limit0", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        with_state(h, |s| add_document(s, r#"{"id_key":"1","title":"reset password"}"#)).unwrap();
        with_state(h, commit).unwrap();

        let q = r#"{"text":"reset","text_fields":["title"],"limit":0}"#;
        let out = with_state(h, |s| search(s, q)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["hits"].as_array().unwrap().len(), 0);

        close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capped_limit_clamps_absurd_values_but_preserves_normal_ones() {
        // Un `limit` gigante del caller reservaría memoria sin techo en el colector. Lo acotamos.
        assert_eq!(capped_limit(20), 20);
        assert_eq!(capped_limit(MAX_RESULT_LIMIT), MAX_RESULT_LIMIT);
        assert_eq!(capped_limit(usize::MAX), MAX_RESULT_LIMIT);
    }

    #[test]
    fn cap_tokens_truncates_beyond_the_max_but_keeps_short_queries() {
        let many: Vec<String> = (0..MAX_QUERY_TOKENS + 8).map(|i| format!("w{i}")).collect();
        assert_eq!(cap_tokens(many).len(), MAX_QUERY_TOKENS);

        let few: Vec<String> = vec!["reset".into(), "password".into()];
        assert_eq!(cap_tokens(few).len(), 2);
    }
}
