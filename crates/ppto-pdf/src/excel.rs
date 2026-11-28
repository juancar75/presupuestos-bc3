// SPDX-License-Identifier: GPL-3.0-or-later
//! Excel de la oferta con el **formato de Presto** («Presupuesto y
//! mediciones»: Código, Nat, Ud, Resumen, Comentario, N, Longitud, Anchura,
//! Altura, Parcial, CanPres, PrPres, ImpPres), para que el cliente lo meta en
//! su programa. Va con los **mismos precios que el PDF**
//! ([`crate::valoracion_cliente`]) y una hoja «Resumen» con el mismo
//! resumen (GG y BI si hay desglose, descuento, IVA e importe en letra sin
//! IVA). Nunca lleva descompuestos ni costes.
//!
//! Los importes son fórmulas como las de Presto (ImpPres =
//! REDONDEAR(CanPres × PrPres; 2); capítulos = suma de sus partidas), así
//! que el total que calcula Excel es el del PDF.

use crate::{ErrorPdf, OpcionesPdf, resumen_cliente, valoracion_cliente};
use ppto_core::formato::{en_letra, num};
use ppto_core::presupuesto::LineaValorada;
use ppto_core::{Decimal, Naturaleza, Presupuesto, TipoLinea, Valoracion};
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Formula, Workbook, Worksheet, XlsxError};
use std::path::Path;

const CABECERA: [&str; 13] = [
    "Código",
    "Nat",
    "Ud",
    "Resumen",
    "Comentario",
    "N",
    "Longitud",
    "Anchura",
    "Altura",
    "Parcial",
    "CanPres",
    "PrPres",
    "ImpPres",
];

fn f64_de(x: Decimal) -> f64 {
    x.to_f64().unwrap_or_default()
}

/// Columna en letra (0 → A).
fn col(c: u16) -> char {
    char::from(b'A' + u8::try_from(c).unwrap_or(0))
}

struct Formatos {
    cab: Format,
    capitulo: Format,
    partida: Format,
    texto: Format,
    cantidad: Format,
    dimension: Format,
    euros: Format,
    euros_neg: Format,
    total: Format,
    total_euros: Format,
    nota: Format,
}

impl Formatos {
    fn new(d: &ppto_core::Decimales) -> Self {
        let num = |n: u32| {
            if n == 0 {
                "#,##0".to_owned()
            } else {
                format!("#,##0.{}", "0".repeat(n as usize))
            }
        };
        let base = Format::new().set_font_name("Arial").set_font_size(9);
        Self {
            cab: base
                .clone()
                .set_bold()
                .set_background_color(Color::RGB(0xD9E1F2))
                .set_border_bottom(FormatBorder::Thin),
            capitulo: base.clone().set_bold().set_font_color(Color::RGB(0x1F3864)),
            partida: base.clone().set_bold(),
            texto: base.clone().set_text_wrap().set_align(FormatAlign::Top),
            cantidad: base.clone().set_num_format(num(d.medicion)),
            dimension: base.clone().set_num_format(num(d.dimensiones)),
            euros: base.clone().set_num_format(num(d.precio.max(2))),
            euros_neg: base.clone().set_num_format("#,##0.00;-#,##0.00"),
            total: base.clone().set_bold().set_border_top(FormatBorder::Thin),
            total_euros: base
                .clone()
                .set_bold()
                .set_border_top(FormatBorder::Thin)
                .set_num_format("#,##0.00"),
            nota: base.set_italic(),
        }
    }
}

struct Hoja<'a> {
    p: &'a Presupuesto,
    v: &'a Valoracion,
    f: Formatos,
    fila: u32,
}

