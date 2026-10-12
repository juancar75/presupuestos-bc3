// SPDX-License-Identifier: GPL-3.0-or-later
//! Herramientas del panel de asistente: mensajes que el usuario deja en el
//! programa (encargos), lectura de adjuntos e informes.

use crate::{Estado, texto};
use ppto_propuestas::conversacion::{Autor, Conversaciones, carpeta_informes, guardar_informe as guardar};
use serde_json::Value;
use std::fmt::Write as _;
use std::path::Path;

/// Tamaño máximo del texto de un adjunto que se devuelve a la IA.
const MAX_ADJUNTO: usize = 150_000;

fn ruta(estado: &Estado) -> Result<std::path::PathBuf, String> {
    estado
        .ruta_conversaciones
        .clone()
        .ok_or_else(|| "no se encuentra la carpeta de configuración".to_owned())
}

/// Obra a la que se refieren los mensajes: la activa (la abre si hace falta).
fn obra(estado: &mut Estado) -> Result<String, String> {
    if estado.obra_activa.is_none() {
        crate::abrir_obra_activa(estado)?;
    }
    estado.obra_activa.clone().ok_or_else(|| "sin obra activa".to_owned())
}

pub(crate) fn leer_mensajes(estado: &mut Estado) -> Result<String, String> {
    let obra = obra(estado)?;
    let c = Conversaciones::cargar(&ruta(estado)?);
    let todos = c.de_obra(&obra);
    let pendientes = c.pendientes(&obra);
    let mut t = format!("Obra activa: {obra}\n");
    if pendientes.is_empty() {
        t.push_str("No hay mensajes pendientes del usuario en el panel del asistente.\n");
    }
    // Contexto: los últimos mensajes ya atendidos.
    let previos: Vec<_> = todos.iter().filter(|m| m.atendido).collect();
    if !previos.is_empty() {
        t.push_str("\n## Conversación anterior (últimos mensajes)\n");
        for m in previos.iter().rev().take(6).rev() {
            let quien = match m.autor {
                Autor::Usuario => "Usuario",
                Autor::Asistente => "Asistente",
                Autor::Sistema => "Programa",
            };
            let _ = writeln!(t, "- [{}] {quien}: {}", m.id, recortar(&m.texto, 600));
        }
    }
    for m in &pendientes {
        let _ = write!(t, "\n## Mensaje pendiente {}\n{}\n", m.id, m.texto);
        for a in &m.adjuntos {
            let _ = writeln!(t, "- Adjunto: {a} (léalo con leer_adjunto)");
        }
    }
    if !pendientes.is_empty() {
        t.push_str(
            "\nAtienda los mensajes (propuestas con proponer, informes con guardar_informe) y \
             conteste con responder; la respuesta aparece en el panel del programa.",
        );
    }
    Ok(t)
}

pub(crate) fn responder(estado: &mut Estado, a: &Value) -> Result<String, String> {
    let texto_r = texto(a, "texto")?.to_owned();
    let ids: Vec<u64> = a
        .get("mensajes")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    let obra = obra(estado)?;
    let id = Conversaciones::modificar(&ruta(estado)?, |c| c.responder(&obra, &ids, &texto_r, None))
        .map_err(|e| e.to_string())?;
    Ok(format!("Respuesta {id} enviada al panel del asistente de «{obra}»."))
}

pub(crate) fn leer_adjunto(a: &Value) -> Result<String, String> {
    let r = texto(a, "ruta")?.trim().trim_matches('"');
    let t = extraer_texto(Path::new(r))?;
    Ok(if t.len() > MAX_ADJUNTO {
        let mut corte = MAX_ADJUNTO;
        while !t.is_char_boundary(corte) {
            corte -= 1;
        }
        format!("{}\n[… recortado: {} caracteres en total]", &t[..corte], t.len())
    } else {
        t
    })
}

