// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-ciego
//!
//! Presupuestos ciegos en Excel: el listado «Presupuesto y mediciones» que
//! exporta Presto (columnas Código, Nat, Ud, Resumen, Comentario, N,
//! Longitud, Anchura, Altura, Parcial, CanPres, PrPres, ImpPres) con los
//! precios a cero, o el de solo mediciones (Código, Nat, Ud, Resumen,
//! Comentario, Cantidad, CanPres), al que se añaden PrPres e ImpPres.
//!
//! 1. [`leer`] lo convierte en un [`Presupuesto`]: capítulos y subcapítulos,
//!    partidas con su texto y sus líneas de medición, y anota en qué celda
//!    va el precio de cada partida.
//! 2. Se valora en el programa (precio directo o descompuesto).
//! 3. [`valorar`] devuelve **el mismo fichero del cliente** con los precios
//!    escritos en la columna PrPres. Solo se tocan esas celdas: formato,
//!    fórmulas y el resto del libro quedan como estaban, y Excel recalcula
//!    los importes al abrirlo.

use calamine::{Data, Reader};
use ppto_core::{Decimal, ErrorMotor, LineaMedicion, Medicion, Naturaleza, Presupuesto, redondear};
use quick_xml::events::{BytesStart, BytesText, Event};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum ErrorCiego {
    #[error("no se pudo leer el Excel: {0}")]
    Lectura(String),
    #[error(
        "no se encuentra la fila de cabecera de Presto (Código, Nat, Ud, Resumen y CanPres o Cantidad) en ninguna hoja"
    )]
    SinCabecera,
    #[error(transparent)]
    Motor(#[from] ErrorMotor),
    #[error("solo se puede devolver valorado un fichero .xlsx (guarde el .xls como .xlsx en Excel)")]
    NoXlsx,
    #[error("no se pudo escribir el Excel valorado: {0}")]
    Escritura(String),
}

/// Celda de la hoja (fila y columna desde 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Celda {
    pub fila: u32,
    pub columna: u32,
}

impl Celda {
    /// Referencia estilo Excel: (10, 11) → «L11».
    pub fn referencia(self) -> String {
        let mut c = self.columna + 1;
        let mut letras = Vec::new();
        while c > 0 {
            let r = (c - 1) % 26;
            letras.push(char::from(b'A' + u8::try_from(r).unwrap_or(0)));
            c = (c - 1) / 26;
        }
        letras.reverse();
        format!("{}{}", letras.into_iter().collect::<String>(), self.fila + 1)
    }
}

/// Una aparición de una partida en un capítulo del ciego.
#[derive(Debug, Clone, PartialEq)]
pub struct Aparicion {
    pub capitulo: String,
    pub partida: String,
    /// Celda PrPres donde se escribe el precio.
    pub celda_precio: Celda,
    /// Cantidad exacta tal como la calcula Excel (sin redondear).
    pub cantidad_excel: Decimal,
}

#[derive(Debug, Clone)]
pub struct Ciego {
    pub presupuesto: Presupuesto,
    pub hoja: String,
    /// El Excel no traía columna de precio (solo mediciones): al valorar se
    /// añaden PrPres e ImpPres en esta fila de cabecera, detrás de la última.
    pub columnas_nuevas: Option<ColumnasNuevas>,
    pub apariciones: Vec<Aparicion>,
    pub incidencias: Vec<String>,
}

/// Columnas de precio que se añaden a un Excel de solo mediciones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnasNuevas {
    /// Fila de cabecera (desde 0).
    pub cabecera: u32,
    pub precio: u32,
    pub importe: u32,
    /// Columna CanPres, para la fórmula del importe.
    pub cantidad: u32,
}

/// Resultado de devolver el Excel valorado.
#[derive(Debug, Clone)]
pub struct Valorado {
    pub bytes: Vec<u8>,
    /// Total que mostrará Excel al recalcular (Σ REDONDEAR(precio × cantidad; 2)).
    pub total_excel: Decimal,
    /// Partidas que se quedan sin precio (0,00).
    pub sin_precio: Vec<String>,
    pub celdas_escritas: usize,
}

const COLUMNAS: [&str; 13] = [
    "código",
    "nat",
    "ud",
    "resumen",
    "comentario",
    "n",
    "longitud",
    "anchura",
    "altura",
    "parcial",
    "canpres",
    "prpres",
    "imppres",
];

/// Posición de cada columna de Presto en la hoja.
#[derive(Debug, Clone, Copy)]
struct Columnas([Option<u32>; 13]);

