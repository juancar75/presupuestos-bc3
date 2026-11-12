// SPDX-License-Identifier: GPL-3.0-or-later
//! Presupuesto ciego sintético con la estructura exacta del listado
//! «Presupuesto y mediciones» de Presto exportado a Excel.

use calamine::{Data, Reader};
use ppto_core::Naturaleza;
use rust_decimal_macros::dec;
use rust_xlsxwriter::{Formula, Workbook};

/// Escribe el ciego. Filas (1-based como en Excel):
/// C01 > C01.01 > P1 (m, con medición: 2 × 10,5 + 1 × 3,25 × 2 + fijo 0,875) y
/// P2 (u, sin medición: 7); C02 con P1 otra vez (4 m) y P3 con números en texto.
fn ciego() -> Vec<u8> {
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    h.set_name("Hoja1").unwrap();
    let cab = [
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
    h.write(0, 0, "OBRA SINTÉTICA DE PRUEBA").unwrap();
    h.write(1, 0, "Presupuesto").unwrap();
    for (j, t) in cab.iter().enumerate() {
        h.write(2, j as u16, *t).unwrap();
    }
    let s = |h: &mut rust_xlsxwriter::Worksheet, f: u32, c: u16, v: &str| {
        h.write(f, c, v).unwrap();
    };
    let n = |h: &mut rust_xlsxwriter::Worksheet, f: u32, c: u16, v: f64| {
        h.write(f, c, v).unwrap();
    };
    let fo = |h: &mut rust_xlsxwriter::Worksheet, f: u32, c: u16, v: &str| {
        h.write_formula(f, c, Formula::new(v)).unwrap();
    };
    let parcial = |h: &mut rust_xlsxwriter::Worksheet, f: u32| {
        let r = f + 1;
        h.write_formula(
            f,
            9,
            Formula::new(format!("=F{r}*(G{r}+(G{r}=0))*(H{r}+(H{r}=0))*(I{r}+(I{r}=0))")),
        )
        .unwrap();
    };
    // fila 4: C01
    s(h, 3, 0, "C01          ");
    s(h, 3, 1, "Capítulo");
    s(h, 3, 3, "OBRA CIVIL");
    s(h, 5, 0, "C01.01       ");
    s(h, 5, 1, "Capítulo");
    s(h, 5, 3, "ZANJAS");
    // P1 con medición
    s(h, 7, 0, "P1           ");
    s(h, 7, 1, "Partida");
    s(h, 7, 2, "m");
    s(h, 7, 3, "ZANJA");
    fo(h, 7, 10, "=K13");
    fo(h, 7, 11, "=L13");
    s(h, 8, 3, "Excavación de zanja por medios mecánicos, incluso carga.\n");
    s(h, 9, 4, "Tramo A");
    n(h, 9, 5, 2.0);
    n(h, 9, 6, 10.5);
    n(h, 9, 7, 0.0);
    n(h, 9, 8, 0.0);
    parcial(h, 9);
    s(h, 10, 4, "Tramo B");
    n(h, 10, 5, 1.0);
    n(h, 10, 6, 3.25);
    n(h, 10, 7, 2.0);
    n(h, 10, 8, 0.0);
    parcial(h, 10);
    // línea con parcial fijo (fórmula propia en Presto): 1 × 3,5 × 0,25 = 0,875
    s(h, 11, 4, "Ajuste");
    n(h, 11, 5, 1.0);
    n(h, 11, 6, 3.5);
    n(h, 11, 7, 0.0);
    n(h, 11, 8, 0.0);
    n(h, 11, 9, 0.875);
    n(h, 11, 10, 0.25);
    s(h, 12, 9, "P1");
    fo(h, 12, 10, "=SUM(J10:J12)");
    n(h, 12, 11, 0.0);
    fo(h, 12, 12, "=ROUND(L13*K13,2)");
    // P2 sin medición: cantidad y precio en su propia fila
    s(h, 14, 0, "P2           ");
    s(h, 14, 1, "Partida");
    s(h, 14, 2, "u");
    s(h, 14, 3, "ARQUETA");
    n(h, 14, 10, 7.0);
    n(h, 14, 11, 0.0);
    fo(h, 14, 12, "=ROUND(K15*L15,2)");
    s(h, 15, 3, "Arqueta registrable 40×40 cm.");
    s(h, 16, 9, "C01.01");
    n(h, 16, 10, 1.0);
    fo(h, 16, 11, "=M13+M15");
    fo(h, 16, 12, "=ROUND(L17*K17,2)");
    s(h, 18, 9, "C01");
    n(h, 18, 10, 1.0);
    fo(h, 18, 11, "=M17");
    fo(h, 18, 12, "=ROUND(L19*K19,2)");
    // C02: P1 repetida y P3 con números y códigos guardados como texto («02.01» parece un número)
    s(h, 20, 0, "C02          ");
    s(h, 20, 1, "Capítulo");
    s(h, 20, 3, "VARIOS");
    s(h, 22, 0, "P1           ");
    s(h, 22, 1, "Partida");
    s(h, 22, 2, "m");
    s(h, 22, 3, "ZANJA");
    s(h, 23, 4, "");
    n(h, 23, 5, 4.0);
    n(h, 23, 6, 0.0);
    n(h, 23, 7, 0.0);
    n(h, 23, 8, 0.0);
    parcial(h, 23);
    s(h, 24, 9, "P1");
    fo(h, 24, 10, "=SUM(J24:J24)");
    n(h, 24, 11, 0.0);
    fo(h, 24, 12, "=ROUND(L25*K25,2)");
    s(h, 26, 0, "02.01        ");
    s(h, 26, 1, "Partida");
    s(h, 26, 2, "m2");
    s(h, 26, 3, "SOLERA");
    s(h, 27, 4, "Nave");
    s(h, 27, 5, "1");
    s(h, 27, 6, "12.5");
    s(h, 27, 7, "4");
    s(h, 27, 8, "0");
    parcial(h, 27);
    s(h, 28, 9, "02.01");
    fo(h, 28, 10, "=SUM(J28:J28)");
    s(h, 28, 11, "0");
    fo(h, 28, 12, "=ROUND(L29*K29,2)");
    s(h, 30, 9, "C02");
    n(h, 30, 10, 1.0);
    fo(h, 30, 11, "=M25+M29");
    fo(h, 30, 12, "=ROUND(L31*K31,2)");
    s(h, 32, 9, "OBRA_2026");
    n(h, 32, 10, 1.0);
    fo(h, 32, 11, "=M19+M31");
    fo(h, 32, 12, "=ROUND(L33*K33,2)");
    wb.save_to_buffer().unwrap()
}

#[test]
fn importar_ciego() {
    let c = ppto_ciego::leer_bytes(&ciego(), "x").unwrap();
    assert!(c.incidencias.is_empty(), "{:?}", c.incidencias);
    let p = &c.presupuesto;
    assert_eq!(p.raiz, "OBRA_2026");
    assert_eq!(p.concepto("OBRA_2026").unwrap().resumen, "OBRA SINTÉTICA DE PRUEBA");
    assert_eq!(p.ruta("P2"), ["OBRA_2026", "C01", "C01.01", "P2"]);
    assert_eq!(
        p.concepto("P1").unwrap().texto.as_deref(),
        Some("Excavación de zanja por medios mecánicos, incluso carga.")
    );
    assert_eq!(p.concepto("02.01").unwrap().naturaleza, Naturaleza::Partida);
    // P1 en C01.01: 2 × 10,5 + 1 × 3,25 × 2 + 0,875 = 21 + 6,5 + 0,875 = 28,375 (sin redondear, como Excel)
    let q = |cap: &str, h: &str| {
        p.lineas_valoradas(cap)
            .unwrap()
            .into_iter()
            .find(|l| l.hijo == h)
            .unwrap()
            .cantidad
    };
    assert_eq!(q("C01.01", "P1"), dec!(28.375));
    assert_eq!(q("C01.01", "P2"), dec!(7));
    assert_eq!(q("C02", "P1"), dec!(4));
    assert_eq!(q("C02", "02.01"), dec!(50));
    // Celdas de precio: P1 en su fila de total (L13 y L25), P2 en su propia fila (L15), 02.01 en L29
    let refs: Vec<_> = c
        .apariciones
        .iter()
        .map(|a| (a.partida.as_str(), a.celda_precio.referencia()))
        .collect();
    assert_eq!(
        refs,
        [
            ("P1", "L13".into()),
            ("P2", "L15".into()),
            ("P1", "L25".into()),
            ("02.01", "L29".into())
        ]
    );
}

#[test]
fn devolver_excel_valorado() {
    // Precios: P1 12,35 €/m; P2 85 €/u; 02.01 18,10 €/m²
    // Excel: REDONDEAR(12,35 × 28,375; 2) = 350,43; 85 × 7 = 595,00; 12,35 × 4 = 49,40; 18,10 × 50 = 905,00
    // Total = 1.899,83 (igual en el programa)
    let original = ciego();
    let c = ppto_ciego::leer_bytes(&original, "x").unwrap();
    let mut p = c.presupuesto.clone();
    p.fijar_precio("P1", dec!(12.35)).unwrap();
    p.fijar_precio("P2", dec!(85)).unwrap();
    let v0 = ppto_ciego::valorar(&original, &c, &p).unwrap();
    assert_eq!(v0.sin_precio, ["02.01"]);
    p.fijar_precio("02.01", dec!(18.10)).unwrap();
    let v = ppto_ciego::valorar(&original, &c, &p).unwrap();
    assert_eq!(v.total_excel, dec!(1899.83));
    assert_eq!(p.pem().unwrap(), dec!(1899.83));
    assert_eq!(v.celdas_escritas, 4);
    assert!(v.sin_precio.is_empty());

    // Solo cambian las celdas de precio; el resto (textos, fórmulas) queda igual
    let leer = |b: &[u8]| {
        let mut l = calamine::open_workbook_auto_from_rs(std::io::Cursor::new(b.to_vec())).unwrap();
        (
            l.worksheet_range("Hoja1").unwrap(),
            l.worksheet_formula("Hoja1").unwrap(),
        )
    };
    let (a, fa) = leer(&original);
    let (b, fb) = leer(&v.bytes);
    assert_eq!(fa, fb, "fórmulas intactas");
    let mut cambios = Vec::new();
    for (i, fila) in a.rows().enumerate() {
        for (j, x) in fila.iter().enumerate() {
            let y = b.get((i, j)).cloned().unwrap_or(Data::Empty);
            if *x != y {
                cambios.push((i + 1, j, y));
            }
        }
    }
    assert_eq!(
        cambios,
        [
            (13, 11, Data::Float(12.35)),
            (15, 11, Data::Float(85.0)),
            (25, 11, Data::Float(12.35)),
            (29, 11, Data::Float(18.1)),
        ]
    );
    // Excel recalculará al abrir
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(v.bytes.clone())).unwrap();
    let mut wbxml = String::new();
    std::io::Read::read_to_string(&mut z.by_name("xl/workbook.xml").unwrap(), &mut wbxml).unwrap();
    assert!(wbxml.contains("fullCalcOnLoad=\"1\""), "{wbxml}");
}