pub(crate) fn guardar_informe(estado: &mut Estado, a: &Value) -> Result<String, String> {
    let titulo = texto(a, "titulo")?;
    let contenido = texto(a, "contenido")?;
    let obra = obra(estado)?;
    let carpeta = carpeta_informes(&ruta(estado)?, &obra);
    let ruta = guardar(&carpeta, titulo, contenido).map_err(|e| e.to_string())?;
    Ok(format!("Informe guardado: {}", ruta.display()))
}

fn recortar(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// Texto de un fichero adjunto: hojas de cálculo (cada hoja como CSV con
/// `;`), PDF (texto extraído) o texto plano.
pub fn extraer_texto(ruta: &Path) -> Result<String, String> {
    let ext = ruta
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => hoja_calculo(ruta),
        "pdf" => {
            let t = pdf_extract::extract_text(ruta).map_err(|e| format!("no se pudo leer el PDF: {e}"))?;
            if t.trim().is_empty() {
                Err("el PDF no tiene texto (¿escaneado?): use el modo directo, que envía el PDF completo".into())
            } else {
                Ok(t)
            }
        }
        "csv" | "txt" | "md" | "bc3" | "json" => {
            let b = std::fs::read(ruta).map_err(|e| format!("no se pudo leer {}: {e}", ruta.display()))?;
            Ok(match String::from_utf8(b) {
                Ok(s) => s,
                // ANSI (Windows-1252/Latin-1), habitual en BC3 y CSV de Excel.
                Err(e) => e.into_bytes().iter().map(|&b| char::from(b)).collect(),
            })
        }
        otro => Err(format!(
            "tipo de adjunto no admitido «{otro}» (xlsx, xls, ods, pdf, csv, txt)"
        )),
    }
}

fn hoja_calculo(ruta: &Path) -> Result<String, String> {
    use calamine::Reader;
    let mut libro =
        calamine::open_workbook_auto(ruta).map_err(|e| format!("no se pudo leer {}: {e}", ruta.display()))?;
    let mut t = String::new();
    for hoja in libro.sheet_names().clone() {
        let Ok(rango) = libro.worksheet_range(&hoja) else {
            continue;
        };
        if rango.is_empty() {
            continue;
        }
        let _ = writeln!(t, "## Hoja «{hoja}»");
        for fila in rango.rows() {
            let celdas: Vec<String> = fila
                .iter()
                .map(|c| c.to_string().replace(['\n', '\r'], " ").replace(';', ","))
                .collect();
            if celdas.iter().all(String::is_empty) {
                continue;
            }
            let _ = writeln!(t, "{}", celdas.join(";").trim_end_matches(';'));
        }
        t.push('\n');
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PDF mínimo de una página con un texto (offsets de la tabla xref calculados).
    fn pdf_minimo(texto: &str) -> Vec<u8> {
        let contenido = format!("BT /F1 12 Tf 72 720 Td ({texto}) Tj ET");
        let objetos = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
            format!("<< /Length {} >>\nstream\n{contenido}\nendstream", contenido.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
        ];
        let mut b = b"%PDF-1.4\n".to_vec();
        let mut offs = Vec::new();
        for (i, o) in objetos.iter().enumerate() {
            offs.push(b.len());
            b.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = b.len();
        b.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objetos.len() + 1).as_bytes());
        for o in offs {
            b.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        b.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objetos.len() + 1
            )
            .as_bytes(),
        );
        b
    }

    #[test]
    fn texto_de_pdf_y_texto_ansi() {
        let dir = std::env::temp_dir().join(format!("mcp-adj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("oferta.pdf");
        std::fs::write(&pdf, pdf_minimo("Oferta KOOLAIR 1234 EUR")).unwrap();
        let t = extraer_texto(&pdf).unwrap();
        assert!(t.contains("Oferta KOOLAIR 1234 EUR"), "{t:?}");

        let csv = dir.join("ansi.csv");
        std::fs::write(&csv, b"Descripci\xf3n;Precio\n").unwrap();
        assert_eq!(extraer_texto(&csv).unwrap(), "Descripción;Precio\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
