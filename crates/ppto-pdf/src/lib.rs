// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-pdf
//!
//! Presupuesto en PDF maquetado con [Typst](https://typst.app) (incluido en el
//! programa, sin instalar nada): portada con logo, fecha y revisión; capítulos
//! y subcapítulos con el texto de cada partida justificado y con partición
//! silábica en castellano; hoja de mediciones; resumen con GG, BI, IVA y total
//! en letra; firma y hoja final de observaciones.
//!
//! El diseño está en `plantillas/presupuesto.typ` y se puede sustituir por
//! una plantilla propia ([`OpcionesPdf::plantilla`]). Los datos le llegan como
//! `datos.json`, con los importes ya formateados (1.234,56) por el motor.

use ppto_core::formato::{en_letra, eur, num};
use ppto_core::presupuesto::LineaValorada;
use ppto_core::{Decimal, ErrorMotor, Naturaleza, Presupuesto, TipoLinea, venta};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

/// Plantilla incluida en el programa.
pub const PLANTILLA: &str = include_str!("../plantillas/presupuesto.typ");

/// Observaciones habituales que se proponen por defecto (editables).
pub const OBSERVACIONES_HABITUALES: [&str; 6] = [
    "El presente presupuesto tiene una validez de 30 días desde la fecha de emisión.",
    "Las mediciones son estimadas a partir de la documentación disponible; se certificará según la medición real ejecutada.",
    "No se incluyen ayudas de albañilería, tasas, licencias ni legalizaciones salvo que figuren expresamente en el presupuesto.",
    "Cualquier trabajo no contemplado en el presente presupuesto se valorará aparte, previa aceptación por escrito.",
    "Los plazos de suministro de equipos están sujetos a la disponibilidad de los fabricantes en el momento del pedido.",
    "Forma de pago: a convenir.",
];

#[derive(Debug, thiserror::Error)]
pub enum ErrorPdf {
    #[error(transparent)]
    Motor(#[from] ErrorMotor),
    #[error("no se pudo leer «{0}»: {1}")]
    Fichero(PathBuf, std::io::Error),
    #[error("el logo debe ser PNG, JPG, GIF o SVG: «{0}»")]
    FormatoLogo(PathBuf),
    #[error("error en la plantilla: {0}")]
    Plantilla(String),
    #[error("no se pudo escribir el PDF: {0}")]
    Escritura(std::io::Error),
}

/// Datos de la portada, el pie y el resumen. Se guardan entre sesiones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpcionesPdf {
    /// «Presupuesto», «Oferta», «Presupuesto de instalaciones»...
    pub tipo_documento: String,
    /// Vacío: el resumen de la raíz del presupuesto.
    pub titulo: String,
    pub direccion: String,
    pub cliente: String,
    /// Referencia de obra u oferta (p. ej. 2026OBM013).
    pub referencia: String,
    pub revision: String,
    /// Fecha tal cual se imprime («10 de octubre de 2026»).
    pub fecha: String,
    pub lugar: String,
    pub empresa: String,
    /// Segunda línea bajo la empresa (CIF, dirección, teléfono...).
    pub empresa_datos: String,
    pub autor: String,
    /// Segunda línea bajo el autor (titulación, n.º de colegiado...).
    pub autor_datos: String,
    pub logo: Option<PathBuf>,
    pub observaciones: Vec<String>,
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    pub iva: Decimal,
    pub mostrar_mediciones: bool,
    pub mostrar_textos: bool,
    /// Plantilla Typst propia; `None` usa [`PLANTILLA`].
    pub plantilla: Option<PathBuf>,
}

impl Default for OpcionesPdf {
    fn default() -> Self {
        Self {
            tipo_documento: "Presupuesto".into(),
            titulo: String::new(),
            direccion: String::new(),
            cliente: String::new(),
            referencia: String::new(),
            revision: "R0".into(),
            fecha: String::new(),
            lugar: String::new(),
            empresa: String::new(),
            empresa_datos: String::new(),
            autor: String::new(),
            autor_datos: String::new(),
            logo: None,
            observaciones: OBSERVACIONES_HABITUALES.iter().map(|s| (*s).to_owned()).collect(),
            gastos_generales: Decimal::ZERO,
            beneficio_industrial: Decimal::ZERO,
            iva: venta::IVA_GENERAL,
            mostrar_mediciones: true,
            mostrar_textos: true,
            plantilla: None,
        }
    }
}

/// Fecha en castellano: (10, 10, 2026) → «10 de octubre de 2026».
pub fn fecha_larga(dia: u32, mes: u32, anyo: i32) -> String {
    const M: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    let m = M.get(mes.saturating_sub(1) as usize).copied().unwrap_or("");
    format!("{dia} de {m} de {anyo}")
}

