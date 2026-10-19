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
        ..Default::default()
    };
    let ex = exportar(&p, &o).unwrap();
    let texto = encoding_rs::WINDOWS_1252.decode(&ex.bytes).0.into_owned();
    assert!(texto.starts_with("~V|presupuestos-bc3|FIEBDC-3/2002|"));
    assert!(texto.contains("|ANSI|\r\n"));
    assert!(texto.contains("~K|\\2\\2\\3\\2\\2\\2\\2\\EUR\\|3\\13\\6\\0\\21|\r\n"));
    assert!(texto.contains("~C|OBRA##||Vivienda tipo (sintético)|"));
    assert!(texto.contains("~C|C01#||Calefacción por suelo radiante|"));
    // Unidad de obra con su coste sin indirectos (como Presto 8.8) y fecha
    assert!(texto.contains("~C|SR.M2|m2|Suelo radiante con panel de tetones y tubo PE-RT 16×2|29.89|101026|0|"));
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

/// Con `descuentos_visibles` el material sale a PVP con su línea de
/// descuento negativa y la partida vale lo mismo (al céntimo) que a neto.
#[test]
fn descuentos_visibles_en_el_descompuesto() {
    use ppto_core::LineaDescomposicion;
    let mut p = Presupuesto::new("dto", "OBRA", "Obra");
    p.insertar(Concepto::basico(
        "MD.REJ",
        "ud",
        "Rejilla",
        Naturaleza::Material,
        dec!(0),
    ))
    .unwrap();
    p.insertar(Concepto::basico(
        "MO.OF",
        "h",
        "Oficial",
        Naturaleza::ManoObra,
        dec!(28),
    ))
    .unwrap();
    p.insertar(Concepto::porcentaje("%MA", "Medios auxiliares")).unwrap();
    p.insertar(Concepto::partida(
        "P1",
        "ud",
        "Rejilla colocada",
        vec![
            LineaDescomposicion::new("MD.REJ", dec!(1)),
            LineaDescomposicion::new("MO.OF", dec!(0.5)),
            LineaDescomposicion::new("%MA", dec!(2)),
        ],
    ))
    .unwrap();
    p.nuevo_capitulo("OBRA", "C1", "Difusión").unwrap();
    p.anadir_linea("C1", "P1", dec!(10)).unwrap();
    p.fijar_tarifa("MD.REJ", "MADEL", dec!(80.49)).unwrap();
    p.fijar_descuento("MADEL", dec!(35)).unwrap();
    let neto = p.precio("P1").unwrap();

    let o = OpcionesExportacion {
        descuentos_visibles: true,
        ..Default::default()
    };
    let ex = exportar(&p, &o).unwrap();
    let texto = String::from_utf8_lossy(&ex.bytes);
    assert!(texto.contains("~C|MD.REJ|ud|Rejilla|80.49|"), "{texto}");
    assert!(texto.contains("MD.REJ%\\1\\-0.35\\"), "{texto}");
    assert!(texto.contains("Descuento MADEL 35 %"), "{texto}");

    let q = importar(&ex.bytes).unwrap().presupuesto;
    let lineas: Vec<_> = q
        .concepto("P1")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .collect();
    // Al volver a leerlo, la línea de descuento se convierte otra vez en
    // tarifa del recurso y descuento del fabricante
    assert_eq!(lineas, ["MD.REJ", "MO.OF", "%MA"]);
    let t = q.concepto("MD.REJ").unwrap().tarifa.clone().unwrap();
    assert_eq!((t.fabricante.as_str(), t.pvp), ("MADEL", dec!(80.49)));
    assert_eq!(q.descuentos.get("MADEL"), Some(&dec!(35)));
    assert!(!q.conceptos.contains_key("MD.REJ%"));
    let visible = q.precio("P1").unwrap();
    assert!((visible - neto).abs() <= dec!(0.01), "{visible} frente a {neto}");
}

/// Para un Presto antiguo que corta los códigos: ningún código pasa del
/// límite, los que empezaban igual siguen siendo distintos (también un
/// recurso y su línea de descuento) y la obra vale lo mismo.
#[test]
fn codigos_cortos_para_presto_antiguo() {
    use ppto_core::LineaDescomposicion;
    let mut p = Presupuesto::new("cortos", "OBRA", "Obra");
    for (c, precio) in [("IT26.TNIRCL7511", dec!(0)), ("IT26.TNIRCL6311", dec!(0))] {
        p.insertar(Concepto::basico(c, "m", "Tubo", Naturaleza::Material, precio))
            .unwrap();
    }
    p.insertar(Concepto::basico(
        "MO.OF",
        "h",
        "Oficial",
        Naturaleza::ManoObra,
        dec!(28),
    ))
    .unwrap();
    for (partida, tubo) in [("ICS011075I", "IT26.TNIRCL7511"), ("ICS011063I", "IT26.TNIRCL6311")] {
        p.insertar(Concepto::partida(
            partida,
            "m",
            "Tubo instalado",
            vec![
                LineaDescomposicion::new(tubo, dec!(1.2)),
                LineaDescomposicion::new("MO.OF", dec!(0.4)),
            ],
        ))
        .unwrap();
    }
    p.nuevo_capitulo("OBRA", "C1", "Hidráulica").unwrap();
    p.anadir_linea("C1", "ICS011075I", dec!(20)).unwrap();
    p.anadir_linea("C1", "ICS011063I", dec!(3)).unwrap();
    p.fijar_tarifa("IT26.TNIRCL7511", "ITALSAN", dec!(24.67)).unwrap();
    p.fijar_tarifa("IT26.TNIRCL6311", "ITALSAN", dec!(18.80)).unwrap();
    p.fijar_descuento("ITALSAN", dec!(50)).unwrap();
    let o = OpcionesExportacion {
        descuentos_visibles: true,
        ..Default::default()
    };
    // Mismo importe que la exportación normal (solo cambian los códigos)
    let antes = importar(&exportar(&p, &o).unwrap().bytes)
        .unwrap()
        .presupuesto
        .valorar()
        .unwrap()
        .pem;
    let o = OpcionesExportacion {
        longitud_codigo: Some(12),
        ..o
    };
    let ex = exportar(&p, &o).unwrap();
    let q = importar(&ex.bytes).unwrap().presupuesto;
    assert!(
        q.conceptos.keys().all(|c| c.chars().count() <= 12),
        "{:?}",
        q.conceptos.keys()
    );
    // Cortados a 12 seguirían siendo únicos
    let cortos: std::collections::BTreeSet<String> = q.conceptos.keys().map(|c| c.chars().take(12).collect()).collect();
    assert_eq!(cortos.len(), q.conceptos.len());
    // El descuento sigue enganchado a su recurso: al leerlo se recupera
    assert!(q.conceptos.values().filter(|c| c.tarifa.is_some()).count() == 2);
    assert_eq!(q.descuentos.get("ITALSAN"), Some(&dec!(50)));
    // y en el fichero va a la vista, con códigos cortos
    let texto = String::from_utf8_lossy(&ex.bytes);
    assert!(
        texto
            .lines()
            .any(|l| l.starts_with("~C|") && l.contains("%|%|Descuento ITALSAN 50 %")),
        "{texto}"
    );
    let despues = q.valorar().unwrap().pem;
    assert_eq!(despues, antes);
}
