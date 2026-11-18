// SPDX-License-Identifier: GPL-3.0-or-later
//! Carpeta de documentos de una obra: dónde se deja cada cosa (proyecto,
//! mediciones, planos…) para que el programa y Claude los encuentren, y
//! qué hay en cada sitio.
//!
//! ```text
//! <obra>\
//!   01 Proyecto\          memorias, pliego, cálculos (PDF, Word)
//!   02 Mediciones\        ciego del cliente (BC3, Excel), mediciones
//!   03 Planos\            DXF (mejor) y PDF
//!   04 Ofertas\           ofertas de proveedores y subcontratas de esta obra
//!   05 Cliente\           correos, actas, aclaraciones, contrato
//! ```

use std::path::{Path, PathBuf};

/// Una subcarpeta de la carpeta de documentos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subcarpeta {
    pub nombre: &'static str,
    pub que_va: &'static str,
    /// Extensiones esperadas (en minúsculas).
    pub extensiones: &'static [&'static str],
}

pub const SUBCARPETAS: [Subcarpeta; 5] = [
    Subcarpeta {
        nombre: "01 Proyecto",
        que_va: "Memoria descriptiva, memoria de cálculo, pliego de condiciones (PDF o Word)",
        extensiones: &["pdf", "docx", "doc"],
    },
    Subcarpeta {
        nombre: "02 Mediciones",
        que_va: "Ciego o mediciones del cliente (BC3 o Excel) y mediciones propias",
        extensiones: &["bc3", "xlsx", "xls", "xlsm", "csv", "pdf"],
    },
    Subcarpeta {
        nombre: "03 Planos",
        que_va: "Planos en DXF (Claude cuenta aparatos y mide distancias) y en PDF; los DWG, guardarlos también como DXF",
        extensiones: &["dxf", "pdf", "dwg"],
    },
    Subcarpeta {
        nombre: "04 Ofertas",
        que_va: "Ofertas de proveedores y subcontratas para esta obra",
        extensiones: &["pdf", "xlsx", "xls", "docx"],
    },
    Subcarpeta {
        nombre: "05 Cliente",
        que_va: "Correos, actas, aclaraciones y contrato",
        extensiones: &["pdf", "msg", "eml", "docx", "txt"],
    },
];

/// Cómo debe estar la carpeta (para enseñarlo en el programa).
pub const NORMAS: &str = "\
Una carpeta por obra (en OneDrive o en la red) con estas subcarpetas:
• 01 Proyecto: memoria descriptiva, memoria de cálculo y pliego (PDF o Word).
• 02 Mediciones: el ciego o las mediciones del cliente (BC3 o Excel).
• 03 Planos: planos en DXF y en PDF. Los DWG, guárdelos también como DXF (Guardar como → DXF): \
Claude cuenta los aparatos dibujados como bloques y mide distancias.
• 04 Ofertas: ofertas de proveedores y subcontratas de esta obra.
• 05 Cliente: correos, actas, aclaraciones y contrato.
«Crear carpetas» crea las que falten. Con todo en su sitio, Claude rellena los datos de la obra y \
lee memorias y planos antes de valorar.";

/// Qué hay en una subcarpeta.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Contenido {
    pub nombre: String,
    pub existe: bool,
    pub ficheros: usize,
}

/// Revisión de la carpeta de documentos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Revision {
    pub subcarpetas: Vec<Contenido>,
    /// Ficheros sueltos en la raíz (deberían ir en una subcarpeta).
    pub sueltos: Vec<String>,
    /// DWG sin su DXF al lado.
    pub dwg_sin_dxf: Vec<String>,
    pub avisos: Vec<String>,
}

fn ficheros(dir: &Path, out: &mut Vec<PathBuf>, nivel: u8) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for p in rd.filter_map(Result::ok).map(|e| e.path()) {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if n.starts_with('.') || n.starts_with("~$") {
            continue;
        }
        if p.is_dir() {
            if nivel < 4 {
                ficheros(&p, out, nivel + 1);
            }
        } else {
            out.push(p);
        }
    }
}

fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Subcarpeta de la estructura con ese nombre (sin distinguir mayúsculas).
fn buscar(dir: &Path, nombre: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(nombre))
        })
}