#[test]
fn errores_claros() {
    assert!(matches!(
        ppto_ciego::leer_bytes(b"no es un excel", "x"),
        Err(ppto_ciego::ErrorCiego::Lectura(_))
    ));
    let mut wb = Workbook::new();
    wb.add_worksheet().write(0, 0, "hola").unwrap();
    let b = wb.save_to_buffer().unwrap();
    assert!(matches!(
        ppto_ciego::leer_bytes(&b, "x"),
        Err(ppto_ciego::ErrorCiego::SinCabecera)
    ));
}

#[test]
fn traer_precios_de_un_bc3_y_devolver_valorado() {
    // Base con P1 descompuesta (0,25 h × 20 + 1 m × 7,35 = 12,35) y P2 a 85 directo.
    use ppto_core::{Naturaleza as N, Presupuesto};
    let mut base = Presupuesto::new("base", "B", "Base");
    base.nuevo_capitulo("B", "C", "c").unwrap();
    base.nueva_partida("C", "P1", "m", "Zanja").unwrap();
    base.nueva_partida("C", "P2", "u", "Arqueta").unwrap();
    base.nuevo_recurso("MO", "h", "Oficial", N::ManoObra, dec!(20)).unwrap();
    base.nuevo_recurso("MT", "m", "Material", N::Material, dec!(7.35))
        .unwrap();
    base.anadir_linea("P1", "MO", dec!(0.25)).unwrap();
    base.anadir_linea("P1", "MT", dec!(1)).unwrap();
    base.fijar_precio("P2", dec!(85)).unwrap();
    // Pasa por BC3, como si viniera de Presto
    let bc3 = ppto_bc3::exportar(&base, &ppto_bc3::OpcionesExportacion::default())
        .unwrap()
        .bytes;
    let base = ppto_bc3::importar(&bc3).unwrap().presupuesto;

    let original = ciego();
    let c = ppto_ciego::leer_bytes(&original, "x").unwrap();
    let mut p = c.presupuesto.clone();
    let t = ppto_ciego::traer_precios(&mut p, &base);
    assert_eq!(t.con_precio.len(), 2);
    assert_eq!(t.no_encontradas, ["02.01"]);
    assert_eq!(
        p.concepto("P1").unwrap().descomposicion.len(),
        2,
        "trae el descompuesto"
    );
    let v = ppto_ciego::valorar(&original, &c, &p).unwrap();
    // 350,43 + 595,00 + 49,40 = 994,83 (02.01 sin precio)
    assert_eq!(v.total_excel, dec!(994.83));
    assert_eq!(v.sin_precio, ["02.01"]);
}

