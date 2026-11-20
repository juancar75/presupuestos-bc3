// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-legalizacion-app
//!
//! Programa **independiente** de costes de legalización. presupuestos-bc3 lo
//! abre en una ventana aparte con un [`Encargo`] (capítulos con su importe,
//! datos leídos de las mediciones y márgenes de la obra) y recibe de vuelta
//! las [`Entrega`]s: el expediente calculado de cada disciplina, con el que
//! crea o sustituye la partida `LEG-RITE` o `LEG-PCI`.
//!
//! El intercambio va por dos ficheros JSON en una carpeta temporal:
//!
//! ```text
//! ppto-legalizacion --encargo <encargo.json> --entregas <entregas.json>
//! ```
//!
//! El programa también se abre solo (sin argumentos) para calcular una
//! legalización fuera de una obra y copiar el descompuesto.
//!
//! La ventana ([`ventana::Legalizacion`]) es la misma dentro y fuera de
//! presupuestos-bc3: si no se encuentra el ejecutable, la GUI la muestra
//! dentro de su propia ventana.

use ppto_legalizacion::{Expediente, Lectura};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod ventana;

/// Nombre del ejecutable (sin extensión).
pub const EJECUTABLE: &str = "ppto-legalizacion";

/// Capítulo de primer nivel de la obra, candidato a base del PEM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capitulo {
    pub codigo: String,
    pub resumen: String,
    pub importe: Decimal,
}

/// Lo que presupuestos-bc3 pasa al programa de legalización.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Encargo {
    /// Nombre de la obra (título de la ventana).
    pub obra: String,
    /// Año del baremo (0 = el actual).
    pub anio: i32,
    pub capitulos: Vec<Capitulo>,
    /// Datos leídos de las mediciones para «Rellenar desde el presupuesto».
    pub lectura: Option<Lectura>,
    /// Costes indirectos, gastos generales y beneficio de la obra (%), para
    /// comparar la tarifa del rol instalador con la referencia de mercado.
    pub costes_indirectos: Decimal,
    pub gg: Decimal,
    pub bi: Decimal,
    /// Partidas `LEG-…` que ya hay en la obra.
    pub existentes: Vec<String>,
}

/// Expediente que el programa devuelve a presupuestos-bc3 para crear la
/// partida. presupuestos-bc3 recalcula con el mismo baremo del año.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrega {
    pub anio: i32,
    pub expediente: Expediente,
}

/// Año en curso (UTC), para el baremo cuando no lo indica el encargo.
pub fn anio_actual() -> i32 {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    anio_de_dias(i64::try_from(s / 86_400).unwrap_or(0))
}

/// Año a partir de días desde 1970-01-01 (algoritmo de H. Hinnant).
fn anio_de_dias(dias: i64) -> i32 {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let anio = yoe + era * 400 + i64::from(mes <= 2);
    i32::try_from(anio).unwrap_or(0)
}

/// Escribe un JSON de forma atómica (fichero temporal y renombrado), para
/// que el otro programa nunca lea un fichero a medias.
pub fn escribir_json<T: Serialize>(ruta: &Path, valor: &T) -> std::io::Result<()> {
    let texto = serde_json::to_string_pretty(valor).map_err(std::io::Error::other)?;
    let tmp = ruta.with_extension("json.tmp");
    std::fs::write(&tmp, texto)?;
    std::fs::rename(&tmp, ruta)
}