impl Columnas {
    fn de(fila: &[Data], inicio: u32) -> Option<Self> {
        let mut c = [None; 13];
        let rotulos: Vec<String> = fila.iter().map(|v| texto(v).trim().to_lowercase()).collect();
        let hay_canpres = rotulos.iter().any(|t| t == "canpres");
        let mut ultima = None;
        for (j, t) in rotulos.iter().enumerate() {
            // Otras exportaciones de Presto rotulan «Pres» en lugar de «PrPres»
            let t = match t.as_str() {
                "pres" | "precio" => "prpres",
                "importe" => "imppres",
                // Con CanPres, «Cantidad» es la de cada línea de medición
                "cantidad" if hay_canpres => "n",
                "cantidad" => "canpres",
                "naturaleza" => "nat",
                "codigo" => "código",
                otro => otro,
            };
            if let Some(k) = COLUMNAS.iter().position(|n| *n == t) {
                let col = inicio + u32::try_from(j).ok()?;
                c[k] = Some(col);
                ultima = Some(ultima.map_or(col, |u: u32| u.max(col)));
            }
        }
        // Imprescindibles: Código, Nat, Resumen y CanPres
        if ![0, 1, 3, 10].iter().all(|&k| c[k].is_some()) {
            return None;
        }
        // Mediciones sin columna de precio: se añaden PrPres e ImpPres detrás
        if c[11].is_none() {
            let u = ultima.unwrap_or(inicio);
            c[11] = Some(u + 1);
            c[12] = Some(u + 2);
        }
        Some(Self(c))
    }
}

/// Lee un ciego desde un fichero .xlsx o .xls.
pub fn leer(ruta: impl AsRef<Path>) -> Result<Ciego, ErrorCiego> {
    let ruta = ruta.as_ref();
    let bytes = std::fs::read(ruta).map_err(|e| ErrorCiego::Lectura(e.to_string()))?;
    let nombre = ruta
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    leer_bytes(&bytes, &nombre)
}

