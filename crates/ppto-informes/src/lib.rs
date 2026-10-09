// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-informes
//!
//! Informes del presupuesto. Primera entrega (P-014): libro Excel con
//!
//! 1. **Resumen** por capítulos con coste directo, indirectos, PEM, GG, BI e IVA.
//! 2. **Presupuesto** completo: capítulos y partidas con cantidad, precio e
//!    importe, agrupados por niveles (se pliegan con los botones de Excel).
//! 3. **Descompuestos**: líneas de cada precio.
//! 4. **Mediciones**: hojas de medición línea a línea.
//! 5. **Recursos**: cantidades totales por naturaleza.
//! 6. **Horas por oficio**: mano de obra con horas, coste y porcentaje.
//!
//! Todas las cifras salen de `ppto-core`: Excel no recalcula nada, de modo que
//! el informe coincide al céntimo con la pantalla.

use ppto_core::concepto::Naturaleza;
use ppto_core::presupuesto::LineaValorada;
use ppto_core::{Decimal, ErrorMotor, Presupuesto, Valoracion, medicion::TipoLinea, venta};
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};
use std::path::Path;

/// Porcentajes del resumen final (en puntos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpcionesInforme {
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    pub iva: Decimal,
}

impl Default for OpcionesInforme {
    fn default() -> Self {
        Self {
            gastos_generales: venta::GG_HABITUAL,
            beneficio_industrial: venta::BI_HABITUAL,
            iva: venta::IVA_GENERAL,
        }
    }
}

#[derive(Debug)]
pub enum ErrorInforme {
    Motor(ErrorMotor),
    Excel(XlsxError),
}

impl std::fmt::Display for ErrorInforme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Motor(e) => write!(f, "{e}"),
            Self::Excel(e) => write!(f, "no se pudo escribir el Excel: {e}"),
        }
    }
}

impl std::error::Error for ErrorInforme {}

impl From<ErrorMotor> for ErrorInforme {
    fn from(e: ErrorMotor) -> Self {
        Self::Motor(e)
    }
}

impl From<XlsxError> for ErrorInforme {
    fn from(e: XlsxError) -> Self {
        Self::Excel(e)
    }
}

/// Genera el libro Excel en memoria.
pub fn excel(p: &Presupuesto, opciones: &OpcionesInforme) -> Result<Vec<u8>, ErrorInforme> {
    let v = p.valorar()?;
    let f = Formatos::new();
    let mut libro = Workbook::new();
    hoja_resumen(libro.add_worksheet(), p, &v, opciones, &f)?;
    hoja_presupuesto(libro.add_worksheet(), p, &v, &f)?;
    hoja_descompuestos(libro.add_worksheet(), p, &v, &f)?;
    hoja_mediciones(libro.add_worksheet(), p, &f)?;
    let e = p.explosion_recursos()?;
    hoja_recursos(libro.add_worksheet(), p, &e, &f)?;
    hoja_horas(libro.add_worksheet(), p, &e, &f)?;
    Ok(libro.save_to_buffer()?)
}

/// Genera el libro Excel y lo guarda en `ruta`.
pub fn guardar_excel(p: &Presupuesto, opciones: &OpcionesInforme, ruta: impl AsRef<Path>) -> Result<(), ErrorInforme> {
    let bytes = excel(p, opciones)?;
    std::fs::write(ruta, bytes).map_err(|e| ErrorInforme::Excel(XlsxError::IoError(e)))
}

// ---------------------------------------------------------------------------

struct Formatos {
    titulo: Format,
    cabecera: Format,
    capitulo: Format,
    capitulo_num: Format,
    total: Format,
    total_num: Format,
    num2: Format,
    num3: Format,
    pct: Format,
    texto: Format,
    nota: Format,
    texto_largo: Format,
}

impl Formatos {
    fn new() -> Self {
        let gris = Color::RGB(0xEEEEEE);
        let azul = Color::RGB(0xDCE6F1);
        let base = Format::new().set_font_name("Calibri").set_font_size(10);
        Self {
            titulo: base.clone().set_bold().set_font_size(14),
            cabecera: base
                .clone()
                .set_bold()
                .set_background_color(gris)
                .set_border_bottom(FormatBorder::Thin),
            capitulo: base.clone().set_bold().set_background_color(azul),
            capitulo_num: base
                .clone()
                .set_bold()
                .set_background_color(azul)
                .set_num_format("#,##0.00"),
            total: base.clone().set_bold().set_border_top(FormatBorder::Thin),
            total_num: base
                .clone()
                .set_bold()
                .set_border_top(FormatBorder::Thin)
                .set_num_format("#,##0.00"),
            num2: base.clone().set_num_format("#,##0.00"),
            num3: base.clone().set_num_format("#,##0.000"),
            pct: base.clone().set_num_format("0.00\\ %"),
            texto: base.clone().set_text_wrap().set_align(FormatAlign::Top),
            texto_largo: base
                .clone()
                .set_text_wrap()
                .set_align(FormatAlign::Top)
                .set_font_size(9)
                .set_font_color(Color::RGB(0x444444)),
            nota: base.set_italic().set_font_color(Color::RGB(0x666666)),
        }
    }
}

