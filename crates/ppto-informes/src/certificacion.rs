// SPDX-License-Identifier: GPL-3.0-or-later
//! Relación valorada de una certificación en Excel.

use super::{ErrorInforme, Formatos, cabeceras, n};
use ppto_core::Presupuesto;
use ppto_core::seguimiento::{FilaRelacion, Relacion, ResumenCertificacion};
use rust_xlsxwriter::Workbook;

/// Relación valorada (una fila por partida, subtotales por capítulo) y
/// resumen con IVA y retención. `precios_venta`: sin líneas de GG y BI.
pub fn excel_certificacion(
    p: &Presupuesto,
    r: &Relacion,
    s: &ResumenCertificacion,
    precios_venta: bool,
) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    let h = libro.add_worksheet();
    h.set_name(format!("Certificación {}", r.numero))?;
    h.write_string_with_format(0, 0, format!("Certificación nº {} — {}", r.numero, r.fecha), &f.titulo)?;
    h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
    h.write_string_with_format(
        1,
        5,
        if r.cerrada {
            "Cerrada: precios congelados"
        } else {
            "Abierta (borrador)"
        },
        &f.nota,
    )?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 12.0),
            ("Descripción", 46.0),
            ("Ud", 5.0),
            ("Presupuesto", 11.0),
            ("Precio €", 11.0),
            ("Anterior", 10.0),
            ("Mes", 10.0),
            ("A origen", 10.0),
            ("Importe presupuesto €", 14.0),
            ("Importe anterior €", 14.0),
            ("Importe mes €", 14.0),
            ("Importe a origen €", 14.0),
            ("% a origen", 10.0),
        ],
        &f,
    )?;
    let mut fila = 4;
    for x in &r.filas {
        match x {
            FilaRelacion::Capitulo {
                codigo,
                resumen,
                nivel,
                importes,
            } => {
                h.write_string_with_format(fila, 0, codigo, &f.capitulo)?;
                h.write_string_with_format(fila, 1, format!("{}{resumen}", "  ".repeat(*nivel)), &f.capitulo)?;
                for c in 2..8 {
                    h.write_blank(fila, c, &f.capitulo)?;
                }
                for (i, v) in [
                    importes.presupuesto,
                    importes.anterior,
                    importes.actual,
                    importes.origen,
                ]
                .into_iter()
                .enumerate()
                {
                    h.write_number_with_format(fila, 8 + i as u16, n(v), &f.capitulo_num)?;
                }
                h.write_number_with_format(
                    fila,
                    12,
                    n(importes.porcentaje() / ppto_core::Decimal::ONE_HUNDRED),
                    &f.pct,
                )?;
            }
            FilaRelacion::Partida(x) => {
                h.write_string(fila, 0, &x.codigo)?;
                h.write_string(fila, 1, &x.resumen)?;
                h.write_string(fila, 2, &x.unidad)?;
                for (c, v) in [
                    (3, x.medicion),
                    (5, x.cant_anterior),
                    (6, x.cant_actual),
                    (7, x.cant_origen),
                ] {
                    h.write_number_with_format(fila, c, n(v), &f.num2)?;
                }
                h.write_number_with_format(fila, 4, n(x.precio), &f.num2)?;
                for (i, v) in [
                    x.importes.presupuesto,
                    x.importes.anterior,
                    x.importes.actual,
                    x.importes.origen,
                ]
                .into_iter()
                .enumerate()
                {
                    h.write_number_with_format(fila, 8 + i as u16, n(v), &f.num2)?;
                }
                h.write_number_with_format(
                    fila,
                    12,
                    n(x.importes.porcentaje() / ppto_core::Decimal::ONE_HUNDRED),
                    &f.pct,
                )?;
                if x.exceso {
                    h.write_string_with_format(fila, 13, "Supera el presupuesto", &f.nota)?;
                }
            }
        }
        fila += 1;
    }
    h.write_string_with_format(fila, 1, "Total", &f.total)?;
    for (i, v) in [r.total.presupuesto, r.total.anterior, r.total.actual, r.total.origen]
        .into_iter()
        .enumerate()
    {
        h.write_number_with_format(fila, 8 + i as u16, n(v), &f.total_num)?;
    }
    fila += 2;
    // Resumen
    h.write_string_with_format(fila, 1, "Resumen", &f.cabecera)?;
    for (c, t) in [(9, "Anterior €"), (10, "Este mes €"), (11, "A origen €")] {
        h.write_string_with_format(fila, c, t, &f.cabecera)?;
    }
    fila += 1;
    let mut lineas: Vec<(String, _, _, _)> = Vec::new();
    let hay_descuento = !s.origen.descuento.is_zero() || !s.anterior.descuento.is_zero();
    let etiqueta = p
        .seguimiento
        .descuento
        .as_ref()
        .map_or("Descuento comercial".to_owned(), |d| d.etiqueta());
    if precios_venta {
        lineas.push((
            "Importe certificado".into(),
            s.anterior.bruto,
            s.actual.bruto,
            s.origen.bruto,
        ));
        if hay_descuento {
            lineas.push((
                etiqueta,
                -s.anterior.descuento,
                -s.actual.descuento,
                -s.origen.descuento,
            ));
            lineas.push(("Base imponible".into(), s.anterior.base, s.actual.base, s.origen.base));
        }
    } else {
        lineas.push(("Ejecución material".into(), s.anterior.pem, s.actual.pem, s.origen.pem));
        lineas.push((
            "Gastos generales".into(),
            s.anterior.gastos_generales,
            s.actual.gastos_generales,
            s.origen.gastos_generales,
        ));
        lineas.push((
            "Beneficio industrial".into(),
            s.anterior.beneficio_industrial,
            s.actual.beneficio_industrial,
            s.origen.beneficio_industrial,
        ));
        if hay_descuento {
            lineas.push((
                etiqueta,
                -s.anterior.descuento,
                -s.actual.descuento,
                -s.origen.descuento,
            ));
        }
        lineas.push(("Base imponible".into(), s.anterior.base, s.actual.base, s.origen.base));
    }
    lineas.push(("IVA".into(), s.anterior.iva, s.actual.iva, s.origen.iva));
    lineas.push(("Total con IVA".into(), s.anterior.total, s.actual.total, s.origen.total));
    for (t, a, m, o) in lineas {
        h.write_string(fila, 1, &t)?;
        for (c, v) in [(9, a), (10, m), (11, o)] {
            h.write_number_with_format(fila, c, n(v), &f.num2)?;
        }
        fila += 1;
    }
    if !s.retencion.is_zero() {
        h.write_string(fila, 1, "Retención de garantía")?;
        h.write_number_with_format(fila, 10, n(-s.retencion), &f.num2)?;
        fila += 1;
    }
    h.write_string_with_format(fila, 1, "A cobrar este mes", &f.total)?;
    h.write_number_with_format(fila, 10, n(s.a_cobrar), &f.total_num)?;
    h.set_freeze_panes(4, 2)?;
    Ok(libro.save_to_buffer()?)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use ppto_core::seguimiento::Precios;
    use rust_decimal_macros::dec;

    #[test]
    fn excel_de_la_certificacion() {
        let mut probadas = 0;
        for semilla in 0..40 {
            let p = ppto_core::ejemplos::aleatorio(semilla, true);
            for c in &p.seguimiento.certificaciones {
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
                    let r = p.relacion_valorada(c.numero, pr).unwrap();
                    let s = p.resumen_certificacion(&r, pr, dec!(21), dec!(5));
                    let b = excel_certificacion(&p, &r, &s, venta).unwrap();
                    assert!(b.starts_with(b"PK"));
                    probadas += 1;
                }
            }
        }
        assert!(probadas > 4);
    }
}
