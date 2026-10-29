// SPDX-License-Identifier: GPL-3.0-or-later
//! Pedido a un fabricante o subcontrata en PDF (plantilla `pedido.typ`).

use crate::ErrorPdf;
use ppto_core::Presupuesto;
use ppto_core::formato::{cant, eur, num};
use ppto_core::pedidos::{Pedido, TipoPedido};
use serde_json::json;
use std::path::PathBuf;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

/// Plantilla del pedido incluida en el programa.
pub const PLANTILLA_PEDIDO: &str = include_str!("../plantillas/pedido.typ");

/// Quién pide y a quién.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DatosPedido {
    pub empresa: String,
    pub empresa_datos: String,
    pub logo: Option<PathBuf>,
    pub fecha: String,
    /// Referencia de obra o nº de pedido propio.
    pub referencia: String,
    /// Datos del proveedor: («CIF», «B12345678»)…
    pub proveedor: Vec<(String, String)>,
    pub condiciones: String,
}

/// Genera el PDF del pedido en memoria.
pub fn pdf_pedido(p: &Presupuesto, x: &Pedido, o: &DatosPedido) -> Result<Vec<u8>, ErrorPdf> {
    let dec = p.decimales;
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
    let material = x.tipo == TipoPedido::Material;
    let lineas: Vec<_> = x
        .lineas
        .iter()
        .map(|l| {
            json!({
                "codigo": l.codigo,
                "referencia": l.referencia.clone().unwrap_or_default(),
                "resumen": l.resumen,
                "unidad": l.unidad,
                "cantidad": cant(l.cantidad, dec.medicion),
                "pvp": l.pvp.map(eur).unwrap_or_default(),
                "descuento": l.descuento.map(|d| format!("{} %", num(d, 2))).unwrap_or_default(),
                "precio": eur(l.precio),
                "importe": eur(l.importe),
            })
        })
        .collect();
    let datos = json!({
        "titulo": format!("Pedido {}", x.nombre()),
        "tipo": if material { "Pedido de material" } else { "Pedido de subcontrata" },
        "material": material,
        "proveedor": x.nombre(),
        "datos_proveedor": o.proveedor.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| vec![k, v]).collect::<Vec<_>>(),
        "empresa": o.empresa,
        "empresa_datos": o.empresa_datos,
        "logo": logo,
        "obra": p.nombre,
        "fecha": o.fecha,
        "referencia": o.referencia,
        "lineas": lineas,
        "total": eur(x.total),
        "condiciones": o.condiciones.trim(),
    });
    ficheros.push(("datos.json".into(), serde_json::to_vec(&datos).unwrap_or_default()));
    let motor = TypstEngine::builder()
        .main_file(PLANTILLA_PEDIDO)
        .fonts(typst_assets::fonts())
        .with_static_file_resolver(ficheros.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
        .build();
    let doc: PagedDocument = motor.compile().output.map_err(|e| ErrorPdf::Plantilla(e.to_string()))?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|e| ErrorPdf::Plantilla(e.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("; ")))
}