#[test]
fn excel_valorado_y_pdf_dicen_lo_mismo() {
    // Con CI 10 %: P1 12,35 → 13,59; P2 85 → 93,50; 02.01 18,10 → 19,91.
    // Cada partida debe salir con el mismo precio e importe en el Excel y en
    // el PDF, y el total del Excel = base imponible del PDF (perfil sin GG/BI).
    let original = ciego();
    let c = ppto_ciego::leer_bytes(&original, "x").unwrap();
    let mut p = c.presupuesto.clone();
    p.fijar_precio("P1", dec!(12.35)).unwrap();
    p.fijar_precio("P2", dec!(85)).unwrap();
    p.fijar_precio("02.01", dec!(18.10)).unwrap();
    p.costes_indirectos = dec!(10);
    let v = ppto_ciego::valorar(&original, &c, &p).unwrap();

    let mut o = ppto_pdf::OpcionesPdf::default();
    ppto_pdf::Perfil::instaladora().aplicar(&mut o);
    let d = ppto_pdf::datos(&p, &o, None).unwrap();
    // Partidas del PDF (recorriendo capítulos y subcapítulos)
    fn partidas(e: &serde_json::Value, cap: &str, out: &mut Vec<(String, String, String, String)>) {
        for x in e["elementos"].as_array().into_iter().flatten() {
            if x["tipo"] == "capitulo" {
                partidas(x, x["codigo"].as_str().unwrap(), out);
            } else {
                out.push((
                    cap.into(),
                    x["codigo"].as_str().unwrap().into(),
                    x["precio"].as_str().unwrap().into(),
                    x["importe"].as_str().unwrap().into(),
                ));
            }
        }
    }
    let mut pdf = Vec::new();
    for cap in d["capitulos"].as_array().unwrap() {
        partidas(cap, cap["codigo"].as_str().unwrap(), &mut pdf);
    }
    // Lo escrito en el Excel
    let mut l = calamine::open_workbook_auto_from_rs(std::io::Cursor::new(v.bytes.clone())).unwrap();
    let hoja = l.worksheet_range("Hoja1").unwrap();
    let mut total = rust_decimal::Decimal::ZERO;
    for a in &c.apariciones {
        let precio_excel = match hoja.get_value((a.celda_precio.fila, a.celda_precio.columna)).unwrap() {
            Data::Float(f) => format!("{f}").parse::<rust_decimal::Decimal>().unwrap(),
            otro => panic!("{otro:?}"),
        };
        let importe_excel = (precio_excel * a.cantidad_excel)
            .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero);
        total += importe_excel;
        let fila = pdf
            .iter()
            .find(|(cap, cod, _, _)| *cap == a.capitulo && *cod == a.partida)
            .unwrap();
        assert_eq!(fila.2, ppto_core::formato::eur(precio_excel), "precio de {}", a.partida);
        assert_eq!(
            fila.3,
            ppto_core::formato::eur(importe_excel),
            "importe de {}",
            a.partida
        );
    }
    assert_eq!(ppto_core::formato::eur(total), d["base"].as_str().unwrap());
    assert_eq!(total, v.total_excel);
    // 13,59 × 28,375 = 385,616 → 385,62; 93,50 × 7 = 654,50; 13,59 × 4 = 54,36; 19,91 × 50 = 995,50 → 2.089,98
    assert_eq!(total, dec!(2089.98));
}

