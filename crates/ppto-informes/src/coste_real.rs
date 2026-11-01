// SPDX-License-Identifier: GPL-3.0-or-later
//! Coste real frente a previsto en Excel.

use super::{ErrorInforme, Formatos, cabeceras, n};
use ppto_core::Presupuesto;
use ppto_core::coste_real::{CosteReal, FilaCosteReal, Gasto, TipoGasto};
use rust_xlsxwriter::{Workbook, Worksheet};

fn fila(
    h: &mut Worksheet,
    r: u32,
    f: &FilaCosteReal,
    fmt: &rust_xlsxwriter::Format,
    pct: &rust_xlsxwriter::Format,
) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut c = 2u16;
    for t in TipoGasto::TODOS {
        h.write_number_with_format(r, c, n(f.previsto.de(t)), fmt)?;
        h.write_number_with_format(r, c + 1, n(f.real.de(t)), fmt)?;
        c += 2;
    }
    h.write_number_with_format(r, 10, n(f.previsto.total()), fmt)?;
    h.write_number_with_format(r, 11, n(f.real.total()), fmt)?;
    h.write_number_with_format(r, 12, n(f.venta), fmt)?;
    h.write_number_with_format(r, 13, n(f.certificado), fmt)?;
    h.write_number_with_format(r, 14, n(f.margen_previsto()), fmt)?;
    h.write_number_with_format(r, 15, n(f.margen_real()), fmt)?;
    if let Some(g) = f.gastado_pct() {
        h.write_number_with_format(r, 16, n(g / ppto_core::Decimal::ONE_HUNDRED), pct)?;
    }
    if let Some(x) = f.certificado_pct() {
        h.write_number_with_format(r, 17, n(x / ppto_core::Decimal::ONE_HUNDRED), pct)?;
    }
    Ok(())
}

/// Libro con el resumen por capítulo y la lista de gastos.
pub fn excel_coste_real(p: &Presupuesto, c: &CosteReal, gastos: &[Gasto]) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    let h = libro.add_worksheet();
    h.set_name("Coste real")?;
    h.write_string_with_format(0, 0, "Coste real frente a previsto", &f.titulo)?;
    h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
    let mut cab: Vec<(String, f64)> = vec![("Capítulo".into(), 12.0), ("Resumen".into(), 38.0)];
    for t in TipoGasto::TODOS {
        cab.push((format!("{} previsto €", t.nombre()), 14.0));
        cab.push((format!("{} real €", t.nombre()), 14.0));
    }
    for (t, a) in [
        ("Total previsto €", 14.0),
        ("Total real €", 14.0),
        ("Venta €", 14.0),
        ("Certificado €", 14.0),
        ("Margen previsto €", 14.0),
        ("Margen real €", 14.0),
        ("% gastado", 10.0),
        ("% certificado", 11.0),
    ] {
        cab.push((t.into(), a));
    }
    let cab: Vec<(&str, f64)> = cab.iter().map(|(t, a)| (t.as_str(), *a)).collect();
    cabeceras(h, 3, &cab, &f)?;
    let mut r = 4;
    for x in &c.capitulos {
        h.write_string(r, 0, &x.capitulo)?;
        h.write_string(r, 1, &x.resumen)?;
        fila(h, r, x, &f.num2, &f.pct)?;
        r += 1;
    }
    h.write_string_with_format(r, 1, "Total obra", &f.total)?;
    fila(h, r, &c.total, &f.total_num, &f.pct)?;
    r += 2;
    h.write_string_with_format(
        r,
        0,
        "Previsto = coste directo del presupuesto. Margen real = certificado a origen (precio de venta) − gastado.",
        &f.nota,
    )?;
    h.set_freeze_panes(4, 2)?;
    // Gastos
    let g = libro.add_worksheet();
    g.set_name("Gastos")?;
    cabeceras(
        g,
        0,
        &[
            ("Nº", 6.0),
            ("Fecha", 11.0),
            ("Tipo", 13.0),
            ("Proveedor", 28.0),
            ("Documento", 14.0),
            ("Concepto", 36.0),
            ("Capítulo", 10.0),
            ("Horas", 8.0),
            ("Importe €", 13.0),
            ("Pedido", 8.0),
        ],
        &f,
    )?;
    for (i, x) in gastos.iter().enumerate() {
        let r = i as u32 + 1;
        g.write_number(r, 0, f64::from(x.numero))?;
        g.write_string(r, 1, &x.fecha)?;
        g.write_string(r, 2, x.tipo.nombre())?;
        g.write_string(r, 3, &x.proveedor)?;
        g.write_string(r, 4, &x.documento)?;
        g.write_string(r, 5, &x.concepto)?;
        g.write_string(r, 6, x.capitulo.as_deref().unwrap_or("obra"))?;
        if let Some(hh) = x.horas {
            g.write_number_with_format(r, 7, n(hh), &f.num2)?;
        }
        g.write_number_with_format(r, 8, n(x.importe), &f.num2)?;
        if let Some(pn) = x.pedido {
            g.write_number(r, 9, f64::from(pn))?;
        }
    }
    g.set_freeze_panes(1, 0)?;
    Ok(libro.save_to_buffer()?)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn excel_del_coste_real() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        p.anotar_gasto(Gasto {
            numero: 0,
            fecha: "1".into(),
            tipo: TipoGasto::ManoObra,
            proveedor: "Operario".into(),
            documento: "Parte 1".into(),
            concepto: "Montaje".into(),
            capitulo: Some("C01".into()),
            horas: Some(dec!(8)),
            importe: dec!(200),
            pedido: None,
        })
        .unwrap();
        let c = p.coste_real(dec!(13), dec!(6)).unwrap();
        let b = excel_coste_real(&p, &c, &p.seguimiento.gastos).unwrap();
        assert!(b.starts_with(b"PK"));
    }
}