/// Fecha de hoy en castellano (según el reloj del sistema, UTC).
pub fn fecha_hoy() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (a, m, d) = civil(i64::try_from(segundos / 86_400).unwrap_or(0));
    fecha_larga(d, m, a)
}

/// Días desde 1970-01-01 → (año, mes, día). Algoritmo de H. Hinnant.
fn civil(dias: i64) -> (i32, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    (
        i32::try_from(a).unwrap_or(1970),
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Genera el PDF en memoria.
pub fn pdf(p: &Presupuesto, o: &OpcionesPdf) -> Result<Vec<u8>, ErrorPdf> {
    let plantilla = match &o.plantilla {
        Some(r) => std::fs::read_to_string(r).map_err(|e| ErrorPdf::Fichero(r.clone(), e))?,
        None => PLANTILLA.to_owned(),
    };
    let mut ficheros: Vec<(String, Vec<u8>)> = Vec::new();
    let mut logo = None;
    if let Some(r) = &o.logo {
        let ext = r
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| ["png", "jpg", "jpeg", "gif", "svg"].contains(&e.as_str()))
            .ok_or_else(|| ErrorPdf::FormatoLogo(r.clone()))?;
        let bytes = std::fs::read(r).map_err(|e| ErrorPdf::Fichero(r.clone(), e))?;
        let nombre = format!("logo.{ext}");
        ficheros.push((nombre.clone(), bytes));
        logo = Some(nombre);
    }
    let datos = datos(p, o, logo)?;
    ficheros.push(("datos.json".into(), serde_json::to_vec(&datos).unwrap_or_default()));

    let motor = TypstEngine::builder()
        .main_file(plantilla)
        .fonts(typst_assets::fonts())
        .with_static_file_resolver(ficheros.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
        .build();
    let doc: PagedDocument = motor.compile().output.map_err(|e| ErrorPdf::Plantilla(e.to_string()))?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|e| ErrorPdf::Plantilla(e.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("; ")))
}

/// Genera el PDF y lo guarda en `ruta`.
pub fn guardar_pdf(p: &Presupuesto, o: &OpcionesPdf, ruta: impl AsRef<Path>) -> Result<(), ErrorPdf> {
    let bytes = pdf(p, o)?;
    std::fs::write(ruta, bytes).map_err(ErrorPdf::Escritura)
}

/// Datos que recibe la plantilla (`datos.json`). Público para pruebas y para
/// quien quiera diseñar su propia plantilla.
pub fn datos(p: &Presupuesto, o: &OpcionesPdf, logo: Option<String>) -> Result<Value, ErrorPdf> {
    let v = p.valorar()?;
    let dec = p.decimales;
    let raiz = p.concepto(&p.raiz)?;
    let titulo = if o.titulo.trim().is_empty() {
        raiz.resumen.clone()
    } else {
        o.titulo.clone()
    };

    let ctx = Ctx { p, lineas: &v.lineas };
    let mut capitulos = Vec::new();
    let mut resumen = Vec::new();
    let mut sueltas = Vec::new();
    for l in v.lineas.get(&p.raiz).into_iter().flatten() {
        if p.concepto(&l.hijo)?.naturaleza == Naturaleza::Capitulo {
            capitulos.push(ctx.capitulo(l, 1, &mut resumen, v.pem)?);
        } else {
            sueltas.push(ctx.partida(&p.raiz, l)?);
        }
    }
    if !sueltas.is_empty() {
        // Partidas colgadas directamente de la raíz: se agrupan aparte
        let importe: Decimal = v.lineas[&p.raiz]
            .iter()
            .filter(|l| p.conceptos[&l.hijo].naturaleza != Naturaleza::Capitulo)
            .map(|l| l.importe)
            .sum();
        resumen.push(fila_resumen("—", "Partidas sin capítulo", importe, v.pem, 1));
        capitulos.push(json!({
            "tipo": "capitulo", "codigo": "", "resumen": "Partidas sin capítulo", "nivel": 1,
            "importe": eur(importe), "elementos": sueltas,
        }));
    }

    let r = venta::resumen(v.pem, o.gastos_generales, o.beneficio_industrial, o.iva, dec.importe);
    let mut recargos = Vec::new();
    if !o.gastos_generales.is_zero() {
        recargos.push(json!({
            "concepto": format!("{} % Gastos generales", num(o.gastos_generales, 2)),
            "importe": eur(r.gastos_generales),
        }));
    }
    if !o.beneficio_industrial.is_zero() {
        recargos.push(json!({
            "concepto": format!("{} % Beneficio industrial", num(o.beneficio_industrial, 2)),
            "importe": eur(r.beneficio_industrial),
        }));
    }
    let etiqueta_base = if recargos.is_empty() {
        "Base imponible"
    } else {
        "Presupuesto de ejecución por contrata"
    };
    let iva =
        (!o.iva.is_zero()).then(|| json!({ "concepto": format!("{} % IVA", num(o.iva, 2)), "importe": eur(r.iva) }));
    let lugar_fecha = match (o.lugar.trim(), o.fecha.trim()) {
        ("", f) => f.to_owned(),
        (l, "") => l.to_owned(),
        (l, f) => format!("{l}, {f}"),
    };

    Ok(json!({
        "tipo_documento": o.tipo_documento,
        "titulo": titulo,
        "direccion": o.direccion,
        "cliente": o.cliente,
        "referencia": if o.referencia.is_empty() { p.raiz.clone() } else { o.referencia.clone() },
        "revision": o.revision,
        "fecha": o.fecha,
        "lugar_fecha": lugar_fecha,
        "empresa": o.empresa,
        "empresa_datos": o.empresa_datos,
        "autor": o.autor,
        "autor_datos": o.autor_datos,
        "logo": logo,
        "mostrar_mediciones": o.mostrar_mediciones,
        "mostrar_textos": o.mostrar_textos,
        "capitulos": capitulos,
        "resumen": resumen,
        "pem": eur(r.pem),
        "recargos": recargos,
        "etiqueta_base": etiqueta_base,
        "base": eur(r.base),
        "iva": iva,
        "total": eur(r.total),
        "total_letra": en_letra(r.total),
        "observaciones": o.observaciones.iter().filter(|s| !s.trim().is_empty()).collect::<Vec<_>>(),
    }))
}

struct Ctx<'a> {
    p: &'a Presupuesto,
    lineas: &'a HashMap<String, Vec<LineaValorada>>,
}