/// Revisa la carpeta de documentos de una obra.
pub fn revisar(dir: &Path) -> Revision {
    let mut r = Revision::default();
    if !dir.is_dir() {
        r.avisos.push(format!("No se encuentra la carpeta {}.", dir.display()));
        return r;
    }
    for s in SUBCARPETAS {
        let mut c = Contenido {
            nombre: s.nombre.to_owned(),
            ..Default::default()
        };
        if let Some(d) = buscar(dir, s.nombre) {
            c.existe = true;
            let mut v = Vec::new();
            ficheros(&d, &mut v, 0);
            c.ficheros = v.len();
        }
        r.subcarpetas.push(c);
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut v: Vec<String> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|n| !n.starts_with('.') && !n.starts_with("~$"))
            .collect();
        v.sort();
        r.sueltos = v;
    }
    let mut todos = Vec::new();
    ficheros(dir, &mut todos, 0);
    for p in &todos {
        if ext(p) == "dwg" && !p.with_extension("dxf").exists() && !p.with_extension("DXF").exists() {
            r.dwg_sin_dxf.push(
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
        }
    }
    r.dwg_sin_dxf.sort();
    let falta = |n: &str| r.subcarpetas.iter().any(|c| c.nombre == n && c.ficheros == 0);
    if r.subcarpetas.iter().all(|c| !c.existe) {
        r.avisos.push(
            "La carpeta no tiene las subcarpetas (01 Proyecto, 02 Mediciones, 03 Planos…): «Crear carpetas».".into(),
        );
    } else {
        if falta("01 Proyecto") {
            r.avisos
                .push("01 Proyecto está vacía: deje ahí la memoria y el pliego.".into());
        }
        if falta("02 Mediciones") {
            r.avisos
                .push("02 Mediciones está vacía: deje ahí el ciego o las mediciones del cliente.".into());
        }
        if falta("03 Planos") {
            r.avisos
                .push("03 Planos está vacía: deje ahí los planos (mejor en DXF).".into());
        }
    }
    if !r.sueltos.is_empty() {
        r.avisos.push(format!(
            "{} ficheros sueltos en la raíz: muévalos a su subcarpeta.",
            r.sueltos.len()
        ));
    }
    if !r.dwg_sin_dxf.is_empty() {
        r.avisos.push(format!(
            "{} planos DWG sin su DXF (Guardar como → DXF en la misma carpeta): {}.",
            r.dwg_sin_dxf.len(),
            r.dwg_sin_dxf.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    r
}

/// Crea las subcarpetas que falten. Devuelve cuántas ha creado.
pub fn crear_estructura(dir: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut n = 0;
    for s in SUBCARPETAS {
        if buscar(dir, s.nombre).is_none() {
            std::fs::create_dir_all(dir.join(s.nombre))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Subcarpeta donde iría un fichero (por la subcarpeta en la que está).
pub fn subcarpeta_de(dir: &Path, fichero: &Path) -> Option<&'static str> {
    let rel = fichero.strip_prefix(dir).ok()?;
    let primera = rel.components().next()?.as_os_str().to_string_lossy().to_string();
    SUBCARPETAS
        .iter()
        .find(|s| s.nombre.eq_ignore_ascii_case(&primera))
        .map(|s| s.nombre)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crea_y_revisa_la_carpeta() {
        let dir = std::env::temp_dir().join(format!("docs-obra-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("suelto.pdf"), b"x").unwrap();
        let r = revisar(&dir);
        assert!(r.avisos.iter().any(|a| a.contains("Crear carpetas")), "{:?}", r.avisos);
        assert_eq!(crear_estructura(&dir).unwrap(), 5);
        assert_eq!(crear_estructura(&dir).unwrap(), 0, "no repite");
        std::fs::write(dir.join("01 Proyecto").join("memoria.pdf"), b"x").unwrap();
        std::fs::write(dir.join("03 Planos").join("planta.dwg"), b"x").unwrap();
        let r = revisar(&dir);
        assert_eq!(r.subcarpetas[0].ficheros, 1);
        assert!(r.avisos.iter().any(|a| a.contains("02 Mediciones está vacía")));
        assert!(r.avisos.iter().any(|a| a.contains("sueltos")));
        assert_eq!(r.dwg_sin_dxf, vec!["planta.dwg".to_owned()]);
        std::fs::write(dir.join("03 Planos").join("planta.dxf"), b"x").unwrap();
        assert!(revisar(&dir).dwg_sin_dxf.is_empty());
        assert_eq!(
            subcarpeta_de(&dir, &dir.join("03 Planos").join("planta.dxf")),
            Some("03 Planos")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