fn n(v: Decimal) -> f64 {
    v.to_f64().unwrap_or_default()
}

fn encabezado(h: &mut Worksheet, nombre: &str, titulo: &str, p: &Presupuesto, f: &Formatos) -> Result<(), XlsxError> {
    h.set_name(nombre)?;
    h.write_string_with_format(0, 0, titulo, &f.titulo)?;
    h.write_string_with_format(1, 0, &p.nombre, &f.nota)?;
    Ok(())
}

fn cabeceras(h: &mut Worksheet, fila: u32, titulos: &[(&str, f64)], f: &Formatos) -> Result<(), XlsxError> {
    for (c, (t, ancho)) in titulos.iter().enumerate() {
        let c = c as u16;
        h.write_string_with_format(fila, c, *t, &f.cabecera)?;
        h.set_column_width(c, *ancho)?;
    }
    h.set_freeze_panes(fila + 1, 0)?;
    // Impresión: A4 apaisado, una página de ancho, cabecera repetida y pie con página.
    h.set_paper_size(9);
    h.set_landscape();
    h.set_print_fit_to_pages(1, 0);
    h.set_repeat_rows(0, fila)?;
    h.set_footer("&L&8presupuestos-bc3&R&8Página &P de &N");
    Ok(())
}

fn hoja_resumen(
    h: &mut Worksheet,
    p: &Presupuesto,
    v: &Valoracion,
    o: &OpcionesInforme,
    f: &Formatos,
) -> Result<(), ErrorInforme> {
    encabezado(h, "Resumen", "Resumen de presupuesto", p, f)?;
    cabeceras(
        h,
        3,
        &[("Código", 12.0), ("Capítulo", 55.0), ("Importe €", 16.0), ("%", 9.0)],
        f,
    )?;
    let mut fila = 4;
    fn capitulos(
        h: &mut Worksheet,
        p: &Presupuesto,
        v: &Valoracion,
        cap: &str,
        nivel: u8,
        fila: &mut u32,
        f: &Formatos,
    ) -> Result<(), XlsxError> {
        for l in v.lineas.get(cap).into_iter().flatten() {
            let c = &p.conceptos[&l.hijo];
            if c.naturaleza != Naturaleza::Capitulo {
                continue;
            }
            let sangria = f.num2.clone();
            let txt = Format::new().set_font_size(10).set_indent(nivel);
            h.write_string_with_format(*fila, 0, &c.codigo, &txt)?;
            h.write_string_with_format(*fila, 1, &c.resumen, &txt)?;
            h.write_number_with_format(*fila, 2, n(l.importe), &sangria)?;
            if !v.pem.is_zero() {
                h.write_number_with_format(*fila, 3, n(l.importe / v.pem), &f.pct)?;
            }
            *fila += 1;
            if nivel < 1 {
                capitulos(h, p, v, &l.hijo, nivel + 1, fila, f)?;
            }
        }
        Ok(())
    }
    capitulos(h, p, v, &p.raiz, 0, &mut fila, f)?;
    fila += 1;

    let r = venta::resumen(
        v.pem,
        o.gastos_generales,
        o.beneficio_industrial,
        o.iva,
        p.decimales.importe,
    );
    let mut filas: Vec<(String, Decimal, bool)> = Vec::new();
    if !p.costes_indirectos.is_zero() {
        filas.push(("Coste directo".into(), v.coste_directo, false));
        filas.push((
            format!("Costes indirectos ({} % por partida)", fmt_pct(p.costes_indirectos)),
            v.pem - v.coste_directo,
            false,
        ));
    }
    filas.push(("Presupuesto de ejecución material (PEM)".into(), r.pem, true));
    filas.push((
        format!("{} % Gastos generales", fmt_pct(o.gastos_generales)),
        r.gastos_generales,
        false,
    ));
    filas.push((
        format!("{} % Beneficio industrial", fmt_pct(o.beneficio_industrial)),
        r.beneficio_industrial,
        false,
    ));
    filas.push(("Presupuesto base (sin IVA)".into(), r.base, true));
    filas.push((format!("{} % IVA", fmt_pct(o.iva)), r.iva, false));
    filas.push(("TOTAL".into(), r.total, true));
    for (t, importe, fuerte) in filas {
        let (ft, fnum) = if fuerte {
            (&f.total, &f.total_num)
        } else {
            (&f.texto, &f.num2)
        };
        h.write_string_with_format(fila, 1, &t, ft)?;
        h.write_number_with_format(fila, 2, n(importe), fnum)?;
        fila += 1;
    }
    Ok(())
}