/// Lee un ciego desde memoria. `nombre` se usa si la hoja no trae título.
pub fn leer_bytes(bytes: &[u8], nombre: &str) -> Result<Ciego, ErrorCiego> {
    let mut libro =
        calamine::open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|e| ErrorCiego::Lectura(e.to_string()))?;
    for hoja in libro.sheet_names() {
        let Ok(rango) = libro.worksheet_range(&hoja) else {
            continue;
        };
        // Celdas con fórmula (para distinguir un Parcial calculado de uno fijo)
        let formulas: HashSet<(u32, u32)> = libro
            .worksheet_formula(&hoja)
            .map(|r| {
                r.cells()
                    .filter(|(_, _, f)| !f.is_empty())
                    .map(|(i, j, _)| {
                        let (a, b) = r.start().unwrap_or((0, 0));
                        (a + u32::try_from(i).unwrap_or(0), b + u32::try_from(j).unwrap_or(0))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let (f0, c0) = rango.start().unwrap_or((0, 0));
        let filas: Vec<&[Data]> = rango.rows().collect();
        let Some((i_cab, cols)) = filas
            .iter()
            .take(40)
            .enumerate()
            .find_map(|(i, f)| Columnas::de(f, c0).map(|c| (i, c)))
        else {
            continue;
        };
        // Título: el primer texto con sentido sobre la cabecera (no «B» ni
        // «Presupuesto», que pone Presto), o el nombre del fichero
        let titulo = filas
            .iter()
            .take(i_cab)
            .flat_map(|f| f.iter())
            .map(texto)
            .map(|t| t.trim().to_owned())
            .find(|t| {
                t.chars().count() > 2
                    && !matches!(
                        t.to_lowercase().as_str(),
                        "presupuesto" | "mediciones" | "presupuesto y mediciones" | "mediciones y presupuesto"
                    )
            })
            .unwrap_or_else(|| nombre.to_owned());
        let mut lector = Lector::new(&hoja, cols, c0, titulo);
        lector.formulas = formulas;
        let sin_precio = !filas[i_cab]
            .iter()
            .any(|v| matches!(texto(v).trim().to_lowercase().as_str(), "prpres" | "pres" | "precio"));
        let mut ciego = lector.leer(&filas, f0, i_cab + 1)?;
        if sin_precio {
            ciego.columnas_nuevas = Some(ColumnasNuevas {
                cabecera: f0 + u32::try_from(i_cab).unwrap_or(0),
                precio: cols.0[11].unwrap_or(0),
                importe: cols.0[12].unwrap_or(0),
                cantidad: cols.0[10].unwrap_or(0),
            });
            ciego.incidencias.push(
                "el Excel solo trae mediciones: al devolverlo valorado se añaden las columnas PrPres e ImpPres".into(),
            );
        }
        return Ok(ciego);
    }
    Err(ErrorCiego::SinCabecera)
}

/// Presupuesto con las mediciones a precisión completa, como las calcula el
/// Excel del cliente: Parcial = N × L × A × H sin redondear y Importe =
/// REDONDEAR(Precio × Cantidad; 2). Así el total del programa coincide al
/// céntimo con el del Excel valorado.
fn ciego_nuevo(nombre: String, raiz: &str, resumen: String) -> Presupuesto {
    let mut p = Presupuesto::new(nombre, raiz, resumen);
    p.decimales.dimensiones = 6;
    p.decimales.medicion = 6;
    p
}

struct PartidaAbierta {
    codigo: String,
    capitulo: String,
    fila: u32,
    cantidad_fila: Option<Decimal>,
    lineas: Vec<LineaMedicion>,
    exacta: Decimal,
    celda_total: Option<Celda>,
}

struct Lector {
    hoja: String,
    c: Columnas,
    c0: u32,
    p: Presupuesto,
    pila: Vec<String>,
    abierta: Option<PartidaAbierta>,
    apariciones: Vec<Aparicion>,
    incidencias: Vec<String>,
    formulas: HashSet<(u32, u32)>,
}

impl Lector {
    fn new(hoja: &str, c: Columnas, c0: u32, titulo: String) -> Self {
        let titulo = titulo.trim().to_owned();
        Self {
            hoja: hoja.to_owned(),
            c,
            c0,
            p: ciego_nuevo(titulo.clone(), "OBRA", titulo),
            pila: Vec::new(),
            abierta: None,
            apariciones: Vec::new(),
            incidencias: Vec::new(),
            formulas: HashSet::new(),
        }
    }

    fn valor<'a>(&self, fila: &'a [Data], k: usize) -> &'a Data {
        self.c.0[k]
            .and_then(|c| c.checked_sub(self.c0))
            .and_then(|j| fila.get(j as usize))
            .unwrap_or(&Data::Empty)
    }

    /// Código que cierra una fila de total: en Parcial (formato habitual de
    /// Presto) o, en otras exportaciones, «Total CÓDIGO» en la columna Resumen
    /// de una fila sin código ni naturaleza.
    fn total_de(&self, f: &[Data]) -> String {
        let parcial = texto(self.valor(f, 9)).trim().to_owned();
        if !parcial.is_empty() {
            return parcial;
        }
        if !texto(self.valor(f, 0)).trim().is_empty() || !texto(self.valor(f, 1)).trim().is_empty() {
            return String::new();
        }
        // «Total CÓDIGO» en Resumen o, en algunas exportaciones, en Cantidad
        [3, 5]
            .iter()
            .find_map(|&k| {
                texto(self.valor(f, k))
                    .trim()
                    .strip_prefix("Total ")
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty() && !c.contains(' '))
            })
            .unwrap_or_default()
    }

    fn leer(mut self, filas: &[&[Data]], f0: u32, desde: usize) -> Result<Ciego, ErrorCiego> {
        // Pasada previa: el código de la raíz es el de la última fila de total
        // que no corresponde a ningún capítulo (p. ej. «08_2024»).
        let codigos: HashSet<String> = filas
            .iter()
            .skip(desde)
            .map(|f| texto(self.valor(f, 0)).trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect();
        let raiz = filas
            .iter()
            .skip(desde)
            .rev()
            .map(|f| self.total_de(f))
            .find(|j| !j.is_empty() && !codigos.contains(j) && !j.starts_with('='));
        if let Some(r) = raiz.filter(|r| ppto_core::edicion::validar_codigo(r).is_ok()) {
            let resumen = self.p.concepto("OBRA")?.resumen.clone();
            self.p = ciego_nuevo(self.p.nombre.clone(), &r, resumen);
        }

        for (i, f) in filas.iter().enumerate().skip(desde) {
            let fila = f0 + u32::try_from(i).unwrap_or(u32::MAX);
            self.fila(f, fila)?;
        }
        self.cerrar_partida()?;
        if !self.pila.is_empty() {
            self.incidencias.push(format!(
                "capítulos sin fila de total al final de la hoja: {}",
                self.pila.join(", ")
            ));
        }
        // Formatos sin fila de total por partida: un solo aviso, no uno por partida
        let sin_total = self
            .incidencias
            .iter()
            .filter(|i| i.contains("medición sin fila de total"))
            .count();
        if sin_total > 3 {
            self.incidencias.retain(|i| !i.contains("medición sin fila de total"));
            self.incidencias.push(format!(
                "{sin_total} partidas sin fila de total: el precio se escribirá en la fila de cada partida"
            ));
        }
        self.p.validar()?;
        Ok(Ciego {
            presupuesto: self.p,
            hoja: self.hoja,
            columnas_nuevas: None,
            apariciones: self.apariciones,
            incidencias: self.incidencias,
        })
    }

    fn fila(&mut self, f: &[Data], fila: u32) -> Result<(), ErrorCiego> {
        let codigo = texto(self.valor(f, 0)).trim().to_owned();
        let nat = texto(self.valor(f, 1)).trim().to_lowercase();
        let resumen = texto(self.valor(f, 3)).trim().to_owned();
        let total_de = self.total_de(f);
        let dims: Vec<Option<Decimal>> = (5..=8).map(|k| decimal(self.valor(f, k))).collect();
        let col_precio = self.c.0[11].unwrap_or(0);

        if nat == "capítulo" || nat == "capitulo" {
            self.cerrar_partida()?;
            let padre = self.pila.last().cloned().unwrap_or_else(|| self.p.raiz.clone());
            if self.p.conceptos.contains_key(&codigo) {
                self.incidencias
                    .push(format!("fila {}: capítulo {codigo} repetido", fila + 1));
            } else {
                self.p.nuevo_capitulo(&padre, &codigo, &resumen)?;
            }
            self.pila.push(codigo);
            return Ok(());
        }
        if nat == "partida" {
            self.cerrar_partida()?;
            let Some(capitulo) = self.pila.last().cloned() else {
                self.incidencias.push(format!(
                    "fila {}: partida {codigo} fuera de capítulo; se ignora",
                    fila + 1
                ));
                return Ok(());
            };
            let unidad = texto(self.valor(f, 2)).trim().to_owned();
            // Resumen y texto en la misma celda: la 1.ª línea es el resumen
            let (resumen, texto_largo) = match resumen.split_once('\n') {
                Some((r, t)) if !t.trim().is_empty() => (r.trim().to_owned(), Some(t.trim().to_owned())),
                _ => (resumen, None),
            };
            if !self.p.conceptos.contains_key(&codigo) {
                self.p.nueva_partida(&capitulo, &codigo, &unidad, &resumen)?;
                if let Some(t) = texto_largo
                    && let Some(c) = self.p.conceptos.get_mut(&codigo)
                {
                    c.texto = Some(t);
                }
            } else if let Err(e) = self.p.anadir_linea(&capitulo, &codigo, Decimal::ONE) {
                self.incidencias.push(format!("fila {}: {e}", fila + 1));
                return Ok(());
            }
            self.abierta = Some(PartidaAbierta {
                codigo,
                capitulo,
                fila,
                cantidad_fila: decimal(self.valor(f, 10)),
                lineas: Vec::new(),
                exacta: Decimal::ZERO,
                celda_total: None,
            });
            return Ok(());
        }
        if !codigo.is_empty() || !nat.is_empty() {
            self.incidencias.push(format!(
                "fila {}: naturaleza «{nat}» no reconocida ({codigo}); se ignora",
                fila + 1
            ));
            return Ok(());
        }
        // Fila de total: en Parcial aparece el código de la partida o del
        // capítulo que se cierra (puede parecer un número: «03.01»).
        if !total_de.is_empty() {
            if let Some(a) = self.abierta.as_mut()
                && a.codigo == total_de
            {
                a.celda_total = Some(Celda {
                    fila,
                    columna: col_precio,
                });
                self.cerrar_partida()?;
                return Ok(());
            }
            if self.pila.last() == Some(&total_de) {
                self.cerrar_partida()?;
                self.pila.pop();
                return Ok(());
            }
            if total_de == self.p.raiz {
                self.cerrar_partida()?;
                return Ok(());
            }
        }
        let comentario = texto(self.valor(f, 4)).trim().to_owned();
        let es_formula = self.c.0[9].is_some_and(|j| self.formulas.contains(&(fila, j)));
        let parcial_celda = if es_formula { None } else { decimal(self.valor(f, 9)) };
        let Some(a) = self.abierta.as_mut() else {
            return Ok(());
        };
        let hay_numeros = dims.iter().any(Option::is_some);
        if !hay_numeros && comentario.is_empty() {
            // Texto descriptivo de la partida (columna Resumen)
            if !resumen.is_empty()
                && let Some(c) = self.p.conceptos.get_mut(&a.codigo)
                && c.texto.is_none()
            {
                c.texto = Some(resumen.trim_end().to_owned());
            }
            return Ok(());
        }
        // Línea de medición. Como Presto: Parcial = N × (L ó 1) × (A ó 1) × (H ó 1),
        // donde una dimensión 0 o vacía cuenta como 1 y N vacío como 0.
        let n = dims[0];
        let medida = |v: Option<Decimal>| v.filter(|x| !x.is_zero());
        let producto = n.unwrap_or_default()
            * dims[1..]
                .iter()
                .map(|v| medida(*v).unwrap_or(Decimal::ONE))
                .product::<Decimal>();
        // Si Parcial trae un valor fijo distinto del producto, la línea tiene
        // fórmula propia en Presto (p. ej. × 0,5): manda el Parcial.
        let parcial_fijo = parcial_celda
            .map(|v| v.round_dp(6).normalize())
            .filter(|v| (*v - producto).abs() > Decimal::new(1, 6));
        let linea = match parcial_fijo {
            Some(v) => LineaMedicion {
                tipo: ppto_core::TipoLinea::Formula,
                formula: Some(v.to_string()),
                ..LineaMedicion::normal(&comentario, n, medida(dims[1]), medida(dims[2]), medida(dims[3]))
            },
            None => LineaMedicion::normal(&comentario, n, medida(dims[1]), medida(dims[2]), medida(dims[3])),
        };
        a.exacta += parcial_fijo.unwrap_or(producto);
        a.lineas.push(linea);
        Ok(())
    }

    fn cerrar_partida(&mut self) -> Result<(), ErrorCiego> {
        let Some(a) = self.abierta.take() else {
            return Ok(());
        };
        let col_precio = self.c.0[11].unwrap_or(0);
        let (celda, exacta) = if a.lineas.is_empty() {
            let q = a.cantidad_fila.unwrap_or(Decimal::ONE);
            self.p.fijar_rendimiento(&a.capitulo, &a.codigo, q)?;
            (
                a.celda_total.unwrap_or(Celda {
                    fila: a.fila,
                    columna: col_precio,
                }),
                q,
            )
        } else {
            self.p
                .actualizar_medicion(&a.capitulo, &a.codigo, Medicion::new(a.lineas))?;
            let celda = a.celda_total.unwrap_or_else(|| {
                self.incidencias.push(format!(
                    "partida {}: medición sin fila de total; el precio se escribirá en su fila",
                    a.codigo
                ));
                Celda {
                    fila: a.fila,
                    columna: col_precio,
                }
            });
            (celda, a.exacta)
        };
        self.apariciones.push(Aparicion {
            capitulo: a.capitulo,
            partida: a.codigo,
            celda_precio: celda,
            cantidad_excel: exacta,
        });
        Ok(())
    }
}

fn texto(v: &Data) -> String {
    match v {
        Data::String(s) => s.clone(),
        Data::Float(f) => format!("{f}"),
        Data::Int(i) => i.to_string(),
        Data::Empty => String::new(),
        otro => otro.to_string(),
    }
}

/// Número de una celda sin pasar por aritmética de coma flotante.
fn decimal(v: &Data) -> Option<Decimal> {
    match v {
        Data::Float(f) => Decimal::from_str(&format!("{f}"))
            .or_else(|_| Decimal::from_scientific(&format!("{f:e}")))
            .ok(),
        Data::Int(i) => Some(Decimal::from(*i)),
        // Números guardados como texto: «3.2» (punto decimal, como los
        // escribe Presto) o «3,2»
        Data::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.starts_with('=') {
                None
            } else {
                Decimal::from_str(t).ok().or_else(|| ppto_core::formato::leer(t))
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- escritura

/// Devuelve el Excel original (`original`, .xlsx) con el precio de cada
/// partida de `p` escrito en su celda PrPres. Pide a Excel que recalcule
/// todas las fórmulas al abrirlo.
pub fn valorar(original: &[u8], ciego: &Ciego, p: &Presupuesto) -> Result<Valorado, ErrorCiego> {
    let v = p.valorar()?;
    // Precio de venta de cada partida en su capítulo (con costes indirectos)
    let mut precio: HashMap<(&str, &str), Decimal> = HashMap::new();
    for (cap, lineas) in &v.lineas {
        for l in lineas {
            precio.insert((cap.as_str(), l.hijo.as_str()), l.precio);
        }
    }
    let mut celdas: BTreeMap<String, Contenido> = BTreeMap::new();
    let mut total_excel = Decimal::ZERO;
    let mut sin_precio = Vec::new();
    for a in &ciego.apariciones {
        let pr = precio
            .get(&(a.capitulo.as_str(), a.partida.as_str()))
            .copied()
            .unwrap_or_default();
        if pr.is_zero() {
            sin_precio.push(a.partida.clone());
        }
        total_excel += redondear(pr * a.cantidad_excel, 2);
        celdas.insert(a.celda_precio.referencia(), Contenido::Numero(pr));
        if let Some(n) = ciego.columnas_nuevas {
            let fila = a.celda_precio.fila;
            let q = Celda {
                fila,
                columna: n.cantidad,
            }
            .referencia();
            let pr_ref = a.celda_precio.referencia();
            celdas.insert(
                Celda {
                    fila,
                    columna: n.importe,
                }
                .referencia(),
                Contenido::Formula(format!("ROUND({q}*{pr_ref},2)")),
            );
        }
    }
    if let Some(n) = ciego.columnas_nuevas {
        for (col, t) in [(n.precio, "PrPres"), (n.importe, "ImpPres")] {
            celdas.insert(
                Celda {
                    fila: n.cabecera,
                    columna: col,
                }
                .referencia(),
                Contenido::Texto(t.into()),
            );
        }
    }

    let mut zip = zip::ZipArchive::new(Cursor::new(original)).map_err(|_| ErrorCiego::NoXlsx)?;
    let ruta_hoja = ruta_de_hoja(&mut zip, &ciego.hoja)?;
    let mut salida = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut escritas = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        let nombre = f.name().to_owned();
        let opciones = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut contenido = Vec::new();
        f.read_to_end(&mut contenido)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        let contenido = if nombre == ruta_hoja {
            let (x, n) = escribir_celdas(&contenido, &celdas)?;
            escritas = n;
            x
        } else if nombre == "xl/workbook.xml" {
            recalcular_al_abrir(&contenido)?
        } else {
            contenido
        };
        salida
            .start_file(nombre, opciones)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
        salida
            .write_all(&contenido)
            .map_err(|e| ErrorCiego::Escritura(e.to_string()))?;
    }
    let bytes = salida
        .finish()
        .map_err(|e| ErrorCiego::Escritura(e.to_string()))?
        .into_inner();
    sin_precio.sort();
    sin_precio.dedup();
    Ok(Valorado {
        bytes,
        total_excel,
        sin_precio,
        celdas_escritas: escritas,
    })
}

fn leer_xml<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, nombre: &str) -> Result<Vec<u8>, ErrorCiego> {
    let mut f = zip.by_name(nombre).map_err(|_| ErrorCiego::NoXlsx)?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).map_err(|e| ErrorCiego::Lectura(e.to_string()))?;
    Ok(v)
}

fn atributo(e: &BytesStart, nombre: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == nombre || a.key.local_name().as_ref() == nombre)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

/// Ruta interna (xl/worksheets/sheetN.xml) de la hoja con ese nombre.
fn ruta_de_hoja<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, hoja: &str) -> Result<String, ErrorCiego> {
    let libro = leer_xml(zip, "xl/workbook.xml")?;
    let mut r = quick_xml::Reader::from_reader(libro.as_slice());
    let mut id = None;
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Empty(e) | Event::Start(e)) if e.local_name().as_ref() == b"sheet" => {
                if atributo(&e, b"name").as_deref() == Some(hoja) {
                    id = atributo(&e, b"r:id").or_else(|| atributo(&e, b"id"));
                    break;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    let id = id.ok_or(ErrorCiego::NoXlsx)?;
    let rels = leer_xml(zip, "xl/_rels/workbook.xml.rels")?;
    let mut r = quick_xml::Reader::from_reader(rels.as_slice());
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Empty(e) | Event::Start(e)) if e.local_name().as_ref() == b"Relationship" => {
                if atributo(&e, b"Id").as_deref() == Some(&id)
                    && let Some(t) = atributo(&e, b"Target")
                {
                    let t = t.trim_start_matches('/');
                    return Ok(if t.starts_with("xl/") {
                        t.to_owned()
                    } else {
                        format!("xl/{t}")
                    });
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    Err(ErrorCiego::NoXlsx)
}

/// Lo que se escribe en una celda.
#[derive(Debug, Clone, PartialEq)]
enum Contenido {
    Numero(Decimal),
    Texto(String),
    Formula(String),
}

/// Fila (desde 1) y columna (desde 0) de una referencia «H12».
fn fila_col(rf: &str) -> (u32, u32) {
    let letras: String = rf.chars().take_while(char::is_ascii_alphabetic).collect();
    let fila = rf[letras.len()..].parse().unwrap_or(0);
    let col = letras
        .bytes()
        .fold(0u32, |a, b| a * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1));
    (fila, col.saturating_sub(1))
}

/// Escribe una celda `<c>` con los atributos de `base` (estilo) y el contenido.
fn escribir_celda(
    w: &mut quick_xml::Writer<Vec<u8>>,
    rf: &str,
    base: Option<&BytesStart>,
    v: &Contenido,
) -> Result<(), ErrorCiego> {
    use quick_xml::events::BytesEnd;
    let err = |e: &dyn std::fmt::Display| ErrorCiego::Escritura(e.to_string());
    let mut c = BytesStart::new("c");
    match base {
        // fuera el tipo (t="s", t="str"...): lo pone el contenido nuevo
        Some(e) => {
            for a in e.attributes().flatten() {
                if a.key.as_ref() != b"t" {
                    c.push_attribute(a);
                }
            }
        }
        None => c.push_attribute(("r", rf)),
    }
    if matches!(v, Contenido::Texto(_)) {
        c.push_attribute(("t", "inlineStr"));
    }
    w.write_event(Event::Start(c)).map_err(|e| err(&e))?;
    let (abre, cierra_interior, texto) = match v {
        Contenido::Numero(n) => ("v", None, n.normalize().to_string()),
        Contenido::Formula(f) => ("f", None, f.clone()),
        Contenido::Texto(t) => ("is", Some("t"), t.clone()),
    };
    w.write_event(Event::Start(BytesStart::new(abre)))
        .map_err(|e| err(&e))?;
    if let Some(t) = cierra_interior {
        w.write_event(Event::Start(BytesStart::new(t))).map_err(|e| err(&e))?;
    }
    w.write_event(Event::Text(BytesText::new(&texto)))
        .map_err(|e| err(&e))?;
    if let Some(t) = cierra_interior {
        w.write_event(Event::End(BytesEnd::new(t))).map_err(|e| err(&e))?;
    }
    w.write_event(Event::End(BytesEnd::new(abre))).map_err(|e| err(&e))?;
    w.write_event(Event::End(BytesEnd::new("c"))).map_err(|e| err(&e))?;
    Ok(())
}

/// Sustituye el contenido de las celdas indicadas conservando su estilo. Las
/// que no existen (columnas nuevas, siempre a la derecha de las de la hoja)
/// se añaden al final de su fila. Devuelve el XML nuevo y cuántas celdas se
/// han escrito.
fn escribir_celdas(xml: &[u8], celdas: &BTreeMap<String, Contenido>) -> Result<(Vec<u8>, usize), ErrorCiego> {
    let err = |e: &dyn std::fmt::Display| ErrorCiego::Escritura(e.to_string());
    let mut r = quick_xml::Reader::from_reader(xml);
    let mut w = quick_xml::Writer::new(Vec::with_capacity(xml.len()));
    let mut buf = Vec::new();
    let mut saltar = 0usize; // profundidad dentro de una celda sustituida
    let mut escritas = 0;
    let mut hechas: HashSet<String> = HashSet::new();
    let mut fila_actual = 0u32;
    // Celdas pendientes de una fila, ordenadas por columna
    let pendientes = |fila: u32, hechas: &HashSet<String>| -> Vec<(u32, String)> {
        let mut v: Vec<(u32, String)> = celdas
            .keys()
            .filter(|k| !hechas.contains(*k))
            .map(|k| (fila_col(k), k.clone()))
            .filter(|((f, _), _)| *f == fila)
            .map(|((_, c), k)| (c, k))
            .collect();
        v.sort();
        v
    };
    loop {
        let ev = r.read_event_into(&mut buf).map_err(|e| err(&e))?;
        match ev {
            Event::Eof => break,
            _ if saltar > 0 => match ev {
                Event::Start(_) => saltar += 1,
                Event::End(_) => saltar -= 1,
                _ => {}
            },
            Event::Start(ref e) if e.local_name().as_ref() == b"row" => {
                fila_actual = atributo(e, b"r").and_then(|x| x.parse().ok()).unwrap_or(0);
                w.write_event(ev.borrow()).map_err(|e| err(&e))?;
            }
            Event::Empty(ref e) if e.local_name().as_ref() == b"row" => {
                let f: u32 = atributo(e, b"r").and_then(|x| x.parse().ok()).unwrap_or(0);
                let p = pendientes(f, &hechas);
                if p.is_empty() {
                    w.write_event(ev.borrow()).map_err(|e| err(&e))?;
                } else {
                    w.write_event(Event::Start(e.to_owned())).map_err(|e| err(&e))?;
                    for (_, k) in p {
                        escribir_celda(&mut w, &k, None, &celdas[&k])?;
                        hechas.insert(k);
                        escritas += 1;
                    }
                    w.write_event(Event::End(quick_xml::events::BytesEnd::new("row")))
                        .map_err(|e| err(&e))?;
                }
            }
            Event::End(ref e) if e.local_name().as_ref() == b"row" => {
                for (_, k) in pendientes(fila_actual, &hechas) {
                    escribir_celda(&mut w, &k, None, &celdas[&k])?;
                    hechas.insert(k);
                    escritas += 1;
                }
                w.write_event(ev.borrow()).map_err(|e| err(&e))?;
            }
            Event::Start(ref e) if e.local_name().as_ref() == b"c" => {
                match atributo(e, b"r").filter(|rf| celdas.contains_key(rf)) {
                    Some(rf) => {
                        escribir_celda(&mut w, &rf, Some(e), &celdas[&rf])?;
                        hechas.insert(rf);
                        escritas += 1;
                        saltar = 1;
                    }
                    None => w.write_event(ev.borrow()).map_err(|e| err(&e))?,
                }
            }
            Event::Empty(ref e) if e.local_name().as_ref() == b"c" => {
                match atributo(e, b"r").filter(|rf| celdas.contains_key(rf)) {
                    Some(rf) => {
                        escribir_celda(&mut w, &rf, Some(e), &celdas[&rf])?;
                        hechas.insert(rf);
                        escritas += 1;
                    }
                    None => w.write_event(ev.borrow()).map_err(|e| err(&e))?,
                }
            }
            _ => w.write_event(ev.borrow()).map_err(|e| err(&e))?,
        }
        buf.clear();
    }
    Ok((w.into_inner(), escritas))
}

/// Marca el libro para que Excel recalcule todas las fórmulas al abrirlo.
fn recalcular_al_abrir(xml: &[u8]) -> Result<Vec<u8>, ErrorCiego> {
    let err = |e: &dyn std::fmt::Display| ErrorCiego::Escritura(e.to_string());
    let mut r = quick_xml::Reader::from_reader(xml);
    let mut w = quick_xml::Writer::new(Vec::with_capacity(xml.len() + 40));
    let mut buf = Vec::new();
    let mut hecho = false;
    let calc = |e: Option<&BytesStart>| {
        let mut c = BytesStart::new("calcPr");
        if let Some(e) = e {
            for a in e.attributes().flatten() {
                if a.key.as_ref() != b"fullCalcOnLoad" {
                    c.push_attribute(a);
                }
            }
        }
        c.push_attribute(("fullCalcOnLoad", "1"));
        c
    };
    loop {
        let ev = r.read_event_into(&mut buf).map_err(|e| err(&e))?;
        match ev {
            Event::Eof => break,
            Event::Empty(ref e) if e.local_name().as_ref() == b"calcPr" => {
                w.write_event(Event::Empty(calc(Some(e)))).map_err(|e| err(&e))?;
                hecho = true;
            }
            Event::End(ref e) if e.local_name().as_ref() == b"workbook" && !hecho => {
                w.write_event(Event::Empty(calc(None))).map_err(|e| err(&e))?;
                w.write_event(ev.borrow()).map_err(|e| err(&e))?;
            }
            _ => w.write_event(ev.borrow()).map_err(|e| err(&e))?,
        }
        buf.clear();
    }
    Ok(w.into_inner())
}

/// Resultado de [`traer_precios`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Traspaso {
    /// Partidas que han recibido precio o descompuesto.
    pub con_precio: Vec<String>,
    /// Partidas que no están en el origen (siguen sin precio).
    pub no_encontradas: Vec<String>,
}

/// Da precio a todas las partidas sin descomponer de `p` que existan con el
/// mismo código en `origen` (otra obra, una base de precios o el mismo ciego
/// valorado en Presto y exportado a BC3).
pub fn traer_precios(p: &mut Presupuesto, origen: &Presupuesto) -> Traspaso {
    let mut t = Traspaso::default();
    let partidas: Vec<String> = p
        .conceptos
        .values()
        .filter(|c| c.naturaleza == Naturaleza::Partida && c.descomposicion.is_empty())
        .map(|c| c.codigo.clone())
        .collect();
    for codigo in partidas {
        if origen.conceptos.contains_key(&codigo) && p.traer_precio(origen, &codigo).is_ok() {
            t.con_precio.push(codigo);
        } else {
            t.no_encontradas.push(codigo);
        }
    }
    t
}
