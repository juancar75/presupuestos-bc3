// SPDX-License-Identifier: GPL-3.0-or-later
//! Datos y documentos de la obra activa: la ficha (dirección, cliente,
//! superficies, plantas, plazas…), la carpeta de documentos (planos,
//! memorias, cálculos) y los datos que la IA propone sacados de ellos.
//! La IA no cambia la ficha: propone y el usuario acepta en el programa.

use crate::{Estado, texto};
use ppto_core::Decimal;
use ppto_core::especialidad::Especialidad;
use ppto_core::ficha;
use ppto_propuestas::{DatoPropuesto, FichaPropuesta, ObraActiva};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const LEGIBLES: [&str; 9] = ["pdf", "docx", "docm", "xlsx", "xls", "xlsm", "ods", "csv", "txt"];
const MAX_TROZO: usize = 60_000;

fn obra(estado: &Estado) -> Result<(String, BTreeMap<String, String>), String> {
    let r = estado
        .ruta_obra_activa
        .as_ref()
        .ok_or("no se encuentra la carpeta de configuración")?;
    let o = ObraActiva::cargar(r)?;
    Ok((o.clave, o.datos))
}

fn carpeta(datos: &BTreeMap<String, String>) -> Result<PathBuf, String> {
    datos
        .get("obra.documentos")
        .map(|s| PathBuf::from(s.trim().trim_matches('"')))
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            "la obra no tiene carpeta de documentos: pida al usuario que la elija en Obra → Datos de la obra".to_owned()
        })
}

fn especialidades(datos: &BTreeMap<String, String>) -> Vec<Especialidad> {
    datos
        .get("especialidad")
        .map(|e| e.split(',').filter_map(Especialidad::leer).collect())
        .unwrap_or_default()
}

/// Datos de la obra: los de texto y la ficha que piden sus especialidades.
pub(crate) fn ver_datos_obra(estado: &Estado) -> Result<String, String> {
    let (clave, datos) = obra(estado)?;
    let textos = ficha::textos_de_datos(&datos);
    let f = ficha::de_datos(&datos);
    let es = especialidades(&datos);
    let mut t = format!("Datos de la obra {clave}");
    if !es.is_empty() {
        let n: Vec<&str> = es.iter().map(|e| e.nombre()).collect();
        let _ = write!(t, " ({})", n.join(" + "));
    }
    t.push_str("\n\n| Clave | Dato | Valor |\n|---|---|---|\n");
    for c in ficha::TEXTOS {
        let v = textos.get(c.clave).map_or("—", String::as_str);
        let _ = writeln!(t, "| {} | {} | {v} |", c.clave, c.etiqueta);
    }
    for c in ficha::campos_de(&es) {
        let v = f
            .get(c.clave)
            .map_or("—".to_owned(), |v| format!("{} {}", v.normalize(), c.unidad));
        let _ = writeln!(t, "| {} | {} ({}) | {v} |", c.clave, c.etiqueta, c.grupo);
    }
    let pendientes = estado
        .ruta_ficha
        .as_ref()
        .map(|r| FichaPropuesta::cargar(r))
        .and_then(|mut p| p.obras.remove(&clave))
        .unwrap_or_default();
    if !pendientes.is_empty() {
        let _ = writeln!(
            t,
            "\n{} datos propuestos pendientes de aceptar en el programa.",
            pendientes.len()
        );
    }
    t.push_str(
        "\nLos que falten («—»), búsquelos en los documentos de la obra (documentos_obra, leer_documento) \
         y propóngalos con proponer_datos_obra, citando documento y página.",
    );
    Ok(t)
}