fn fmt_pct(v: Decimal) -> String {
    ppto_core::formato::num(v, 2)
}

fn hoja_presupuesto(h: &mut Worksheet, p: &Presupuesto, v: &Valoracion, f: &Formatos) -> Result<(), ErrorInforme> {
    encabezado(h, "Presupuesto", "Presupuesto", p, f)?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 12.0),
            ("Ud", 6.0),
            ("Resumen", 60.0),
            ("Cantidad", 12.0),
            ("Precio €", 12.0),
            ("Importe €", 14.0),
        ],
        f,
    )?;
    let mut fila = 4;
    rama(h, p, v, &p.raiz, 0, &mut fila, f)?;
    h.write_string_with_format(fila, 2, "TOTAL PRESUPUESTO DE EJECUCIÓN MATERIAL", &f.total)?;
    h.write_number_with_format(fila, 5, n(v.pem), &f.total_num)?;
    Ok(())
}

fn rama(
    h: &mut Worksheet,
    p: &Presupuesto,
    v: &Valoracion,
    cap: &str,
    nivel: u8,
    fila: &mut u32,
    f: &Formatos,
) -> Result<(), XlsxError> {
    for l in v.lineas.get(cap).into_iter().flatten() {
        let c = &p.conceptos[&l.hijo];
        if c.naturaleza == Naturaleza::Capitulo {
            h.write_string_with_format(*fila, 0, &c.codigo, &f.capitulo)?;
            h.write_string_with_format(*fila, 1, "", &f.capitulo)?;
            h.write_string_with_format(*fila, 2, &c.resumen, &f.capitulo)?;
            for col in 3..5 {
                h.write_string_with_format(*fila, col, "", &f.capitulo)?;
            }
            h.write_number_with_format(*fila, 5, n(l.importe), &f.capitulo_num)?;
            *fila += 1;
            let ini = *fila;
            rama(h, p, v, &l.hijo, nivel + 1, fila, f)?;
            h.write_string_with_format(*fila, 2, format!("Total {} {}", c.codigo, c.resumen), &f.total)?;
            h.write_number_with_format(*fila, 5, n(l.importe), &f.total_num)?;
            *fila += 1;
            if *fila > ini + 1 && nivel < 7 {
                h.group_rows(ini, *fila - 1)?;
            }
        } else {
            partida(h, c, l, *fila, f)?;
            *fila += 1;
            if let Some(t) = c.texto.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                h.write_string_with_format(*fila, 2, t, &f.texto_largo)?;
                // Altura aproximada: ~75 caracteres por línea (letra 9) en la columna de 60
                let lineas = t.lines().map(|l| l.chars().count() / 75 + 1).sum::<usize>().max(1);
                h.set_row_height(*fila, (lineas as u32 * 13 + 4) as u16)?;
                *fila += 1;
            }
        }
    }
    Ok(())
}

fn partida(
    h: &mut Worksheet,
    c: &ppto_core::Concepto,
    l: &LineaValorada,
    fila: u32,
    f: &Formatos,
) -> Result<(), XlsxError> {
    h.write_string(fila, 0, &c.codigo)?;
    h.write_string(fila, 1, &c.unidad)?;
    h.write_string_with_format(fila, 2, &c.resumen, &f.texto)?;
    h.write_number_with_format(fila, 3, n(l.cantidad), &f.num2)?;
    h.write_number_with_format(fila, 4, n(l.precio), &f.num2)?;
    h.write_number_with_format(fila, 5, n(l.importe), &f.num2)?;
    Ok(())
}

