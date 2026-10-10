// SPDX-License-Identifier: GPL-3.0-or-later
//! Exportador BC3: pruebas de ida y vuelta (P-011).
//! Criterio de aceptación: conservar estructura, mediciones, precios e importes.

use ppto_bc3::{OpcionesExportacion, exportar, importar, importar_fichero};
use ppto_core::Presupuesto;
use ppto_core::concepto::{Concepto, Naturaleza};
use ppto_core::ejemplos::suelo_radiante;
use rust_decimal_macros::dec;

fn datos(nombre: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/datos")
        .join(nombre)
}

/// Exporta, vuelve a importar y comprueba que el resultado es equivalente.
fn ida_y_vuelta(p: &Presupuesto) -> Presupuesto {
    let ex = exportar(p, &OpcionesExportacion::default()).unwrap();
    let imp = importar(&ex.bytes).unwrap();
    assert_eq!(imp.errores(), 0, "{:#?}", imp.incidencias);
    assert_eq!(imp.avisos(), 0, "{:#?}", imp.incidencias);
    assert!(imp.discrepancias.is_empty(), "{:?}", imp.discrepancias);
    let q = imp.presupuesto;
    // Estructura y datos: mismos conceptos, descomposiciones, textos y mediciones
    assert_eq!(q.raiz, p.raiz);
    assert_eq!(q.costes_indirectos, p.costes_indirectos);
    assert_eq!(q.conceptos.len(), p.conceptos.len());
    for (codigo, c) in &p.conceptos {
        let d = &q.conceptos[codigo];
        assert_eq!(d.naturaleza, c.naturaleza, "{codigo}");
        assert_eq!(
            (&d.unidad, &d.resumen, &d.texto),
            (&c.unidad, &c.resumen, &c.texto),
            "{codigo}"
        );
        if c.naturaleza.es_basico() {
            assert_eq!(d.precio, c.precio, "{codigo}");
        }
        assert_eq!(d.descomposicion.len(), c.descomposicion.len(), "{codigo}");
        for (a, b) in d.descomposicion.iter().zip(&c.descomposicion) {
            assert_eq!((&a.hijo, a.cantidad()), (&b.hijo, b.cantidad()), "{codigo}");
        }
    }
    assert_eq!(q.mediciones, p.mediciones);
    // Precios e importes al céntimo
    let (vp, vq) = (p.valorar().unwrap(), q.valorar().unwrap());
    assert_eq!(vq.precios, vp.precios);
    assert_eq!((vq.pem, vq.coste_directo), (vp.pem, vp.coste_directo));
    q
}

#[test]
fn ejemplo_suelo_radiante() {
    let q = ida_y_vuelta(&suelo_radiante());
    assert_eq!(q.pem().unwrap(), dec!(7005.15));
}

#[test]
fn con_costes_indirectos() {
    let mut p = suelo_radiante();
    p.costes_indirectos = dec!(45);
    let q = ida_y_vuelta(&p);
    assert_eq!(q.pem().unwrap(), dec!(10157.37));
    assert_eq!(q.coste_directo().unwrap(), dec!(7005.15));
}

#[test]
fn ficheros_de_prueba_importados_y_reexportados() {
    for f in ["suelo-radiante.bc3", "presto88-minimo.bc3", "utf8.bc3"] {
        let p = importar_fichero(datos(f)).unwrap().presupuesto;
        ida_y_vuelta(&p);
    }
}

#[test]
fn exportar_es_estable() {
    // exportar(importar(exportar(p))) produce exactamente los mismos bytes
    let a = exportar(&suelo_radiante(), &OpcionesExportacion::default())
        .unwrap()
        .bytes;
    let p = importar(&a).unwrap().presupuesto;
    let b = exportar(&p, &OpcionesExportacion::default()).unwrap().bytes;
    assert_eq!(String::from_utf8_lossy(&a), String::from_utf8_lossy(&b));
}

#[test]
fn formato_como_presto_8_8() {
    let mut p = suelo_radiante();
    p.costes_indirectos = dec!(3);
    let o = OpcionesExportacion {
        gastos_generales: Some(dec!(13)),
        beneficio_industrial: Some(dec!(6)),
        iva: Some(dec!(21)),
        fecha: Some("101026".into()),
    };
    let ex = exportar(&p, &o).unwrap();
    let texto = encoding_rs::WINDOWS_1252.decode(&ex.bytes).0.into_owned();
    assert!(texto.starts_with("~V|presupuestos-bc3|FIEBDC-3/2002|"));
    assert!(texto.contains("|ANSI|\r\n"));
    assert!(texto.contains("~K|\\2\\2\\3\\2\\2\\2\\2\\EUR\\|3\\13\\6\\0\\21|\r\n"));
    assert!(texto.contains("~C|OBRA##||Vivienda tipo (sintético)|"));
    assert!(texto.contains("~C|C01#||Calefacción por suelo radiante|"));
    // Unidad de obra con precio de venta (29,89 × 1,03 = 30,7867 → 30,79) y fecha
    assert!(texto.contains("~C|SR.M2|m2|Suelo radiante con panel de tetones y tubo PE-RT 16×2|30.79|101026|0|"));
    assert!(texto.contains("~C|MO.OF1F|h|Oficial 1ª instalador de calefacción|24.5|101026|1|"));
    // Porcentaje en fracción
    assert!(texto.contains("%MA\\1\\0.02\\"));
    // Hijos sin «#», mediciones con padre\hijo
    assert!(texto.contains("~D|OBRA##|C01\\1\\1\\|"));
    assert!(texto.contains("~M|C01#\\SR.M2||210|"));
    // ANSI real: «ñ» de «Baños» es el byte 0xF1
    assert!(ex.bytes.windows(4).any(|w| w == b"Ba\xf1o"));
    assert!(ex.avisos.is_empty(), "{:?}", ex.avisos);
}

#[test]
fn caracteres_reservados_y_fuera_de_ansi() {
    let mut p = suelo_radiante();
    p.conceptos.get_mut("SR.M2").unwrap().resumen = "Suelo | radiante ~ 漢 €".into();
    let ex = exportar(&p, &OpcionesExportacion::default()).unwrap();
    assert_eq!(ex.avisos.len(), 2, "{:?}", ex.avisos);
    let imp = importar(&ex.bytes).unwrap();
    assert_eq!(imp.errores(), 0);
    assert_eq!(
        imp.presupuesto.concepto("SR.M2").unwrap().resumen,
        "Suelo / radiante - ? €"
    );
    assert_eq!(imp.presupuesto.pem().unwrap(), dec!(7005.15));
}

#[test]
fn subcontrata_se_avisa() {
    let mut p = suelo_radiante();
    p.insertar(Concepto::basico(
        "SUB1",
        "m2",
        "Montaje subcontratado",
        Naturaleza::Subcontrata,
        dec!(9),
    ))
    .unwrap();
    let ex = exportar(&p, &OpcionesExportacion::default()).unwrap();
    assert!(ex.avisos.iter().any(|a| a.contains("subcontrata")));
}
