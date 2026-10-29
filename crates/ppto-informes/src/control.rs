// SPDX-License-Identifier: GPL-3.0-or-later
//! Informes de compras y control de obra en Excel: pedido a cada fabricante
//! o subcontrata, mano de obra prevista y coste de la obra frente a la venta.
//! Los datos salen de [`ppto_core::pedidos`].

use super::{ErrorInforme, Formatos, cabeceras, n};
use ppto_core::pedidos::{CosteObra, ManoObra, Pedido, TipoPedido};
use ppto_core::{Decimal, Presupuesto};
use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
use std::collections::BTreeSet;

/// Datos que encabezan un pedido.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CabeceraPedido {
    /// Fecha del pedido (texto libre, p. ej. «29/10/2026»).
    pub fecha: String,
    /// Datos del proveedor: («CIF», «B12345678»), («Contacto», …)…
    pub proveedor: Vec<(String, String)>,
    /// Condiciones o notas (plazo, portes, forma de pago…).
    pub condiciones: String,
}

/// Nombre de hoja válido y único (Excel: 31 caracteres, sin `[]:*?/\`).
fn nombre_hoja(t: &str, usados: &mut BTreeSet<String>) -> String {
    let base: String = t
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect::<String>()
        .trim_matches('\'')
        .chars()
        .take(28)
        .collect();
    let base = if base.trim().is_empty() {
        "Hoja".to_owned()
    } else {
        base
    };
    let mut nombre = base.clone();
    let mut i = 2;
    while !usados.insert(nombre.to_lowercase()) {
        nombre = format!("{base} {i}");
        i += 1;
    }
    nombre
}

fn hoja_pedido(
    h: &mut Worksheet,
    nombre: &str,
    p: &Presupuesto,
    x: &Pedido,
    cab: &CabeceraPedido,
    f: &Formatos,
) -> Result<(), XlsxError> {
    h.set_name(nombre)?;
    let titulo = match x.tipo {
        TipoPedido::Material => format!("Pedido de material — {}", x.nombre()),
        TipoPedido::Subcontrata => format!("Pedido de subcontrata — {}", x.nombre()),
    };
    h.write_string_with_format(0, 0, &titulo, &f.titulo)?;
    h.write_string_with_format(1, 0, format!("Obra: {}", p.nombre), &f.nota)?;
    let mut fila = 2;
    if !cab.fecha.is_empty() {
        h.write_string_with_format(fila, 0, format!("Fecha: {}", cab.fecha), &f.nota)?;
        fila += 1;
    }
    for (k, v) in cab.proveedor.iter().filter(|(_, v)| !v.trim().is_empty()) {
        h.write_string_with_format(fila, 0, format!("{k}: {v}"), &f.nota)?;
        fila += 1;
    }
    fila += 1;
    let material = x.tipo == TipoPedido::Material;
    let mut cols: Vec<(&str, f64)> = vec![
        ("Código", 14.0),
        ("Referencia", 14.0),
        ("Descripción", 50.0),
        ("Ud", 6.0),
        ("Cantidad", 11.0),
    ];
    if material {
        cols.extend([("PVP €", 11.0), ("Dto. %", 8.0)]);
    }
    cols.extend([
        ("Precio €/ud", 12.0),
        ("Importe €", 13.0),
        ("Oferta", 18.0),
        ("Partidas", 30.0),
    ]);
    cabeceras(h, fila, &cols, f)?;
    fila += 1;
    for l in &x.lineas {
        let mut c = 0u16;
        h.write_string(fila, c, &l.codigo)?;
        c += 1;
        h.write_string(fila, c, l.referencia.as_deref().unwrap_or(""))?;
        c += 1;
        h.write_string_with_format(fila, c, &l.resumen, &f.texto)?;
        c += 1;
        h.write_string(fila, c, &l.unidad)?;
        c += 1;
        h.write_number_with_format(fila, c, n(l.cantidad), &f.num3)?;
        c += 1;
        if material {
            if let Some(v) = l.pvp {
                h.write_number_with_format(fila, c, n(v), &f.num2)?;
            }
            if let Some(d) = l.descuento {
                h.write_number_with_format(fila, c + 1, n(d), &f.num2)?;
            }
            c += 2;
        }
        h.write_number_with_format(fila, c, n(l.precio), &f.num2)?;
        h.write_number_with_format(fila, c + 1, n(l.importe), &f.num2)?;
        h.write_string(fila, c + 2, l.oferta.as_deref().unwrap_or(""))?;
        h.write_string_with_format(fila, c + 3, l.partidas.join(", "), &f.texto)?;
        fila += 1;
    }
    let ci = if material { 8 } else { 6 };
    h.write_string_with_format(fila, 2, "Total pedido (sin IVA)", &f.total)?;
    h.write_number_with_format(fila, ci, n(x.total), &f.total_num)?;
    fila += 2;
    if !cab.condiciones.trim().is_empty() {
        h.write_string_with_format(fila, 0, "Condiciones:", &f.total)?;
        h.write_string_with_format(fila, 2, cab.condiciones.trim(), &f.texto)?;
        fila += 2;
    }
    h.write_string_with_format(
        fila,
        0,
        "Cantidades totales previstas en el presupuesto; revíselas antes de enviar el pedido.",
        &f.nota,
    )?;
    Ok(())
}

