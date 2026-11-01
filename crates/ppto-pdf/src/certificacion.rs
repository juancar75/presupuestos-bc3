// SPDX-License-Identifier: GPL-3.0-or-later
//! Certificación de obra en PDF: relación valorada (anterior, mes y a
//! origen por partida) y resumen con IVA y retención, con firmas
//! (plantilla `certificacion.typ`).

use crate::ErrorPdf;
use ppto_core::Presupuesto;
use ppto_core::formato::{cant, eur, num};
use ppto_core::seguimiento::{FilaRelacion, Relacion, ResumenCertificacion};
use rust_decimal::Decimal;
use serde_json::json;
use std::path::PathBuf;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

/// Plantilla de la certificación incluida en el programa.
pub const PLANTILLA_CERTIFICACION: &str = include_str!("../plantillas/certificacion.typ");

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DatosCertificacion {
    pub empresa: String,
    pub empresa_datos: String,
    pub logo: Option<PathBuf>,
    /// Título de la obra.
    pub obra: String,
    pub cliente: String,
    pub referencia: String,
    /// Fecha de la certificación tal cual se imprime.
    pub fecha: String,
    /// Precios de venta (instaladora): sin líneas de GG y BI en el resumen.
    pub precios_venta: bool,
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    pub iva: Decimal,
    pub retencion: Decimal,
    /// Solo las partidas con algo certificado (a origen).
    pub solo_con_movimiento: bool,
}