pub fn leer_encargo(ruta: &Path) -> std::io::Result<Encargo> {
    let texto = std::fs::read_to_string(ruta)?;
    serde_json::from_str(&texto).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Entregas hechas hasta ahora (ninguna si el fichero aún no existe).
pub fn leer_entregas(ruta: &Path) -> std::io::Result<Vec<Entrega>> {
    match std::fs::read_to_string(ruta) {
        Ok(t) if t.trim().is_empty() => Ok(Vec::new()),
        Ok(t) => serde_json::from_str(&t).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Añade una entrega al fichero de entregas.
pub fn anadir_entrega(ruta: &Path, e: Entrega) -> std::io::Result<()> {
    let mut v = leer_entregas(ruta)?;
    v.push(e);
    escribir_json(ruta, &v)
}

/// Carpeta temporal nueva para el intercambio de una sesión.
pub fn carpeta_intercambio() -> std::io::Result<PathBuf> {
    let marca = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join("presupuestos-bc3-legalizacion")
        .join(format!("{}-{marca}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Ejecutable del programa junto al de presupuestos-bc3 (o en `PPTO_LEGALIZACION`).
pub fn ruta_ejecutable() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("PPTO_LEGALIZACION").map(PathBuf::from)
        && r.is_file()
    {
        return Some(r);
    }
    let r = std::env::current_exe()
        .ok()?
        .with_file_name(format!("{EJECUTABLE}{}", std::env::consts::EXE_SUFFIX));
    r.is_file().then_some(r)
}

/// Argumentos de línea de órdenes: `--encargo <ruta> --entregas <ruta>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Argumentos {
    pub encargo: Option<PathBuf>,
    pub entregas: Option<PathBuf>,
}

impl Argumentos {
    pub fn leer(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut a = Self::default();
        let mut it = args.into_iter();
        while let Some(x) = it.next() {
            match x.as_str() {
                "--encargo" => a.encargo = Some(it.next().ok_or("falta la ruta de --encargo")?.into()),
                "--entregas" => a.entregas = Some(it.next().ok_or("falta la ruta de --entregas")?.into()),
                otro => return Err(format!("argumento desconocido: {otro}")),
            }
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppto_legalizacion::{Disciplina, Documento};
    use rust_decimal_macros::dec;

    #[test]
    fn anio_de_dias_hinnant() {
        assert_eq!(anio_de_dias(0), 1970);
        assert_eq!(anio_de_dias(20_412), 2025); // 2025-11-20
        assert_eq!(anio_de_dias(20_453), 2025); // 2025-12-31
        assert_eq!(anio_de_dias(20_454), 2026); // 2026-01-01
    }

    #[test]
    fn intercambio_de_encargo_y_entregas() {
        let dir = carpeta_intercambio().unwrap();
        let enc = Encargo {
            obra: "Gimnasio".into(),
            anio: 2026,
            capitulos: vec![Capitulo {
                codigo: "C01".into(),
                resumen: "CLIMATIZACIÓN".into(),
                importe: dec!(120000),
            }],
            gg: dec!(13),
            bi: dec!(6),
            ..Encargo::default()
        };
        let ruta = dir.join("encargo.json");
        escribir_json(&ruta, &enc).unwrap();
        assert_eq!(leer_encargo(&ruta).unwrap(), enc);
        let entregas = dir.join("entregas.json");
        assert!(leer_entregas(&entregas).unwrap().is_empty());
        for d in [Disciplina::Rite, Disciplina::Pci] {
            anadir_entrega(
                &entregas,
                Entrega {
                    anio: 2026,
                    expediente: Expediente::nuevo(d, Documento::Memoria, dec!(1000)),
                },
            )
            .unwrap();
        }
        let v = leer_entregas(&entregas).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].expediente.disciplina, Disciplina::Pci);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn argumentos() {
        let a = Argumentos::leer(["--encargo", "a.json", "--entregas", "b.json"].map(String::from)).unwrap();
        assert_eq!(a.encargo.as_deref(), Some(Path::new("a.json")));
        assert_eq!(a.entregas.as_deref(), Some(Path::new("b.json")));
        assert!(Argumentos::leer(["--encargo".to_owned()]).is_err());
        assert!(Argumentos::leer(["--otro".to_owned()]).is_err());
        assert_eq!(Argumentos::leer(Vec::new()).unwrap(), Argumentos::default());
    }
}