/// Exportación de Presto sin mediciones: cabecera «Pres» en lugar de
/// «PrPres» y filas «Total CÓDIGO» en la columna Resumen (no en Parcial).
#[test]
fn listado_sin_mediciones_con_pres_y_totales_en_resumen() {
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    h.set_name("Hoja1").unwrap();
    h.write(0, 0, "Presupuesto").unwrap();
    for (j, t) in ["Código", "Nat", "Ud", "Resumen", "CanPres", "Pres", "ImpPres"]
        .iter()
        .enumerate()
    {
        h.write(1, j as u16, *t).unwrap();
    }
    let filas: &[(&str, &str, &str, &str, f64)] = &[
        ("C17", "Capítulo", "", "PCI", 1.0),
        ("PCI_01", "Capítulo", "", "DETECCIÓN", 1.0),
        ("07.01.01", "Partida", "Ud", "Central", 1.0),
        ("", "", "", "Central analógica de 1 lazo", 0.0),
        ("06710604", "Partida", "Ud", "Detector", 99.0),
        ("", "", "", "Total PCI_01", 1.0),
        ("PCI_02", "Capítulo", "", "EXTINCIÓN", 1.0),
        ("E26FJ180", "Partida", "Ud", "Extintor", 96.0),
        ("", "", "", "Total PCI_02", 1.0),
        ("", "", "", "Total C17", 1.0),
        ("C18", "Capítulo", "", "VENTILACIÓN", 1.0),
        ("VG04", "Partida", "Ud", "Ventilador", 6.0),
        ("", "", "", "Total C18", 1.0),
        ("", "", "", "Total 0", 1.0),
    ];
    for (i, (c, n, u, r, q)) in filas.iter().enumerate() {
        let f = 2 + i as u32;
        for (j, v) in [c, n, u, r].iter().enumerate() {
            if !v.is_empty() {
                h.write(f, j as u16, **v).unwrap();
            }
        }
        if *q > 0.0 {
            h.write(f, 4, *q).unwrap();
            h.write(f, 5, 0.0).unwrap();
            h.write_formula(f, 6, Formula::new(format!("=ROUND(E{0}*F{0},2)", f + 1)))
                .unwrap();
        }
    }
    let bytes = wb.save_to_buffer().unwrap();
    let c = ppto_ciego::leer_bytes(&bytes, "pci.xlsx").unwrap();
    let p = &c.presupuesto;
    assert_eq!(p.raiz, "0");
    let hijos = |cap: &str| {
        p.concepto(cap)
            .unwrap()
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(hijos("0"), ["C17", "C18"]);
    assert_eq!(hijos("C17"), ["PCI_01", "PCI_02"]);
    assert_eq!(hijos("PCI_01"), ["07.01.01", "06710604"]);
    assert_eq!(hijos("C18"), ["VG04"]);
    assert!(
        c.incidencias.iter().all(|i| !i.contains("sin fila de total")),
        "{:?}",
        c.incidencias
    );
    assert_eq!(
        p.concepto("07.01.01").unwrap().texto.as_deref(),
        Some("Central analógica de 1 lazo")
    );
    assert_eq!(p.concepto("06710604").unwrap().naturaleza, Naturaleza::Partida);
}

/// Código, Nat, Ud, Resumen, Comentario, Cantidad y fórmula de CanPres.
type Fila<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    Option<f64>,
    Option<&'a str>,
);