/// Pedido a un proveedor en un libro Excel.
pub fn excel_pedido(p: &Presupuesto, x: &Pedido, cab: &CabeceraPedido) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    hoja_pedido(libro.add_worksheet(), "Pedido", p, x, cab, &f)?;
    Ok(libro.save_to_buffer()?)
}

/// Todos los pedidos, uno por hoja, con una hoja de resumen al principio.
/// `cab` da la cabecera de cada proveedor.
pub fn excel_pedidos(
    p: &Presupuesto,
    pedidos: &[Pedido],
    cab: impl Fn(&Pedido) -> CabeceraPedido,
) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    let mut usados = BTreeSet::new();
    {
        let h = libro.add_worksheet();
        h.set_name(nombre_hoja("Resumen", &mut usados))?;
        h.write_string_with_format(0, 0, "Pedidos por proveedor", &f.titulo)?;
        h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
        cabeceras(
            h,
            3,
            &[
                ("Tipo", 13.0),
                ("Proveedor", 34.0),
                ("Líneas", 8.0),
                ("Importe €", 14.0),
            ],
            &f,
        )?;
        let mut fila = 4;
        for x in pedidos {
            h.write_string(fila, 0, x.tipo.texto())?;
            h.write_string(fila, 1, x.nombre())?;
            h.write_number(fila, 2, x.lineas.len() as f64)?;
            h.write_number_with_format(fila, 3, n(x.total), &f.num2)?;
            fila += 1;
        }
        let total: Decimal = pedidos.iter().map(|x| x.total).sum();
        h.write_string_with_format(fila, 1, "Total", &f.total)?;
        h.write_number_with_format(fila, 3, n(total), &f.total_num)?;
    }
    for x in pedidos {
        let nombre = nombre_hoja(x.nombre(), &mut usados);
        hoja_pedido(libro.add_worksheet(), &nombre, p, x, &cab(x), &f)?;
    }
    Ok(libro.save_to_buffer()?)
}

fn jornadas(h: Decimal) -> Decimal {
    (h / Decimal::from(8)).round_dp(1)
}