fn hoja_descompuestos(h: &mut Worksheet, p: &Presupuesto, v: &Valoracion, f: &Formatos) -> Result<(), ErrorInforme> {
    encabezado(h, "Descompuestos", "Precios descompuestos (coste directo)", p, f)?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 12.0),
            ("Ud", 6.0),
            ("Resumen", 55.0),
            ("Naturaleza", 13.0),
            ("Cantidad", 11.0),
            ("Precio €", 12.0),
            ("Importe €", 12.0),
        ],
        f,
    )?;
    let mut fila = 4;
    for c in p.conceptos.values() {
        if c.naturaleza == Naturaleza::Capitulo || c.descomposicion.is_empty() {
            continue;
        }
        let Some(lineas) = v.lineas.get(&c.codigo) else {
            continue;
        };
        h.write_string_with_format(fila, 0, &c.codigo, &f.capitulo)?;
        h.write_string_with_format(fila, 1, &c.unidad, &f.capitulo)?;
        h.write_string_with_format(fila, 2, &c.resumen, &f.capitulo)?;
        for col in 3..6 {
            h.write_string_with_format(fila, col, "", &f.capitulo)?;
        }
        h.write_number_with_format(
            fila,
            6,
            n(v.precios.get(&c.codigo).copied().unwrap_or_default()),
            &f.capitulo_num,
        )?;
        fila += 1;
        let ini = fila;
        for l in lineas {
            let hijo = &p.conceptos[&l.hijo];
            h.write_string(fila, 0, &hijo.codigo)?;
            h.write_string(fila, 1, &hijo.unidad)?;
            h.write_string(fila, 2, &hijo.resumen)?;
            h.write_string(fila, 3, naturaleza(hijo.naturaleza))?;
            h.write_number_with_format(fila, 4, n(l.cantidad), &f.num3)?;
            h.write_number_with_format(fila, 5, n(l.precio), &f.num2)?;
            h.write_number_with_format(fila, 6, n(l.importe), &f.num2)?;
            fila += 1;
        }
        h.group_rows(ini, fila - 1)?;
        fila += 1;
    }
    Ok(())
}

fn hoja_mediciones(h: &mut Worksheet, p: &Presupuesto, f: &Formatos) -> Result<(), ErrorInforme> {
    encabezado(h, "Mediciones", "Mediciones", p, f)?;
    cabeceras(
        h,
        3,
        &[
            ("Comentario", 45.0),
            ("Uds", 9.0),
            ("Longitud", 10.0),
            ("Anchura", 10.0),
            ("Altura", 10.0),
            ("Fórmula", 14.0),
            ("Parcial", 12.0),
        ],
        f,
    )?;
    let mut fila = 4;
    for ((padre, hijo), m) in &p.mediciones {
        let Some(c) = p.conceptos.get(hijo) else { continue };
        let r = m.calcular(&p.decimales, &c.unidad)?;
        h.write_string_with_format(fila, 0, format!("{hijo}  {}  ({padre})", c.resumen), &f.capitulo)?;
        for col in 1..6 {
            h.write_string_with_format(fila, col, "", &f.capitulo)?;
        }
        h.write_string_with_format(fila, 6, &c.unidad, &f.capitulo)?;
        fila += 1;
        let ini = fila;
        for (i, l) in m.lineas.iter().enumerate() {
            let sub = matches!(l.tipo, TipoLinea::SubtotalParcial | TipoLinea::SubtotalAcumulado);
            h.write_string(
                fila,
                0,
                if sub && l.comentario.is_empty() {
                    "Subtotal"
                } else {
                    &l.comentario
                },
            )?;
            for (col, d) in [(1u16, l.unidades), (2, l.longitud), (3, l.anchura), (4, l.altura)] {
                if let Some(d) = d {
                    h.write_number_with_format(fila, col, n(d), &f.num2)?;
                }
            }
            if let Some(fm) = &l.formula {
                h.write_string(fila, 5, fm)?;
            }
            let parcial = r.parciales.get(i).copied().unwrap_or_default();
            h.write_number_with_format(fila, 6, n(parcial), if sub { &f.total_num } else { &f.num2 })?;
            fila += 1;
        }
        h.write_string_with_format(fila, 0, "Total", &f.total)?;
        h.write_number_with_format(fila, 6, n(r.total), &f.total_num)?;
        fila += 1;
        h.group_rows(ini, fila - 1)?;
        fila += 1;
    }
    Ok(())
}