/// Listado de solo mediciones (Código, Nat, Ud, Resumen, Comentario,
/// Cantidad, CanPres), sin columna de precio, con resumen y texto en la misma
/// celda y «Total 00» en la columna Cantidad. Datos sintéticos.
fn solo_mediciones() -> Vec<u8> {
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    h.set_name("Hoja1").unwrap();
    h.write(0, 0, "B").unwrap();
    h.write(1, 0, "Presupuesto").unwrap();
    for (j, t) in ["Código", "Nat", "Ud", "Resumen", "Comentario", "Cantidad", "CanPres"]
        .iter()
        .enumerate()
    {
        h.write(2, j as u16, *t).unwrap();
    }
    let filas: &[Fila] = &[
        ("00", "Capítulo", "", "NOTAS", "", None, Some("=G7")),
        ("00.01", "Partida", "", "NOTAS GENERALES", "", None, None),
        ("", "", "", "Consideraciones generales de la oferta.", "", None, None),
        ("", "", "", "", "", None, None),
        ("", "", "", "", "", None, None),
        ("", "", "", "", "", None, None),
        ("21", "Capítulo", "", "VENTILACIÓN", "", None, None),
        ("21.01", "Capítulo", "", "GARAJE", "", None, None),
        (
            "21.01.01",
            "Partida",
            "u",
            "DETECTOR CO\nSuministro y montaje de detector de CO.",
            "",
            None,
            Some("=SUM(F13:F15)"),
        ),
        ("", "", "", "", "S1", Some(6.0), None),
        ("", "", "", "", "S2", Some(6.0), None),
        ("", "", "", "", "S3", Some(4.0), None),
        (
            "21.01.02",
            "Partida",
            "m2",
            "CONDUCTO CHAPA\nConducto rectangular de chapa galvanizada.",
            "",
            None,
            Some("=SUM(F17:F18)"),
        ),
        ("", "", "", "", "Aporte", Some(120.5), None),
        ("", "", "", "", "Extracción", Some(99.25), None),
        ("", "", "", "Total 21.01", "", None, None),
    ];
    for (i, (c, nat, ud, res, com, q, canpres)) in filas.iter().enumerate() {
        let f = 3 + i as u32;
        for (j, v) in [c, nat, ud, res, com].iter().enumerate() {
            if !v.is_empty() {
                h.write(f, j as u16, **v).unwrap();
            }
        }
        if let Some(q) = q {
            h.write(f, 5, *q).unwrap();
        }
        if let Some(cp) = canpres {
            h.write_formula(f, 6, Formula::new(*cp)).unwrap();
        }
    }
    // Fila 7 (índice 6): «Total 00» en Cantidad y 1 en CanPres
    h.write(6, 5, "Total 00").unwrap();
    h.write(6, 6, 1).unwrap();
    wb.save_to_buffer().unwrap()
}