/// Mano de obra prevista: por oficio y por capítulo.
pub fn excel_mano_obra(p: &Presupuesto, mo: &ManoObra) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    let h = libro.add_worksheet();
    h.set_name("Mano de obra")?;
    h.write_string_with_format(0, 0, "Mano de obra prevista", &f.titulo)?;
    h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 14.0),
            ("Oficio", 50.0),
            ("Ud", 6.0),
            ("Cantidad", 12.0),
            ("Jornadas (8 h)", 14.0),
            ("Precio €", 11.0),
            ("Importe €", 13.0),
        ],
        &f,
    )?;
    let mut fila = 4;
    let bloque = |h: &mut Worksheet,
                  fila: &mut u32,
                  oficios: &[ppto_core::pedidos::Oficio],
                  horas: Decimal,
                  importe: Decimal,
                  titulo: &str|
     -> Result<(), XlsxError> {
        h.write_string_with_format(*fila, 0, titulo, &f.capitulo)?;
        for c in 1..7 {
            h.write_blank(*fila, c, &f.capitulo)?;
        }
        *fila += 1;
        for o in oficios {
            h.write_string(*fila, 0, &o.codigo)?;
            h.write_string(*fila, 1, &o.resumen)?;
            h.write_string(*fila, 2, &o.unidad)?;
            h.write_number_with_format(*fila, 3, n(o.cantidad), &f.num2)?;
            if o.en_horas() {
                h.write_number_with_format(*fila, 4, n(jornadas(o.cantidad)), &f.num2)?;
            }
            h.write_number_with_format(*fila, 5, n(o.precio), &f.num2)?;
            h.write_number_with_format(*fila, 6, n(o.importe), &f.num2)?;
            *fila += 1;
        }
        h.write_string_with_format(*fila, 1, "Total horas", &f.total)?;
        h.write_number_with_format(*fila, 3, n(horas), &f.total_num)?;
        h.write_number_with_format(*fila, 4, n(jornadas(horas)), &f.total_num)?;
        h.write_number_with_format(*fila, 6, n(importe), &f.total_num)?;
        *fila += 2;
        Ok(())
    };
    bloque(h, &mut fila, &mo.oficios, mo.horas, mo.importe, "TODA LA OBRA")?;
    for c in &mo.capitulos {
        let t = if c.capitulo.is_empty() {
            c.resumen.clone()
        } else {
            format!("{} {}", c.capitulo, c.resumen)
        };
        bloque(h, &mut fila, &c.oficios, c.horas, c.importe, &t)?;
    }
    h.write_string_with_format(
        fila,
        0,
        "Horas = cantidades de los recursos de mano de obra medidos en h; jornadas de 8 h.",
        &f.nota,
    )?;
    Ok(libro.save_to_buffer()?)
}

/// Coste de la obra por naturaleza y capítulo, frente a la venta.
pub fn excel_coste(p: &Presupuesto, c: &CosteObra) -> Result<Vec<u8>, ErrorInforme> {
    let f = Formatos::new();
    let mut libro = Workbook::new();
    let h = libro.add_worksheet();
    h.set_name("Coste de obra")?;
    h.write_string_with_format(0, 0, "Coste de la obra", &f.titulo)?;
    h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
    cabeceras(
        h,
        3,
        &[
            ("Capítulo", 12.0),
            ("Resumen", 40.0),
            ("Material €", 13.0),
            ("Mano de obra €", 13.0),
            ("Maquinaria €", 12.0),
            ("Subcontrata €", 13.0),
            ("Otros €", 11.0),
            ("Coste (PEM) €", 14.0),
            ("Venta €", 14.0),
            ("Beneficio €", 13.0),
            ("Margen s/venta", 12.0),
        ],
        &f,
    )?;
    let mut fila = 4;
    let fila_num = |h: &mut Worksheet,
                    fila: u32,
                    d: &ppto_core::pedidos::Desglose,
                    coste: Decimal,
                    venta: Decimal,
                    fmt: &rust_xlsxwriter::Format|
     -> Result<(), XlsxError> {
        for (i, v) in [
            d.material,
            d.mano_obra,
            d.maquinaria,
            d.subcontrata,
            d.otros,
            coste,
            venta,
            venta - coste,
        ]
        .into_iter()
        .enumerate()
        {
            h.write_number_with_format(fila, 2 + i as u16, n(v), fmt)?;
        }
        if !venta.is_zero() {
            h.write_number_with_format(fila, 10, n((venta - coste) / venta), &f.pct)?;
        }
        Ok(())
    };
    for x in &c.capitulos {
        h.write_string(fila, 0, &x.capitulo)?;
        h.write_string(fila, 1, &x.resumen)?;
        fila_num(h, fila, &x.desglose, x.coste, x.venta, &f.num2)?;
        fila += 1;
    }
    h.write_string_with_format(fila, 1, "Total obra", &f.total)?;
    fila_num(h, fila, &c.desglose, c.pem, c.venta, &f.total_num)?;
    fila += 2;
    for (t, v) in [
        ("Coste directo", c.coste_directo),
        ("Costes indirectos", c.costes_indirectos),
        ("Coste total (PEM)", c.pem),
        ("Venta (sin IVA)", c.venta),
        ("Beneficio", c.beneficio()),
    ] {
        h.write_string_with_format(fila, 1, t, &f.total)?;
        h.write_number_with_format(fila, 7, n(v), &f.num2)?;
        fila += 1;
    }
    fila += 1;
    h.write_string_with_format(
        fila,
        0,
        "Material, mano de obra… son costes directos; la diferencia con el PEM son los costes indirectos.",
        &f.nota,
    )?;
    Ok(libro.save_to_buffer()?)
}