fn recorrer(dir: &Path, base: &Path, nivel: u8, out: &mut Vec<(String, u64, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut v: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    v.sort();
    for p in v {
        if out.len() >= 400 {
            return;
        }
        let nombre = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if nombre.starts_with('.') || nombre.starts_with("~$") {
            continue;
        }
        if p.is_dir() {
            if nivel < 4 {
                recorrer(&p, base, nivel + 1, out);
            }
            continue;
        }
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let nota = if LEGIBLES.contains(&ext.as_str()) {
            "se lee".to_owned()
        } else if ext == "dwg" {
            "no se lee: exportar a DXF o PDF".to_owned()
        } else if ext == "dxf" {
            "plano DXF: aún no se lee".to_owned()
        } else {
            "no se lee".to_owned()
        };
        let rel = p.strip_prefix(base).unwrap_or(&p).display().to_string();
        let tam = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        out.push((rel, tam, nota));
    }
}

/// Ficheros de la carpeta de documentos de la obra.
pub(crate) fn documentos_obra(estado: &Estado) -> Result<String, String> {
    let (_, datos) = obra(estado)?;
    let dir = carpeta(&datos)?;
    if !dir.is_dir() {
        return Err(format!("no se encuentra la carpeta de documentos {}", dir.display()));
    }
    let mut v = Vec::new();
    recorrer(&dir, &dir, 0, &mut v);
    let mut t = format!(
        "Carpeta de documentos: {}\n\n| Fichero | KB | |\n|---|---:|---|\n",
        dir.display()
    );
    for (r, tam, nota) in &v {
        let _ = writeln!(t, "| {r} | {} | {nota} |", tam.div_ceil(1024));
    }
    if v.len() >= 400 {
        t.push_str("\n(solo los 400 primeros)\n");
    }
    t.push_str(
        "\nLea primero la memoria (descriptiva y de cálculo) y el pliego: datos del edificio, marcas, \
         modelos y alcance. leer_documento con la ruta tal cual sale aquí.",
    );
    Ok(t)
}

/// Texto de un documento de la carpeta de la obra, por trozos.
pub(crate) fn leer_documento(estado: &Estado, a: &Value) -> Result<String, String> {
    let (_, datos) = obra(estado)?;
    let dir = carpeta(&datos)?;
    let rel = texto(a, "ruta")?.trim().trim_matches('"');
    let ruta = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        dir.join(rel)
    };
    // Solo dentro de la carpeta de la obra
    let (c_dir, c_ruta) = (
        dir.canonicalize().map_err(|e| format!("carpeta de documentos: {e}"))?,
        ruta.canonicalize()
            .map_err(|e| format!("no se encuentra {}: {e}", ruta.display()))?,
    );
    if !c_ruta.starts_with(&c_dir) {
        return Err("solo se leen documentos de la carpeta de la obra".into());
    }
    let todo = crate::asistente::extraer_texto(&c_ruta)?;
    let desde = a
        .get("desde")
        .and_then(Value::as_u64)
        .and_then(|d| usize::try_from(d).ok())
        .unwrap_or(0);
    let mut i = desde.min(todo.len());
    while !todo.is_char_boundary(i) {
        i -= 1;
    }
    let mut f = (i + MAX_TROZO).min(todo.len());
    while !todo.is_char_boundary(f) {
        f -= 1;
    }
    let mut t = format!("{}\n", &todo[i..f]);
    if f < todo.len() {
        let _ = write!(
            t,
            "\n[… sigue: {} de {} caracteres leídos; continúe con desde = {f}]",
            f,
            todo.len()
        );
    }
    Ok(t)
}

/// Propone datos de la ficha sacados de los documentos.
pub(crate) fn proponer_datos_obra(estado: &Estado, a: &Value) -> Result<String, String> {
    let (clave, _) = obra(estado)?;
    let lista = a
        .get("datos")
        .and_then(Value::as_array)
        .ok_or("falta «datos»: lista de {clave, valor, fuente}")?;
    let ahora = ppto_propuestas::ahora_segundos();
    let mut v = Vec::new();
    let mut mal = Vec::new();
    for d in lista {
        let k = d.get("clave").and_then(Value::as_str).unwrap_or("").trim();
        let valor = match d.get("valor") {
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let fuente = d.get("fuente").and_then(Value::as_str).unwrap_or("").trim();
        if ficha::describir(k).is_none() {
            mal.push(format!("{k}: clave desconocida (vea ver_datos_obra)"));
            continue;
        }
        if valor.is_empty() || fuente.is_empty() {
            mal.push(format!("{k}: faltan valor o fuente"));
            continue;
        }
        if ficha::es_numerica(k) && valor.replace(',', ".").parse::<Decimal>().is_err() {
            mal.push(format!("{k}: «{valor}» no es un número"));
            continue;
        }
        v.push(DatoPropuesto {
            clave: k.to_owned(),
            valor: if ficha::es_numerica(k) {
                valor.replace(',', ".")
            } else {
                valor
            },
            fuente: fuente.to_owned(),
            cuando: ahora,
        });
    }
    let n = v.len();
    if n > 0 {
        let r = estado
            .ruta_ficha
            .as_ref()
            .ok_or("no se encuentra la carpeta de configuración")?;
        FichaPropuesta::modificar(r, |f| f.proponer(&clave, v)).map_err(|e| e.to_string())?;
    }
    let mut t = format!(
        "{n} datos propuestos: el usuario los acepta o descarta en Obra → Datos de la obra (salen en azul con su fuente)."
    );
    if !mal.is_empty() {
        let _ = write!(t, "\nNo se han propuesto: {}.", mal.join("; "));
    }
    Ok(t)
}