#[test]
fn mediciones_sin_columna_de_precio() {
    let bytes = solo_mediciones();
    let c = ppto_ciego::leer_bytes(&bytes, "SIC ventilación").unwrap();
    let p = &c.presupuesto;
    assert_eq!(p.concepto(&p.raiz).unwrap().resumen, "SIC ventilación");
    let det = p.concepto("21.01.01").unwrap();
    assert_eq!(det.resumen, "DETECTOR CO");
    assert_eq!(det.texto.as_deref(), Some("Suministro y montaje de detector de CO."));
    assert_eq!(det.naturaleza, Naturaleza::Partida);
    let q = |cap: &str, h: &str| {
        p.concepto(cap)
            .unwrap()
            .descomposicion
            .iter()
            .find(|l| l.hijo == h)
            .unwrap()
            .cantidad()
    };
    assert_eq!(q("21.01", "21.01.01"), dec!(16));
    assert_eq!(q("21.01", "21.01.02"), dec!(219.75));
    assert_eq!(
        p.concepto("00.01").unwrap().texto.as_deref(),
        Some("Consideraciones generales de la oferta.")
    );
    let nuevas = c.columnas_nuevas.unwrap();
    assert_eq!((nuevas.precio, nuevas.importe, nuevas.cantidad), (7, 8, 6));

    // Valorar: precio en la columna H nueva, importe en I con fórmula
    let mut v = p.clone();
    v.fijar_precio("21.01.01", dec!(85)).unwrap();
    v.fijar_precio("21.01.02", dec!(32.4)).unwrap();
    let r = ppto_ciego::valorar(&bytes, &c, &v).unwrap();
    assert_eq!(r.total_excel, dec!(1360) + dec!(7119.90));
    let mut libro: calamine::Xlsx<_> = calamine::open_workbook_from_rs(std::io::Cursor::new(r.bytes)).unwrap();
    let h = libro.worksheet_range("Hoja1").unwrap();
    assert_eq!(h.get_value((2, 7)), Some(&Data::String("PrPres".into())));
    assert_eq!(h.get_value((2, 8)), Some(&Data::String("ImpPres".into())));
    assert_eq!(h.get_value((11, 7)), Some(&Data::Float(85.0)));
    assert_eq!(h.get_value((15, 7)), Some(&Data::Float(32.4)));
    let f = libro.worksheet_formula("Hoja1").unwrap();
    assert_eq!(f.get_value((11, 8)).map(String::as_str), Some("ROUND(G12*H12,2)"));
    // Lo que había sigue igual
    assert_eq!(h.get_value((12, 4)), Some(&Data::String("S1".into())));
}

