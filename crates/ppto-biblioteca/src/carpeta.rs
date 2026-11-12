// SPDX-License-Identifier: GPL-3.0-or-later
//! Carpeta de tarifas: los documentos de tarifa de cada fabricante, la única
//! fuente de los PVP.
//!
//! ```text
//! Tarifas\
//!   KOOLAIR\
//!     2026-05 Tarifa KOOLAIR.pdf      ← vigente (la fecha más reciente)
//!     2025-03 Tarifa KOOLAIR.pdf      ← historial
//!   S&P\
//!     2026-06 Tarifa S&P.pdf
//! ```
//!
//! - Una carpeta por fabricante, con el mismo nombre que en los materiales
//!   (en mayúsculas).
//! - Cada fichero empieza por la fecha de la tarifa, `AAAA-MM` (o `AAAA`), y
//!   es PDF o Excel. El de fecha más reciente es el vigente.

use std::path::{Path, PathBuf};

/// Cómo deben estar los ficheros (para enseñarlo en el programa).
pub const NORMAS: &str = "\
• Una carpeta por fabricante, con su nombre en mayúsculas, igual que en los materiales \
(KOOLAIR, TROX, S&P, MITSUBISHI ELECTRIC…).
• Dentro, cada tarifa en PDF o Excel, con el nombre empezando por la fecha de la tarifa \
AAAA-MM (año y mes), por ejemplo «2026-05 Tarifa KOOLAIR.pdf».
• La de fecha más reciente es la vigente; las anteriores se quedan como historial (no se borran).
• Solo tarifas generales del fabricante. Las ofertas para una obra van con la obra, no aquí.
• Use «Añadir tarifa…»: elige el fichero, pone el nombre correcto y lo copia a su carpeta.";

/// Extensiones admitidas.
pub const EXTENSIONES: [&str; 4] = ["pdf", "xlsx", "xls", "csv"];

/// Un documento de tarifa de la carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocTarifa {
    pub fabricante: String,
    /// `AAAA-MM`.
    pub fecha: String,
    pub ruta: PathBuf,
}

/// Lo que hay en la carpeta de tarifas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carpeta {
    /// Por fabricante, sus tarifas de la más reciente (vigente) a la más antigua.
    pub fabricantes: Vec<(String, Vec<DocTarifa>)>,
    /// Ficheros que no siguen las normas (con el motivo).
    pub mal: Vec<(PathBuf, String)>,
}

/// Fecha `AAAA-MM` del principio del nombre (`AAAA-MM…`, `AAAA_MM…`,
/// `AAAA.MM…` o solo `AAAA`, que cuenta como enero).
pub fn fecha_del_nombre(nombre: &str) -> Option<String> {
    let b = nombre.as_bytes();
    if b.len() < 4 || !b[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let anio = &nombre[..4];
    let y: u32 = anio.parse().ok()?;
    if !(1990..=2100).contains(&y) {
        return None;
    }
    if b.len() >= 7 && matches!(b[4], b'-' | b'_' | b'.') && b[5..7].iter().all(u8::is_ascii_digit) {
        let m: u32 = nombre[5..7].parse().ok()?;
        // Un mes que no existe no es «solo año»: el nombre está mal
        return ((1..=12).contains(&m) && b.get(7).is_none_or(|c| !c.is_ascii_digit()))
            .then(|| format!("{anio}-{m:02}"));
    }
    if b.get(4).is_none_or(|c| !c.is_ascii_digit()) {
        return Some(format!("{anio}-01"));
    }
    None
}

/// Nombre de la carpeta de un fabricante: en mayúsculas y sin caracteres que
/// Windows no admite.
pub fn nombre_carpeta(fabricante: &str) -> String {
    ppto_core::descuentos::normalizar(fabricante)
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_owned()
}

fn extension_valida(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONES.contains(&e.to_ascii_lowercase().as_str()))
}

/// Recorre la carpeta de tarifas.
pub fn explorar(carpeta: &Path) -> Carpeta {
    let mut c = Carpeta::default();
    let Ok(rd) = std::fs::read_dir(carpeta) else { return c };
    let mut dirs: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs {
        if !d.is_dir() {
            if d.is_file() {
                c.mal
                    .push((d, "está suelto: debe ir dentro de la carpeta de su fabricante".into()));
            }
            continue;
        }
        let fab = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut docs = Vec::new();
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for f in rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_file()) {
            let nombre = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Índices, descuentos y ficheros temporales de Office no son tarifas
            if nombre.starts_with("~$") || nombre.ends_with(".json") || nombre.ends_with(".txt") {
                continue;
            }
            if !extension_valida(&f) {
                c.mal.push((f, "no es PDF ni Excel".into()));
                continue;
            }
            match fecha_del_nombre(&nombre) {
                Some(fecha) => docs.push(DocTarifa {
                    fabricante: fab.clone(),
                    fecha,
                    ruta: f,
                }),
                None => c.mal.push((f, "el nombre no empieza por la fecha AAAA-MM".into())),
            }
        }
        docs.sort_by(|a, b| b.fecha.cmp(&a.fecha).then_with(|| b.ruta.cmp(&a.ruta)));
        if !docs.is_empty() {
            c.fabricantes.push((fab, docs));
        }
    }
    c
}

