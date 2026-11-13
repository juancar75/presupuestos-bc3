// SPDX-License-Identifier: GPL-3.0-or-later
//! Dónde está cada referencia en las tarifas de la carpeta: la página del PDF
//! en la que aparece, para abrir la tarifa justo ahí desde el presupuesto.
//!
//! El texto de cada página se lee una vez y se guarda al lado de la tarifa
//! (`<tarifa>.paginas.json`); se vuelve a leer solo si el PDF cambia.

use crate::carpeta::{self, DocTarifa};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Paginas {
    tam: u64,
    modificado: u64,
    /// Texto de cada página, ya normalizado ([`normalizar`]).
    paginas: Vec<String>,
}

/// Mayúsculas, sin tildes, y todo lo que no sea letra o cifra como un
/// espacio (seguidos, uno solo).
pub fn normalizar(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut esp = true;
    for c in s.chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'Á' | 'À' | 'Ä' => 'A',
            'é' | 'è' | 'ë' | 'É' | 'È' | 'Ë' => 'E',
            'í' | 'ì' | 'ï' | 'Í' | 'Ì' | 'Ï' => 'I',
            'ó' | 'ò' | 'ö' | 'Ó' | 'Ò' | 'Ö' => 'O',
            'ú' | 'ù' | 'ü' | 'Ú' | 'Ù' | 'Ü' => 'U',
            'ñ' | 'Ñ' => 'N',
            c => c.to_ascii_uppercase(),
        };
        if c.is_ascii_alphanumeric() {
            o.push(c);
            esp = false;
        } else if !esp {
            o.push(' ');
            esp = true;
        }
    }
    o.trim_end().to_owned()
}

fn cache_de(pdf: &Path) -> PathBuf {
    pdf.with_extension("paginas.json")
}

fn huella(pdf: &Path) -> (u64, u64) {
    let m = std::fs::metadata(pdf).ok();
    let tam = m.as_ref().map_or(0, std::fs::Metadata::len);
    let modificado = m
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    (tam, modificado)
}

/// Texto normalizado de cada página del PDF (del caché si el PDF no ha
/// cambiado; si no, se lee y se guarda el caché).
pub fn paginas_pdf(pdf: &Path) -> Result<Vec<String>, String> {
    let (tam, modificado) = huella(pdf);
    let cache = cache_de(pdf);
    if let Ok(t) = std::fs::read_to_string(&cache)
        && let Ok(p) = serde_json::from_str::<Paginas>(&t)
        && p.tam == tam
        && p.modificado == modificado
    {
        return Ok(p.paginas);
    }
    let paginas: Vec<String> = pdf_extract::extract_text_by_pages(pdf)
        .map_err(|e| format!("no se pudo leer {}: {e}", pdf.display()))?
        .iter()
        .map(|t| normalizar(t))
        .collect();
    if paginas.iter().all(String::is_empty) {
        return Err(format!(
            "{} no tiene texto (¿escaneado?): no se puede buscar la referencia",
            pdf.display()
        ));
    }
    // El caché es una ayuda: si no se puede escribir, se sigue igual
    let _ = serde_json::to_string(&Paginas {
        tam,
        modificado,
        paginas: paginas.clone(),
    })
    .map(|t| std::fs::write(&cache, t));
    Ok(paginas)
}

/// ¿Está `r` (normalizada) en `texto` como palabra o grupo de palabras
/// completo? («E22» no casa con «E220».)
fn contiene(texto: &str, r: &str) -> bool {
    let b = texto.as_bytes();
    texto.match_indices(r).any(|(i, _)| {
        let antes = i == 0 || b[i - 1] == b' ';
        let fin = i + r.len();
        let despues = fin == b.len() || b[fin] == b' ';
        antes && despues
    })
}

/// Primera página (desde 1) en la que aparece la referencia.
pub fn pagina_en(paginas: &[String], referencia: &str) -> Option<u32> {
    let r = normalizar(referencia);
    if r.len() < 3 {
        return None;
    }
    let pos = paginas.iter().position(|p| contiene(p, &r)).or_else(|| {
        // Con separadores distintos («ICTF-41» / «ICTF 41» / «ICTF41»)
        let junta = r.replace(' ', "");
        (junta.len() >= 5).then(|| paginas.iter().position(|p| p.replace(' ', "").contains(&junta)))?
    })?;
    u32::try_from(pos + 1).ok()
}