/// Listado con cabecera «EDT | Código | Resumen | CanPres | Ud | Pres | ImpPres»
/// y las filas corridas (Código, Nat, Ud, Resumen, CanPres, Pres, ImpPres),
/// texto descriptivo en la columna Resumen y sin filas de total.
#[test]
fn ciego_con_cabecera_corrida_se_lee_por_el_contenido() {
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    let filas: Vec<Vec<&str>> = vec![
        vec!["EDT", "Código", "Resumen", "CanPres", "Ud", "Pres", "ImpPres"],
        vec!["5.16", "CAP", "INSTALACION SINTETICA", "1", "", "0", ""],
        vec!["CAP", "Capítulo", "", "INSTALACION SINTETICA", "1", "", "0"],
        vec![
            "P.VEN",
            "Partida",
            "ud",
            "CAJA DE VENTILACION FABA-15/15-1,1kW",
            "2",
            "0",
            "0",
        ],
        vec!["", "", "", "Suministro y montaje de caja de ventilación."],
        vec!["P.REJ", "Partida", "ud", "REJILLA RETORNO 400x100 mm.", "8", "0", "0"],
        vec!["", "", "", "Rejilla de retorno de aluminio."],
        vec![
            "P.CON",
            "Partida",
            "m2",
            "CONDUCTO CHAPA ACERO GALV.",
            "153,5",
            "0",
            "0",
        ],
    ];
    for (i, f) in filas.iter().enumerate() {
        for (j, v) in f.iter().enumerate() {
            let (i, j) = (u32::try_from(i).unwrap(), u16::try_from(j).unwrap());
            match v.replace(',', ".").parse::<f64>() {
                Ok(n) if j >= 4 || (j == 3 && *v != "INSTALACION SINTETICA") && !v.is_empty() => {
                    h.write_number(i, j, n).unwrap();
                }
                _ if !v.is_empty() => {
                    h.write_string(i, j, *v).unwrap();
                }
                _ => {}
            }
        }
    }
    let b = wb.save_to_buffer().unwrap();
    let c = ppto_ciego::leer_bytes(&b, "Memoria sintética").unwrap();
    let p = &c.presupuesto;
    assert_eq!(p.conceptos["OBRA"].resumen, "Memoria sintética");
    let cap = &p.conceptos["CAP"];
    assert_eq!(cap.resumen, "INSTALACION SINTETICA");
    let q: Vec<(String, rust_decimal::Decimal)> = cap
        .descomposicion
        .iter()
        .map(|l| (l.hijo.clone(), l.rendimiento))
        .collect();
    assert_eq!(
        q,
        vec![
            ("P.VEN".into(), dec!(2)),
            ("P.REJ".into(), dec!(8)),
            ("P.CON".into(), dec!(153.5))
        ]
    );
    assert_eq!(p.conceptos["P.CON"].unidad, "m2");
    assert_eq!(
        p.conceptos["P.REJ"].texto.as_deref(),
        Some("Rejilla de retorno de aluminio.")
    );
    assert!(c.incidencias.iter().any(|i| i.contains("columnas deducidas")));
    assert!(!c.incidencias.iter().any(|i| i.contains("sin fila de total al final")));
    // Se devuelve valorado en sus columnas Pres e ImpPres
    let mut v = p.clone();
    for k in ["P.VEN", "P.REJ", "P.CON"] {
        v.fijar_precio(k, dec!(10)).unwrap();
    }
    let val = ppto_ciego::valorar(&b, &c, &v).unwrap();
    assert_eq!(val.total_excel, dec!(1635));
    assert!(val.sin_precio.is_empty());
}

