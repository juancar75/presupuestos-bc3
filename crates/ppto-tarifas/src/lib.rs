// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-tarifas
//!
//! Tarifas de subcontratas: listas de precios **netos** (sin IVA) por unidad
//! de obra, p. ej. «SER0061 Instalación detector analógico programado
//! 15,00 €». Cada línea se usa en los descompuestos como un recurso de
//! **subcontrata** en lugar de la mano de obra propia.
//!
//! - [`leer_excel`] lee una tarifa de Excel (columnas Referencia,
//!   Descripción, Precio y, si la hay, Unidad). Si falta la unidad se deduce
//!   de la descripción ([`deducir_unidad`]) y la línea queda marcada para
//!   revisarla.
//! - [`Biblioteca`] guarda las tarifas de todas las subcontratas en un JSON
//!   del usuario (nunca en el repositorio) y permite buscar en todas.
//! - [`LineaTarifa::insertar_en`] da de alta la línea en un presupuesto como
//!   recurso de subcontrata con código `PROVEEDOR.REFERENCIA`.

use calamine::{Data, Reader};
use ppto_core::{Decimal, ErrorMotor, Naturaleza, Presupuesto};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ErrorTarifa {
    #[error("no se pudo leer el Excel: {0}")]
    Lectura(String),
    #[error("no se encuentra la cabecera (Referencia/Código, Descripción, Precio) en ninguna hoja")]
    SinCabecera,
    #[error("la tarifa no tiene ninguna línea con precio")]
    Vacia,
    #[error("no se pudo guardar la biblioteca de tarifas: {0}")]
    Guardar(String),
    #[error(transparent)]
    Motor(#[from] ErrorMotor),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaTarifa {
    pub referencia: String,
    pub descripcion: String,
    pub unidad: String,
    /// Precio neto sin IVA.
    pub precio: Decimal,
    /// `true` si la unidad no venía en la tarifa y se ha deducido con poca
    /// seguridad: hay que revisarla.
    #[serde(default)]
    pub revisar_unidad: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tarifa {
    /// Nombre corto de la subcontrata («LUXFIRE»).
    pub proveedor: String,
    /// Año o fecha de la tarifa («2026»).
    pub vigencia: String,
    /// Especialidad («Detección de incendios»).
    #[serde(default)]
    pub especialidad: String,
    /// Notas de lo que incluye (cable, tubo, pequeño material…).
    #[serde(default)]
    pub notas: String,
    pub lineas: Vec<LineaTarifa>,
}

impl Tarifa {
    /// Prefijo de los códigos: el proveedor en mayúsculas, solo letras y
    /// cifras, máximo 8 caracteres («LUXFIRE»).
    pub fn prefijo(&self) -> String {
        prefijo(&self.proveedor)
    }

    /// Código del recurso en el presupuesto: «LUXFIRE.SER0061».
    pub fn codigo(&self, l: &LineaTarifa) -> String {
        format!("{}.{}", self.prefijo(), l.referencia)
    }

    pub fn nombre(&self) -> String {
        format!("{} {}", self.proveedor, self.vigencia).trim().to_owned()
    }

    pub fn por_revisar(&self) -> usize {
        self.lineas.iter().filter(|l| l.revisar_unidad).count()
    }
}

fn prefijo(proveedor: &str) -> String {
    proveedor
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .take(8)
        .collect()
}

/// Deduce la unidad a partir de la descripción. Devuelve `(unidad, seguro)`.
///
/// - «/ METRO», «POR METRO», «ML» → `m` (seguro).
/// - Cable, manguera, tubo, canal, bandeja → `m` (probable, a revisar).
/// - «(1H)», «HORA» → `h` (seguro).
/// - Programación, puesta en marcha, certificado → `ud` por
///   obra (seguro).
/// - Desmontaje o retirada → `ud` a revisar (puede ser por metro o por equipo).
/// - Instalación / sustitución / colocación de un equipo → `ud` (seguro).
/// - Lo demás → `ud` a revisar.
pub fn deducir_unidad(descripcion: &str) -> (&'static str, bool) {
    let d = format!(" {} ", descripcion.to_uppercase().replace(['(', ')', '/', ','], " "));
    let tiene = |p: &[&str]| p.iter().any(|x| d.contains(x));
    if tiene(&[" METRO ", " METROS ", " ML ", " M.L. "]) {
        return ("m", true);
    }
    if tiene(&[" M2 ", " M² "]) {
        return ("m2", true);
    }
    if tiene(&[" 1H ", " HORA ", " HORAS "]) {
        return ("h", true);
    }
    if tiene(&[" DESMONTAJE ", " RETIRADA ", " LEGALIZACION ", " LEGALIZACIÓN "]) && !tiene(&[" UD ", " UNIDAD "]) {
        return ("ud", false);
    }
    if tiene(&[
        " CABLE",
        " CABLEADO ",
        " MANGUERA ",
        " TUBO ",
        " CANAL ",
        " BANDEJA ",
        " UTP ",
    ]) {
        return ("m", false);
    }
    if tiene(&[
        " PROGRAMACION ",
        " PROGRAMACIÓN ",
        " PUESTA EN MARCHA ",
        " CERTIFICADO ",
        " INSTALACION ",
        " INSTALACIÓN ",
        " SUSTITUCION ",
        " SUSTITUCIÓN ",
        " COLOCACION ",
        " COLOCACIÓN ",
    ]) {
        return ("ud", true);
    }
    ("ud", false)
}

/// Lee una tarifa de Excel. `proveedor` y `vigencia` vacíos se deducen del
/// título («TARIFA PRECIOS LUXFIRE 2026»).
pub fn leer_excel(ruta: &Path, proveedor: &str, vigencia: &str) -> Result<Tarifa, ErrorTarifa> {
    let mut libro = calamine::open_workbook_auto(ruta).map_err(|e| ErrorTarifa::Lectura(e.to_string()))?;
    let hojas = libro.sheet_names().clone();
    for hoja in hojas {
        let Ok(rango) = libro.worksheet_range(&hoja) else {
            continue;
        };
        let filas: Vec<Vec<String>> = rango.rows().map(|f| f.iter().map(texto_celda).collect()).collect();
        match leer_filas(&filas, proveedor, vigencia) {
            Err(ErrorTarifa::SinCabecera) => continue,
            r => return r,
        }
    }
    Err(ErrorTarifa::SinCabecera)
}

fn texto_celda(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_owned(),
        Data::Float(f) => {
            // Los precios de Excel vienen en binario: pasar por texto con 6
            // decimales y quitar ceros (no hay aritmética en coma flotante).
            let s = format!("{f:.6}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        Data::Int(i) => i.to_string(),
        other => other.to_string().trim().to_owned(),
    }
}

fn normal(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace(['á', 'à'], "a")
        .replace(['é', 'è'], "e")
        .replace(['í', 'ì'], "i")
        .replace(['ó', 'ò'], "o")
        .replace(['ú', 'ù'], "u")
        .replace(['(', ')', '€', '.'], "")
        .trim()
        .to_owned()
}

/// Columnas de la cabecera: (referencia, descripción, precio, unidad).
fn cabecera(fila: &[String]) -> Option<(usize, usize, usize, Option<usize>)> {
    let col = |nombres: &[&str]| {
        fila.iter().position(|c| {
            nombres
                .iter()
                .any(|n| normal(c) == *n || normal(c).starts_with(&format!("{n} ")))
        })
    };
    let r = col(&["referencia", "ref", "codigo", "cod", "articulo"])?;
    let d = col(&["descripcion", "resumen", "concepto", "denominacion"])?;
    let p = col(&["precio", "pvp", "precio neto", "importe", "precio unitario"])?;
    let u = col(&["unidad", "ud", "uds", "un"]);
    Some((r, d, p, u))
}

/// Lee una tarifa de filas de texto (cualquier origen tabular).
pub fn leer_filas(filas: &[Vec<String>], proveedor: &str, vigencia: &str) -> Result<Tarifa, ErrorTarifa> {
    let (i_cab, (cr, cd, cp, cu)) = filas
        .iter()
        .enumerate()
        .find_map(|(i, f)| cabecera(f).map(|c| (i, c)))
        .ok_or(ErrorTarifa::SinCabecera)?;
    let titulos: Vec<&str> = filas[..i_cab]
        .iter()
        .flat_map(|f| f.iter())
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    let (prov_titulo, anio_titulo) = titulos.first().map(|t| deducir_titulo(t)).unwrap_or_default();
    let especialidad = titulos.get(1).map(|s| (*s).to_owned()).unwrap_or_default();
    let mut lineas: Vec<LineaTarifa> = Vec::new();
    for f in &filas[i_cab + 1..] {
        let celda = |i: usize| f.get(i).map(String::as_str).unwrap_or("").trim();
        let (referencia, descripcion) = (celda(cr), celda(cd));
        let Some(precio) = ppto_core::formato::leer(celda(cp)) else {
            continue;
        };
        if referencia.is_empty() || descripcion.is_empty() || precio < Decimal::ZERO {
            continue;
        }
        let (unidad, revisar) = match cu.map(celda).filter(|u| !u.is_empty()) {
            Some(u) => (u.to_owned(), false),
            None => {
                let (u, seguro) = deducir_unidad(descripcion);
                (u.to_owned(), !seguro)
            }
        };
        // Referencia repetida: se queda la última
        lineas.retain(|l| l.referencia != referencia);
        lineas.push(LineaTarifa {
            referencia: referencia.to_owned(),
            descripcion: descripcion.to_owned(),
            unidad,
            precio,
            revisar_unidad: revisar,
        });
    }
    if lineas.is_empty() {
        return Err(ErrorTarifa::Vacia);
    }
    Ok(Tarifa {
        proveedor: if proveedor.trim().is_empty() {
            prov_titulo
        } else {
            proveedor.trim().to_owned()
        },
        vigencia: if vigencia.trim().is_empty() {
            anio_titulo
        } else {
            vigencia.trim().to_owned()
        },
        especialidad,
        notas: String::new(),
        lineas,
    })
}

/// «TARIFA PRECIOS LUXFIRE 2026» → («LUXFIRE», «2026»).
pub fn deducir_titulo(titulo: &str) -> (String, String) {
    let mut anio = String::new();
    let mut resto = Vec::new();
    for p in titulo.split_whitespace() {
        let n = normal(p);
        if p.len() == 4 && p.starts_with("20") && p.chars().all(|c| c.is_ascii_digit()) {
            anio = p.to_owned();
        } else if ![
            "tarifa", "tarifas", "precios", "precio", "de", "lista", "netos", "neto", "-",
        ]
        .contains(&n.as_str())
        {
            resto.push(p);
        }
    }
    (resto.join(" "), anio)
}

impl Tarifa {
    /// Da de alta la línea en el presupuesto como recurso de subcontrata.
    /// Si el código ya existe se reutiliza (con su precio actual) y se
    /// devuelve `false`.
    pub fn insertar_en(&self, l: &LineaTarifa, p: &mut Presupuesto) -> Result<(String, bool), ErrorTarifa> {
        let codigo = self.codigo(l);
        if p.conceptos.contains_key(&codigo) {
            return Ok((codigo, false));
        }
        p.nuevo_recurso(&codigo, &l.unidad, &l.descripcion, Naturaleza::Subcontrata, l.precio)?;
        Ok((codigo, true))
    }
}

/// Todas las tarifas del usuario.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Biblioteca {
    pub tarifas: Vec<Tarifa>,
}

/// Resultado de una búsqueda: índices de tarifa y de línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encontrada {
    pub tarifa: usize,
    pub linea: usize,
}

impl Biblioteca {
    /// Carga la biblioteca; si no existe o está dañada, vacía.
    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), ErrorTarifa> {
        if let Some(d) = ruta.parent() {
            std::fs::create_dir_all(d).map_err(|e| ErrorTarifa::Guardar(e.to_string()))?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| ErrorTarifa::Guardar(e.to_string()))?;
        std::fs::write(ruta, s).map_err(|e| ErrorTarifa::Guardar(e.to_string()))
    }

    /// Añade la tarifa; si ya hay una del mismo proveedor y vigencia, la
    /// sustituye. Devuelve `true` si sustituyó.
    pub fn anadir(&mut self, t: Tarifa) -> bool {
        let misma = |x: &Tarifa| x.prefijo() == t.prefijo() && x.vigencia == t.vigencia;
        let habia = self.tarifas.iter().any(misma);
        self.tarifas.retain(|x| !misma(x));
        self.tarifas.push(t);
        self.tarifas
            .sort_by(|a, b| (a.proveedor.as_str(), &b.vigencia).cmp(&(b.proveedor.as_str(), &a.vigencia)));
        habia
    }

    /// Busca en todas las tarifas por código o texto (sin tildes ni
    /// mayúsculas; todas las palabras deben aparecer).
    pub fn buscar(&self, texto: &str) -> Vec<Encontrada> {
        let palabras: Vec<String> = normal(texto).split_whitespace().map(str::to_owned).collect();
        let mut v = Vec::new();
        for (ti, t) in self.tarifas.iter().enumerate() {
            for (li, l) in t.lineas.iter().enumerate() {
                let h = normal(&format!(
                    "{} {} {} {}",
                    t.proveedor,
                    t.codigo(l),
                    l.descripcion,
                    t.especialidad
                ));
                if palabras.iter().all(|p| h.contains(p.as_str())) {
                    v.push(Encontrada { tarifa: ti, linea: li });
                }
            }
        }
        v
    }

    pub fn linea(&self, e: Encontrada) -> Option<(&Tarifa, &LineaTarifa)> {
        let t = self.tarifas.get(e.tarifa)?;
        Some((t, t.lineas.get(e.linea)?))
    }
}