fn hoja_recursos(
    h: &mut Worksheet,
    p: &Presupuesto,
    e: &ppto_core::explosion::Explosion,
    f: &Formatos,
) -> Result<(), ErrorInforme> {
    encabezado(h, "Recursos", "Recursos necesarios (coste directo)", p, f)?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 12.0),
            ("Ud", 6.0),
            ("Resumen", 55.0),
            ("Cantidad", 12.0),
            ("Precio €", 12.0),
            ("Importe €", 14.0),
        ],
        f,
    )?;
    let mut fila = 4;
    for nat in ORDEN {
        let grupo: Vec<_> = e.por_naturaleza(nat).collect();
        if grupo.is_empty() {
            continue;
        }
        let subtotal: Decimal = grupo.iter().map(|r| r.importe).sum();
        h.write_string_with_format(fila, 0, naturaleza(nat), &f.capitulo)?;
        for col in 1..5 {
            h.write_string_with_format(fila, col, "", &f.capitulo)?;
        }
        h.write_number_with_format(fila, 5, n(subtotal), &f.capitulo_num)?;
        fila += 1;
        for r in grupo {
            h.write_string(fila, 0, &r.codigo)?;
            h.write_string(fila, 1, &r.unidad)?;
            h.write_string(fila, 2, &r.resumen)?;
            if nat != Naturaleza::Porcentaje {
                h.write_number_with_format(fila, 3, n(r.cantidad), &f.num3)?;
                h.write_number_with_format(fila, 4, n(r.precio), &f.num2)?;
            }
            h.write_number_with_format(fila, 5, n(r.importe), &f.num2)?;
            fila += 1;
        }
        fila += 1;
    }
    h.write_string_with_format(fila, 2, "Total recursos", &f.total)?;
    h.write_number_with_format(fila, 5, n(e.total), &f.total_num)?;
    fila += 1;
    h.write_string(fila, 2, "Coste directo de la obra")?;
    h.write_number_with_format(fila, 5, n(e.coste_directo), &f.num2)?;
    fila += 1;
    h.write_string_with_format(
        fila,
        2,
        "Descuadre de redondeo (coste directo − total recursos)",
        &f.nota,
    )?;
    h.write_number_with_format(fila, 5, n(e.descuadre_redondeo), &f.num2)?;
    Ok(())
}

fn hoja_horas(
    h: &mut Worksheet,
    p: &Presupuesto,
    e: &ppto_core::explosion::Explosion,
    f: &Formatos,
) -> Result<(), ErrorInforme> {
    encabezado(h, "Horas por oficio", "Horas previstas por oficio", p, f)?;
    cabeceras(
        h,
        3,
        &[
            ("Código", 12.0),
            ("Oficio", 50.0),
            ("Ud", 6.0),
            ("Horas", 12.0),
            ("Precio €/h", 12.0),
            ("Importe €", 14.0),
            ("% horas", 10.0),
        ],
        f,
    )?;
    let mo: Vec<_> = e.por_naturaleza(Naturaleza::ManoObra).collect();
    let horas: Decimal = mo.iter().map(|r| r.cantidad).sum();
    let importe: Decimal = mo.iter().map(|r| r.importe).sum();
    let mut fila = 4;
    for r in &mo {
        h.write_string(fila, 0, &r.codigo)?;
        h.write_string(fila, 1, &r.resumen)?;
        h.write_string(fila, 2, &r.unidad)?;
        h.write_number_with_format(fila, 3, n(r.cantidad), &f.num2)?;
        h.write_number_with_format(fila, 4, n(r.precio), &f.num2)?;
        h.write_number_with_format(fila, 5, n(r.importe), &f.num2)?;
        if !horas.is_zero() {
            h.write_number_with_format(fila, 6, n(r.cantidad / horas), &f.pct)?;
        }
        fila += 1;
    }
    h.write_string_with_format(fila, 1, "Total mano de obra", &f.total)?;
    h.write_number_with_format(fila, 3, n(horas), &f.total_num)?;
    h.write_number_with_format(fila, 5, n(importe), &f.total_num)?;
    fila += 2;
    h.write_string_with_format(
        fila,
        1,
        "Se suman las unidades de cada recurso de mano de obra (normalmente horas).",
        &f.nota,
    )?;
    Ok(())
}

const ORDEN: [Naturaleza; 6] = [
    Naturaleza::ManoObra,
    Naturaleza::Material,
    Naturaleza::Maquinaria,
    Naturaleza::Subcontrata,
    Naturaleza::Otros,
    Naturaleza::Porcentaje,
];

fn naturaleza(n: Naturaleza) -> &'static str {
    match n {
        Naturaleza::Capitulo => "Capítulo",
        Naturaleza::Partida => "Partida",
        Naturaleza::ManoObra => "Mano de obra",
        Naturaleza::Maquinaria => "Maquinaria",
        Naturaleza::Material => "Material",
        Naturaleza::Subcontrata => "Subcontrata",
        Naturaleza::Porcentaje => "Porcentaje",
        Naturaleza::Otros => "Otros",
    }
}