/// Tarifa vigente de un fabricante (la de fecha más reciente).
pub fn vigente(carpeta: &Path, fabricante: &str) -> Option<DocTarifa> {
    let n = nombre_carpeta(fabricante);
    explorar(carpeta)
        .fabricantes
        .into_iter()
        .find(|(f, _)| nombre_carpeta(f) == n)
        .and_then(|(_, v)| v.into_iter().next())
}

/// Copia `origen` a la carpeta de tarifas como
/// `<FABRICANTE>\<AAAA-MM> Tarifa <FABRICANTE>.<ext>`. No sobrescribe.
pub fn anadir(carpeta: &Path, fabricante: &str, fecha: &str, origen: &Path) -> std::io::Result<PathBuf> {
    let err = |m: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, m);
    let fab = nombre_carpeta(fabricante);
    if fab.is_empty() {
        return Err(err("falta el fabricante".into()));
    }
    let fecha = fecha_del_nombre(fecha.trim())
        .filter(|_| fecha.trim().len() <= 7)
        .ok_or_else(|| err(format!("fecha «{fecha}» no válida: use AAAA-MM, p. ej. 2026-05")))?;
    if !extension_valida(origen) {
        return Err(err("la tarifa debe ser PDF o Excel".into()));
    }
    let ext = origen
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("pdf")
        .to_ascii_lowercase();
    let dir = carpeta.join(&fab);
    std::fs::create_dir_all(&dir)?;
    let destino = dir.join(format!("{fecha} Tarifa {fab}.{ext}"));
    if destino.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("ya existe {}", destino.display()),
        ));
    }
    std::fs::copy(origen, &destino)?;
    Ok(destino)
}

/// Carpeta de tarifas configurada (`tarifas-ruta.txt` de la configuración).
pub fn ruta_carpeta() -> Option<PathBuf> {
    let cfg = ppto_propuestas::carpeta_config()?;
    std::fs::read_to_string(cfg.join("tarifas-ruta.txt"))
        .ok()
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Apunta la carpeta de tarifas (la crea si no existe).
pub fn fijar_carpeta(carpeta: &Path) -> std::io::Result<()> {
    let cfg = ppto_propuestas::carpeta_config()
        .ok_or_else(|| std::io::Error::other("no se encuentra la carpeta de configuración"))?;
    std::fs::create_dir_all(&cfg)?;
    std::fs::create_dir_all(carpeta)?;
    std::fs::write(cfg.join("tarifas-ruta.txt"), carpeta.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fechas_del_nombre() {
        assert_eq!(
            fecha_del_nombre("2026-05 Tarifa KOOLAIR.pdf").as_deref(),
            Some("2026-05")
        );
        assert_eq!(fecha_del_nombre("2026_10 tarifa.xlsx").as_deref(), Some("2026-10"));
        assert_eq!(fecha_del_nombre("2026 Tarifa TROX.pdf").as_deref(), Some("2026-01"));
        assert_eq!(fecha_del_nombre("2026-13 x.pdf"), None);
        assert_eq!(fecha_del_nombre("Tarifa 2026.pdf"), None);
        assert_eq!(fecha_del_nombre("20260512.pdf"), None);
    }

    #[test]
    fn carpeta_de_tarifas() {
        let dir = std::env::temp_dir().join(format!("tarifas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("FABSINT")).unwrap();
        let origen = dir.join("descargada.pdf");
        std::fs::write(&origen, b"%PDF sintetico").unwrap();
        std::fs::write(dir.join("FABSINT").join("2025-03 Tarifa FABSINT.pdf"), b"%PDF").unwrap();
        std::fs::write(dir.join("FABSINT").join("tarifa sin fecha.pdf"), b"%PDF").unwrap();
        std::fs::write(dir.join("suelta.pdf"), b"%PDF").unwrap();
        let r = anadir(&dir, "fabsint", "2026-05", &origen).unwrap();
        assert!(
            r.ends_with("FABSINT/2026-05 Tarifa FABSINT.pdf") || r.ends_with("FABSINT\\2026-05 Tarifa FABSINT.pdf")
        );
        assert!(anadir(&dir, "FABSINT", "2026-05", &origen).is_err(), "no sobrescribe");
        assert!(anadir(&dir, "FABSINT", "mayo", &origen).is_err());
        let c = explorar(&dir);
        assert_eq!(c.fabricantes.len(), 1);
        assert_eq!(c.fabricantes[0].1[0].fecha, "2026-05", "la vigente primero");
        assert_eq!(c.fabricantes[0].1.len(), 2);
        assert_eq!(c.mal.len(), 3, "{:?}", c.mal); // sin fecha, suelta, descargada.pdf suelta
        assert_eq!(vigente(&dir, "fabsint").unwrap().fecha, "2026-05");
        assert_eq!(nombre_carpeta("S&P"), "S&P");
        assert_eq!(nombre_carpeta("A/B"), "A-B");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