impl Ctx<'_> {
    fn capitulo(
        &self,
        l: &LineaValorada,
        nivel: u32,
        resumen: &mut Vec<Value>,
        pem: Decimal,
    ) -> Result<Value, ErrorPdf> {
        let c = self.p.concepto(&l.hijo)?;
        resumen.push(fila_resumen(&c.codigo, &c.resumen, l.importe, pem, nivel));
        let mut elementos = Vec::new();
        for h in self.lineas.get(&c.codigo).into_iter().flatten() {
            if self.p.concepto(&h.hijo)?.naturaleza == Naturaleza::Capitulo {
                elementos.push(self.capitulo(h, nivel + 1, resumen, pem)?);
            } else {
                elementos.push(self.partida(&c.codigo, h)?);
            }
        }
        Ok(json!({
            "tipo": "capitulo",
            "codigo": c.codigo,
            "resumen": c.resumen,
            "nivel": nivel,
            "importe": eur(l.importe),
            "elementos": elementos,
        }))
    }

    fn partida(&self, capitulo: &str, l: &LineaValorada) -> Result<Value, ErrorPdf> {
        let p = self.p;
        let dec = p.decimales;
        let c = p.concepto(&l.hijo)?;
        let mut mediciones = Vec::new();
        if let Some(m) = p.mediciones.get(&(capitulo.to_owned(), l.hijo.clone())) {
            let r = m.calcular(&dec, &c.unidad)?;
            let op = |x: Option<Decimal>| x.map(|v| num(v, dec.dimensiones)).unwrap_or_default();
            for (lm, parcial) in m.lineas.iter().zip(&r.parciales) {
                let subtotal = matches!(lm.tipo, TipoLinea::SubtotalParcial | TipoLinea::SubtotalAcumulado);
                let comentario = match (&lm.formula, lm.comentario.is_empty()) {
                    (Some(f), true) if lm.tipo == TipoLinea::Formula => f.clone(),
                    _ => lm.comentario.clone(),
                };
                mediciones.push(json!({
                    "comentario": comentario,
                    "uds": op(lm.unidades),
                    "largo": op(lm.longitud),
                    "ancho": op(lm.anchura),
                    "alto": op(lm.altura),
                    "parcial": num(*parcial, dec.medicion),
                    "subtotal": subtotal,
                }));
            }
        }
        Ok(json!({
            "tipo": "partida",
            "codigo": c.codigo,
            "unidad": c.unidad,
            "resumen": c.resumen,
            "texto": c.texto.clone().unwrap_or_default(),
            "cantidad": num(l.cantidad, dec.medicion),
            "precio": eur(l.precio),
            "importe": eur(l.importe),
            "mediciones": mediciones,
        }))
    }
}

fn fila_resumen(codigo: &str, resumen: &str, importe: Decimal, pem: Decimal, nivel: u32) -> Value {
    let pct = if pem.is_zero() {
        Decimal::ZERO
    } else {
        importe * Decimal::ONE_HUNDRED / pem
    };
    json!({
        "codigo": codigo, "resumen": resumen, "importe": eur(importe),
        "pct": format!("{} %", num(pct, 2)), "nivel": nivel,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn calendario() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_736), (2026, 10, 10));
        assert_eq!(super::civil(19_782), (2024, 2, 29));
        assert_eq!(super::fecha_larga(1, 12, 2026), "1 de diciembre de 2026");
    }
}