/// Genera el PDF de la certificación en memoria.
pub fn pdf_certificacion(
    p: &Presupuesto,
    r: &Relacion,
    s: &ResumenCertificacion,
    o: &DatosCertificacion,
) -> Result<Vec<u8>, ErrorPdf> {
    let dec = p.decimales;
    let mut ficheros: Vec<(String, Vec<u8>)> = Vec::new();
    let mut logo = None;
    if let Some(ruta) = &o.logo {
        let ext = ruta
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| ["png", "jpg", "jpeg", "gif", "svg"].contains(&e.as_str()))
            .ok_or_else(|| ErrorPdf::FormatoLogo(ruta.clone()))?;
        let bytes = std::fs::read(ruta).map_err(|e| ErrorPdf::Fichero(ruta.clone(), e))?;
        let nombre = format!("logo.{ext}");
        ficheros.push((nombre.clone(), bytes));
        logo = Some(nombre);
    }
    let vacio = |x: Decimal, d: u32| if x.is_zero() { String::new() } else { cant(x, d) };
    let filas: Vec<_> = r
        .filas
        .iter()
        .filter(|f| match f {
            FilaRelacion::Partida(x) => !o.solo_con_movimiento || !x.cant_origen.is_zero(),
            FilaRelacion::Capitulo { importes, .. } => !o.solo_con_movimiento || !importes.origen.is_zero(),
        })
        .map(|f| match f {
            FilaRelacion::Capitulo {
                codigo,
                resumen,
                nivel,
                importes,
            } => json!({
                "capitulo": true,
                "nivel": nivel,
                "codigo": codigo,
                "resumen": resumen,
                "presupuesto": eur(importes.presupuesto),
                "anterior": eur(importes.anterior),
                "actual": eur(importes.actual),
                "origen": eur(importes.origen),
                "pct": format!("{} %", num(importes.porcentaje(), 1)),
            }),
            FilaRelacion::Partida(x) => json!({
                "capitulo": false,
                "codigo": x.codigo,
                "resumen": x.resumen,
                "unidad": x.unidad,
                "medicion": cant(x.medicion, dec.medicion),
                "precio": eur(x.precio),
                "c_anterior": vacio(x.cant_anterior, dec.medicion),
                "c_actual": vacio(x.cant_actual, dec.medicion),
                "c_origen": vacio(x.cant_origen, dec.medicion),
                "anterior": eur(x.importes.anterior),
                "actual": eur(x.importes.actual),
                "origen": eur(x.importes.origen),
                "pct": format!("{} %", num(x.importes.porcentaje(), 1)),
                "exceso": x.exceso,
            }),
        })
        .collect();
    let pct = |x: Decimal| num(x, 2);
    let mut resumen = Vec::new();
    let fila = |c: String, a: Decimal, m: Decimal, g: Decimal, fuerte: bool| json!({ "concepto": c, "anterior": eur(a), "actual": eur(m), "origen": eur(g), "fuerte": fuerte });
    let descuento = |resumen: &mut Vec<serde_json::Value>| {
        if !s.origen.descuento.is_zero() || !s.anterior.descuento.is_zero() {
            let etiqueta = p
                .seguimiento
                .descuento
                .as_ref()
                .map_or("Descuento comercial".to_owned(), |d| d.etiqueta());
            resumen.push(fila(
                etiqueta,
                -s.anterior.descuento,
                -s.actual.descuento,
                -s.origen.descuento,
                false,
            ));
            true
        } else {
            false
        }
    };
    if o.precios_venta {
        resumen.push(fila(
            "Importe certificado".into(),
            s.anterior.bruto,
            s.actual.bruto,
            s.origen.bruto,
            true,
        ));
        if descuento(&mut resumen) {
            resumen.push(fila(
                "Base imponible".into(),
                s.anterior.base,
                s.actual.base,
                s.origen.base,
                true,
            ));
        }
    } else {
        resumen.push(fila(
            "Ejecución material".into(),
            s.anterior.pem,
            s.actual.pem,
            s.origen.pem,
            true,
        ));
        resumen.push(fila(
            format!("Gastos generales {} %", pct(o.gastos_generales)),
            s.anterior.gastos_generales,
            s.actual.gastos_generales,
            s.origen.gastos_generales,
            false,
        ));
        resumen.push(fila(
            format!("Beneficio industrial {} %", pct(o.beneficio_industrial)),
            s.anterior.beneficio_industrial,
            s.actual.beneficio_industrial,
            s.origen.beneficio_industrial,
            false,
        ));
        descuento(&mut resumen);
        resumen.push(fila(
            "Base imponible".into(),
            s.anterior.base,
            s.actual.base,
            s.origen.base,
            true,
        ));
    }
    resumen.push(fila(
        format!("IVA {} %", pct(o.iva)),
        s.anterior.iva,
        s.actual.iva,
        s.origen.iva,
        false,
    ));
    resumen.push(fila(
        "Total con IVA".into(),
        s.anterior.total,
        s.actual.total,
        s.origen.total,
        true,
    ));
    let titulo = format!("Certificación nº {}", r.numero);
    let datos = json!({
        "titulo": titulo,
        "empresa": o.empresa,
        "empresa_datos": o.empresa_datos,
        "logo": logo,
        "obra": if o.obra.trim().is_empty() { p.nombre.clone() } else { o.obra.clone() },
        "cliente": o.cliente,
        "referencia": o.referencia,
        "fecha": o.fecha,
        "filas": filas,
        "presupuesto": eur(r.total.presupuesto),
        "anterior": eur(r.total.anterior),
        "actual": eur(r.total.actual),
        "origen": eur(r.total.origen),
        "pct": format!("{} %", num(r.total.porcentaje(), 1)),
        "resumen": resumen,
        "retencion": (!o.retencion.is_zero()).then(|| json!({
            "concepto": format!("Retención de garantía {} % s/ base del mes", pct(o.retencion)),
            "importe": eur(-s.retencion),
        })),
        "a_cobrar": eur(s.a_cobrar),
        "precios": if o.precios_venta { "Precios de venta del contrato, sin IVA." } else { "Precios de ejecución material." },
        "excesos": r.excesos().len(),
    });
    ficheros.push(("datos.json".into(), serde_json::to_vec(&datos).unwrap_or_default()));
    let motor = TypstEngine::builder()
        .main_file(PLANTILLA_CERTIFICACION)
        .fonts(typst_assets::fonts())
        .with_static_file_resolver(ficheros.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
        .build();
    let doc: PagedDocument = motor.compile().output.map_err(|e| ErrorPdf::Plantilla(e.to_string()))?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|e| ErrorPdf::Plantilla(e.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("; ")))
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use ppto_core::seguimiento::Precios;
    use rust_decimal_macros::dec;

    #[test]
    fn genera_la_certificacion() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        let n = p.nueva_certificacion("31/10/2026").unwrap();
        let pares: Vec<(String, String)> = p
            .conceptos
            .values()
            .filter(|c| c.naturaleza == ppto_core::Naturaleza::Capitulo)
            .flat_map(|c| c.descomposicion.iter().map(|l| (c.codigo.clone(), l.hijo.clone())))
            .filter(|(_, h)| p.conceptos[h].naturaleza != ppto_core::Naturaleza::Capitulo)
            .collect();
        for (c, h) in pares.iter().take(3) {
            p.certificar_completa(n, c, h).unwrap();
        }
        for venta in [false, true] {
            let pr = if venta {
                Precios::Venta {
                    gg: dec!(13),
                    bi: dec!(6),
                }
            } else {
                Precios::Ejecucion {
                    gg: dec!(13),
                    bi: dec!(6),
                }
            };
            let r = p.relacion_valorada(n, pr).unwrap();
            let s = p.resumen_certificacion(&r, pr, dec!(21), dec!(5));
            let o = DatosCertificacion {
                empresa: "Instalaciones Ejemplo, S.L.".into(),
                fecha: "31/10/2026".into(),
                precios_venta: venta,
                gastos_generales: dec!(13),
                beneficio_industrial: dec!(6),
                iva: dec!(21),
                retencion: dec!(5),
                solo_con_movimiento: venta,
                ..Default::default()
            };
            let b = pdf_certificacion(&p, &r, &s, &o).unwrap();
            assert!(b.starts_with(b"%PDF"));
        }
    }
}