/// Listado completo de Presto (Código, Nat, Ud, Resumen, Comentario, N,
/// Longitud, Anchura, Altura, Parcial, CanPres, PrPres, ImpPres) sin fila de
/// cabecera, con «Total CÓDIGO» en Parcial y líneas de medición.
#[test]
fn listado_completo_sin_cabecera() {
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    // (fila, columna, valor): número si empieza por '#'
    let celdas: Vec<(u32, u16, &str)> = vec![
        (1, 0, "C1"),
        (1, 1, "Capítulo"),
        (1, 3, "INSTALACION SINTETICA"),
        (1, 10, "#1"),
        (1, 11, "#0"),
        (1, 12, "#0"),
        (2, 0, "C1.1"),
        (2, 1, "Capítulo"),
        (2, 3, "EXTINCION"),
        (2, 10, "#1"),
        (2, 11, "#0"),
        (2, 12, "#0"),
        (3, 0, "P.EXT"),
        (3, 1, "Partida"),
        (3, 2, "ud"),
        (3, 3, "EXTINTOR SINTETICO 6 kg"),
        (3, 10, "#22"),
        (3, 11, "#0"),
        (3, 12, "#0"),
        (4, 3, "Extintor de polvo sintético."),
        (5, 9, "Total P.EXT"),
        (5, 10, "#22"),
        (5, 11, "#0"),
        (5, 12, "#0"),
        (6, 9, "Total C1.1"),
        (6, 10, "#1"),
        (7, 0, "C1.2"),
        (7, 1, "Capítulo"),
        (7, 3, "VENTILACION"),
        (7, 10, "#1"),
        (7, 11, "#0"),
        (7, 12, "#0"),
        (8, 0, "P.CON"),
        (8, 1, "Partida"),
        (8, 2, "m2"),
        (8, 3, "CONDUCTO SINTETICO"),
        (8, 10, "#22"),
        (8, 11, "#0"),
        (8, 12, "#0"),
        (9, 4, "Escalera 1"),
        (9, 5, "#1"),
        (9, 6, "#11"),
        (9, 9, "#11"),
        (10, 4, "Escalera 2"),
        (10, 5, "#1"),
        (10, 6, "#11"),
        (10, 9, "#11"),
        (11, 9, "Total P.CON"),
        (11, 10, "#22"),
        (11, 11, "#0"),
        (11, 12, "#0"),
        (12, 9, "Total C1.2"),
        (12, 10, "#1"),
        (13, 9, "Total C1"),
        (13, 10, "#1"),
    ];
    for (i, j, v) in celdas {
        match v.strip_prefix('#') {
            Some(n) => h.write_number(i, j, n.parse::<f64>().unwrap()).unwrap(),
            None => h.write_string(i, j, v).unwrap(),
        };
    }
    let b = wb.save_to_buffer().unwrap();
    let c = ppto_ciego::leer_bytes(&b, "Memoria sintética").unwrap();
    let p = &c.presupuesto;
    let hijos = |k: &str| -> Vec<String> { p.conceptos[k].descomposicion.iter().map(|l| l.hijo.clone()).collect() };
    assert_eq!(hijos("C1"), vec!["C1.1", "C1.2"], "{:?}", c.incidencias);
    assert_eq!(hijos("C1.1"), vec!["P.EXT"]);
    assert_eq!(p.conceptos["C1.2"].descomposicion[0].rendimiento, dec!(22));
    assert_eq!(p.conceptos["P.CON"].unidad, "m2");
    assert_eq!(
        p.conceptos["P.EXT"].texto.as_deref(),
        Some("Extintor de polvo sintético.")
    );
    assert!(c.incidencias.iter().any(|i| i.contains("columnas deducidas")));
    assert!(
        !c.incidencias.iter().any(|i| i.contains("sin fila de total")),
        "{:?}",
        c.incidencias
    );
    let v = ppto_ciego::valorar(&b, &c, p).unwrap();
    assert_eq!(v.sin_precio.len(), 2);
}