/// Dónde está una referencia de un fabricante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encontrada {
    pub doc: DocTarifa,
    /// Página del PDF (`None` si no se ha encontrado o la tarifa es un Excel).
    pub pagina: Option<u32>,
    /// Tarifas en las que no se ha podido buscar (escaneadas, dañadas).
    pub avisos: Vec<String>,
}

/// Busca `referencia` en las tarifas de `fabricante`, de la vigente a la más
/// antigua. Si no aparece en ninguna, devuelve la vigente sin página.
pub fn buscar(carpeta: &Path, fabricante: &str, referencia: &str) -> Option<Encontrada> {
    let n = carpeta::nombre_carpeta(fabricante);
    let docs = carpeta::explorar(carpeta)
        .fabricantes
        .into_iter()
        .find(|(f, _)| carpeta::nombre_carpeta(f) == n)?
        .1;
    let mut avisos = Vec::new();
    for d in &docs {
        let es_pdf = d.ruta.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !es_pdf {
            continue;
        }
        match paginas_pdf(&d.ruta) {
            Ok(p) => {
                if let Some(pagina) = pagina_en(&p, referencia) {
                    return Some(Encontrada {
                        doc: d.clone(),
                        pagina: Some(pagina),
                        avisos,
                    });
                }
            }
            Err(e) => avisos.push(e),
        }
    }
    Some(Encontrada {
        doc: docs.into_iter().next()?,
        pagina: None,
        avisos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_y_busca_palabras_completas() {
        assert_eq!(normalizar("  Rejilla  «ICTF-41» Ø200 "), "REJILLA ICTF 41 200");
        let p = vec![
            normalizar("Indice E220 E221"),
            normalizar("Serie E22 — 230 V"),
            normalizar("Toma ICTF41 y BIF70"),
        ];
        assert_eq!(pagina_en(&p, "E22"), Some(2), "E22 no es E220");
        assert_eq!(pagina_en(&p, "ictf-41"), Some(3), "con otro separador");
        assert_eq!(pagina_en(&p, "BIF 70"), Some(3));
        assert_eq!(pagina_en(&p, "XYZ999"), None);
        assert_eq!(pagina_en(&p, "E2"), None, "demasiado corta");
    }

    #[test]
    fn busca_en_un_pdf_y_guarda_el_cache() {
        // PDF sintético: el presupuesto de ejemplo impreso
        let dir = std::env::temp_dir().join(format!("indice-pdf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("FABSINT")).unwrap();
        let p = ppto_core::ejemplos::suelo_radiante();
        let pdf = dir.join("FABSINT").join("2026-05 Tarifa FABSINT.pdf");
        ppto_pdf::guardar_pdf(&p, &ppto_pdf::OpcionesPdf::default(), &pdf).unwrap();
        let e = buscar(&dir, "FABSINT", "SR.M2").unwrap();
        assert!(e.pagina.is_some(), "{e:?}");
        assert!(cache_de(&pdf).exists(), "deja el caché al lado");
        assert!(
            carpeta::explorar(&dir).mal.is_empty(),
            "el caché no es una tarifa mal puesta"
        );
        // Del caché, lo mismo
        assert_eq!(buscar(&dir, "FABSINT", "SR.M2").unwrap().pagina, e.pagina);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sin_pdf_devuelve_la_vigente() {
        let dir = std::env::temp_dir().join(format!("indice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("FABSINT")).unwrap();
        std::fs::write(dir.join("FABSINT").join("2026-05 Tarifa FABSINT.xlsx"), b"x").unwrap();
        let e = buscar(&dir, "fabsint", "REF1").unwrap();
        assert_eq!(e.doc.fecha, "2026-05");
        assert_eq!(e.pagina, None);
        assert!(buscar(&dir, "OTRO", "REF1").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
