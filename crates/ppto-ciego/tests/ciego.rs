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