impl Hoja<'_> {
    fn capitulo(&mut self, h: &mut Worksheet, l: &LineaValorada) -> Result<u32, ErrorPdf> {
        let c = self.p.concepto(&l.hijo)?;
        h.write_string_with_format(self.fila, 0, &c.codigo, &self.f.capitulo)?;
        h.write_string_with_format(self.fila, 1, "Capítulo", &self.f.capitulo)?;
        h.write_string_with_format(self.fila, 3, &c.resumen, &self.f.capitulo)?;
        self.fila += 2;
        let mut importes = Vec::new();
        for x in self.v.lineas.get(&c.codigo).into_iter().flatten() {
            let r = if self.p.concepto(&x.hijo)?.naturaleza == Naturaleza::Capitulo {
                self.capitulo(h, x)?
            } else {
                self.partida(h, &c.codigo, x)?
            };
            importes.push(format!("M{}", r + 1));
        }
        Ok(self.total(h, &c.codigo, &importes)?)
    }

    /// Fila de total de un capítulo (o de la obra): CanPres 1, PrPres = suma
    /// de sus importes. Devuelve la fila.
    fn total(&mut self, h: &mut Worksheet, codigo: &str, importes: &[String]) -> Result<u32, XlsxError> {
        let r = self.fila;
        let e = r + 1;
        h.write_string_with_format(r, 9, codigo, &self.f.total)?;
        h.write_number_with_format(r, 10, 1.0, &self.f.total)?;
        let suma = if importes.is_empty() {
            "=0".to_owned()
        } else {
            format!("={}", importes.join("+"))
        };
        h.write_formula_with_format(r, 11, Formula::new(suma), &self.f.total_euros)?;
        h.write_formula_with_format(r, 12, Formula::new(format!("=ROUND(K{e}*L{e},2)")), &self.f.total_euros)?;
        self.fila += 2;
        Ok(r)
    }

    /// Partida con su texto y su medición. Devuelve la fila de su importe.
    fn partida(&mut self, h: &mut Worksheet, capitulo: &str, l: &LineaValorada) -> Result<u32, ErrorPdf> {
        let p = self.p;
        let d = p.decimales;
        let c = p.concepto(&l.hijo)?;
        let fila_partida = self.fila;
        h.write_string_with_format(self.fila, 0, &c.codigo, &self.f.partida)?;
        h.write_string_with_format(self.fila, 1, "Partida", &self.f.partida)?;
        h.write_string_with_format(self.fila, 2, &c.unidad, &self.f.partida)?;
        h.write_string_with_format(self.fila, 3, &c.resumen, &self.f.partida)?;
        if let Some(rotulo) = c.rotulo_sin_importe() {
            h.write_string_with_format(self.fila, 4, rotulo, &self.f.nota)?;
        }
        self.fila += 1;
        if let Some(t) = c.texto.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            h.write_string_with_format(self.fila, 3, t, &self.f.texto)?;
            let lineas: u32 = t
                .lines()
                .map(|x| u32::try_from(x.chars().count()).unwrap_or(0) / 70 + 1)
                .sum();
            if lineas > 1 {
                h.set_row_height(self.fila, f64::from(12 * lineas.min(40)))?;
            }
            self.fila += 1;
        }
        let precio = if c.fuera_de_importe() { Decimal::ZERO } else { l.precio };
        let mut lineas_med = Vec::new();
        if let Some(m) = p.mediciones.get(&(capitulo.to_owned(), l.hijo.clone())) {
            let r = m.calcular(&d, &c.unidad)?;
            for (lm, parcial) in m.lineas.iter().zip(&r.parciales) {
                // Los subtotales no se repiten: Presto los recalcula y
                // sumarían dos veces.
                if matches!(lm.tipo, TipoLinea::SubtotalParcial | TipoLinea::SubtotalAcumulado) {
                    continue;
                }
                lineas_med.push((lm, *parcial));
            }
        }
        if lineas_med.is_empty() {
            // Sin medición: cantidad, precio e importe en la fila de la partida
            let e = fila_partida + 1;
            h.write_number_with_format(fila_partida, 10, f64_de(l.cantidad), &self.f.cantidad)?;
            h.write_number_with_format(fila_partida, 11, f64_de(precio), &self.f.euros)?;
            h.write_formula_with_format(
                fila_partida,
                12,
                Formula::new(format!("=ROUND(K{e}*L{e},2)")),
                &self.f.euros_neg,
            )?;
            self.fila += 1;
            return Ok(fila_partida);
        }
        for (lm, parcial) in lineas_med {
            let r = self.fila;
            let comentario = match (&lm.formula, lm.comentario.is_empty()) {
                (Some(fx), true) if lm.tipo == TipoLinea::Formula => fx.clone(),
                _ => lm.comentario.clone(),
            };
            h.write_string_with_format(r, 4, &comentario, &self.f.texto)?;
            for (k, v) in [lm.unidades, lm.longitud, lm.anchura, lm.altura]
                .into_iter()
                .enumerate()
            {
                let k = u16::try_from(k).unwrap_or(0) + 5;
                match v {
                    Some(x) => h.write_number_with_format(r, k, f64_de(x), &self.f.dimension)?,
                    None => h.write_number_with_format(r, k, 0.0, &self.f.dimension)?,
                };
            }
            h.write_number_with_format(r, 9, f64_de(parcial), &self.f.cantidad)?;
            self.fila += 1;
        }
        // Total de la partida: CanPres = la medición del programa (redondeada
        // como en el PDF), PrPres e ImpPres
        let t = self.fila;
        let e = t + 1;
        h.write_string_with_format(t, 9, &c.codigo, &self.f.total)?;
        h.write_number_with_format(t, 10, f64_de(l.cantidad), &self.f.cantidad)?;
        h.write_number_with_format(t, 11, f64_de(precio), &self.f.euros)?;
        h.write_formula_with_format(t, 12, Formula::new(format!("=ROUND(K{e}*L{e},2)")), &self.f.euros_neg)?;
        // La fila de la partida remite a su total, como en Presto
        h.write_formula_with_format(fila_partida, 10, Formula::new(format!("=K{e}")), &self.f.cantidad)?;
        h.write_formula_with_format(fila_partida, 11, Formula::new(format!("=L{e}")), &self.f.euros)?;
        self.fila += 2;
        Ok(t)
    }
}

/// Excel para el cliente con el formato del listado de Presto y los mismos
/// precios y resumen que el PDF de `o`.
pub fn excel_presto(p: &Presupuesto, o: &OpcionesPdf) -> Result<Vec<u8>, ErrorPdf> {
    let v = valoracion_cliente(p, o)?;
    let raiz = p.concepto(&p.raiz)?;
    let titulo = if o.titulo.trim().is_empty() {
        raiz.resumen.clone()
    } else {
        o.titulo.trim().to_owned()
    };
    let mut libro = Workbook::new();
    let mut hj = Hoja {
        p,
        v: &v,
        f: Formatos::new(&p.decimales),
        fila: 3,
    };
    let h = libro.add_worksheet();
    h.set_name("Presupuesto")?;
    h.write_string_with_format(0, 0, &titulo, &hj.f.capitulo)?;
    h.write_string_with_format(1, 0, "Presupuesto", &hj.f.partida)?;
    for (j, t) in CABECERA.iter().enumerate() {
        h.write_string_with_format(2, u16::try_from(j).unwrap_or(0), *t, &hj.f.cab)?;
    }
    for (j, w) in [12.0, 9.0, 6.0, 55.0, 22.0, 7.0, 9.0, 9.0, 9.0, 10.0, 10.0, 11.0, 13.0]
        .into_iter()
        .enumerate()
    {
        h.set_column_width(u16::try_from(j).unwrap_or(0), w)?;
    }
    h.set_freeze_panes(3, 0)?;
    let mut caps = Vec::new();
    let mut resumen_caps = Vec::new();
    for l in v.lineas.get(&p.raiz).into_iter().flatten() {
        let c = p.concepto(&l.hijo)?;
        let r = if c.naturaleza == Naturaleza::Capitulo {
            hj.capitulo(h, l)?
        } else {
            hj.partida(h, &p.raiz, l)?
        };
        caps.push(format!("M{}", r + 1));
        resumen_caps.push((c.codigo.clone(), c.resumen.clone(), r + 1));
    }
    let fila_total = hj.total(h, &p.raiz, &caps)? + 1;
    let f = hj.f;

    // Hoja «Resumen»: como el resumen del PDF, con fórmulas a la hoja anterior
    let (gg, bi, r) = resumen_cliente(p, o, v.pem);
    let s = libro.add_worksheet();
    s.set_name("Resumen")?;
    s.set_column_width(0, 12.0)?;
    s.set_column_width(1, 60.0)?;
    s.set_column_width(2, 15.0)?;
    s.write_string_with_format(0, 0, &titulo, &f.capitulo)?;
    s.write_string_with_format(1, 0, "Resumen de presupuesto", &f.partida)?;
    let mut fila = 3;
    for (codigo, resumen, rf) in &resumen_caps {
        s.write_string_with_format(fila, 0, codigo, &f.capitulo)?;
        s.write_string(fila, 1, resumen)?;
        s.write_formula_with_format(fila, 2, Formula::new(format!("=Presupuesto!M{rf}")), &f.euros_neg)?;
        fila += 1;
    }
    fila += 1;
    let c2 = col(2);
    let suma = fila;
    let etiqueta = if o.desglose_contrata {
        "Presupuesto de ejecución material"
    } else {
        "Suma"
    };
    s.write_string_with_format(fila, 1, etiqueta, &f.total)?;
    s.write_formula_with_format(
        fila,
        2,
        Formula::new(format!("=Presupuesto!M{fila_total}")),
        &f.total_euros,
    )?;
    fila += 1;
    let mut piezas = vec![format!("{c2}{}", suma + 1)];
    for (pct, txt) in [(gg, "Gastos generales"), (bi, "Beneficio industrial")] {
        if !pct.is_zero() {
            s.write_string(fila, 1, format!("{} % {txt}", num(pct, 2)))?;
            s.write_formula_with_format(
                fila,
                2,
                Formula::new(format!("=ROUND({c2}{}*{}/100,2)", suma + 1, pct)),
                &f.euros_neg,
            )?;
            piezas.push(format!("{c2}{}", fila + 1));
            fila += 1;
        }
    }
    if let Some(d) = p.seguimiento.descuento.as_ref().filter(|d| !d.es_nulo()) {
        s.write_string(fila, 1, d.etiqueta())?;
        s.write_number_with_format(fila, 2, f64_de(-r.descuento), &f.euros_neg)?;
        piezas.push(format!("{c2}{}", fila + 1));
        fila += 1;
    }
    let base = fila;
    let etiqueta_base = if gg.is_zero() && bi.is_zero() {
        "Base imponible"
    } else {
        "Presupuesto de ejecución por contrata"
    };
    s.write_string_with_format(fila, 1, etiqueta_base, &f.total)?;
    s.write_formula_with_format(fila, 2, Formula::new(format!("={}", piezas.join("+"))), &f.total_euros)?;
    fila += 1;
    if !o.iva.is_zero() {
        s.write_string(fila, 1, format!("{} % IVA", num(o.iva, 2)))?;
        s.write_formula_with_format(
            fila,
            2,
            Formula::new(format!("=ROUND({c2}{}*{}/100,2)", base + 1, o.iva)),
            &f.euros_neg,
        )?;
        fila += 1;
        s.write_string_with_format(fila, 1, "Total con IVA", &f.total)?;
        s.write_formula_with_format(
            fila,
            2,
            Formula::new(format!("={c2}{}+{c2}{}", base + 1, fila)),
            &f.total_euros,
        )?;
        fila += 1;
    }
    fila += 1;
    s.write_string_with_format(
        fila,
        0,
        format!(
            "Asciende el presupuesto a la expresada cantidad de {} (IVA no incluido).",
            en_letra(r.base)
        ),
        &f.nota,
    )?;
    Ok(libro.save_to_buffer()?)
}

/// Guarda [`excel_presto`] en `ruta`.
pub fn guardar_excel_presto(p: &Presupuesto, o: &OpcionesPdf, ruta: impl AsRef<Path>) -> Result<(), ErrorPdf> {
    let b = excel_presto(p, o)?;
    std::fs::write(ruta, b).map_err(ErrorPdf::Escritura)
}

impl From<XlsxError> for ErrorPdf {
    fn from(e: XlsxError) -> Self {
        ErrorPdf::Excel(e.to_string())
    }
}
