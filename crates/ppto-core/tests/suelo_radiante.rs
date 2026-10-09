// SPDX-License-Identifier: GPL-3.0-or-later
//! Caso de validación «suelo radiante» (P-003, P-006, P-007, P-008, P-017).
//! Cada valor esperado está calculado a mano en
//! docs/casos-validacion/suelo-radiante.md. Si cambia un valor aquí, debe
//! cambiar allí y pasar revisión técnica (etiqueta needs-engineering-review).

use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::subcontrata::{Asignacion, Contratacion, PaqueteTrabajo, simular};
use ppto_core::{Decimal, ErrorMotor, Presupuesto, venta};
use rust_decimal_macros::dec;

#[test]
fn descompuesto_suelo_radiante() {
    let p = suelo_radiante();
    let l = p.lineas_valoradas("SR.M2").unwrap();
    let importes: Vec<Decimal> = l.iter().map(|x| x.importe).collect();
    // 9,975→9,98 · 7,70 · 0,24 · 6,125→6,13 · 5,25 · 2 % × 29,30 = 0,586→0,59
    assert_eq!(
        importes,
        vec![dec!(9.98), dec!(7.70), dec!(0.24), dec!(6.13), dec!(5.25), dec!(0.59)]
    );
    assert_eq!(
        l[5].precio,
        dec!(29.30),
        "base del porcentaje = suma de líneas anteriores"
    );
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(29.89));
    assert_eq!(p.precio("SR.COL8").unwrap(), dec!(242.75));
}

#[test]
fn mediciones_e_importes_de_capitulo() {
    let p = suelo_radiante();
    let cap = p.lineas_valoradas("C01").unwrap();
    assert_eq!(cap[0].cantidad, dec!(210.00));
    assert_eq!(cap[0].importe, dec!(6276.90));
    assert_eq!(cap[1].cantidad, dec!(3));
    assert_eq!(cap[1].importe, dec!(728.25));
    assert_eq!(p.pem().unwrap(), dec!(7005.15));
}

#[test]
fn explosion_de_recursos_y_descuadre_de_redondeo() {
    let p = suelo_radiante();
    let e = p.explosion_recursos().unwrap();
    let r = |c| e.recurso(c).unwrap();
    assert_eq!(r("MO.OF1F").cantidad, dec!(57.0)); // 52,5 + 4,5 h
    assert_eq!(r("MO.OF1F").importe, dec!(1396.50));
    assert_eq!(r("MO.AYUF").cantidad, dec!(55.5)); // 52,5 + 3 h
    assert_eq!(r("MO.AYUF").importe, dec!(1165.50));
    assert_eq!(r("MT.PANEL").cantidad, dec!(220.5));
    assert_eq!(r("MT.TUBO16").cantidad, dec!(1470));
    assert_eq!(r("%MA").importe, dec!(123.90));
    let horas: Decimal = e.por_naturaleza(Naturaleza::ManoObra).map(|x| x.cantidad).sum();
    assert_eq!(horas, dec!(112.5));
    assert_eq!(e.total, dec!(7003.05));
    // Descuadre explicado: (9,98 − 9,975) × 210 + (6,13 − 6,125) × 210 = 2,10 €
    assert_eq!(e.descuadre_redondeo, dec!(2.10));
}

#[test]
fn ofertas_con_distinta_unidad_de_contratacion() {
    let p = suelo_radiante();
    let pem_antes = p.pem().unwrap();
    let [a, b, c] = ofertas_montaje_suelo_radiante();

    let ra = simular(&p, std::slice::from_ref(&a)).unwrap();
    assert_eq!(ra.coste_retirado, dec!(2389.80)); // 210 × (6,13 + 5,25)
    assert_eq!(ra.coste_contratado, dec!(1890.00)); // 210 m² × 9,00
    assert_eq!(ra.coste_escenario, dec!(6505.35));
    assert_eq!(ra.ahorro, dec!(499.80));
    assert_eq!(ra.horas_liberadas["MO.OF1F"], dec!(52.5));
    assert_eq!(ra.horas_liberadas["MO.AYUF"], dec!(52.5));
    assert_eq!(ra.notas.len(), 1, "debe avisar de los medios auxiliares");

    let rb = simular(&p, std::slice::from_ref(&b)).unwrap();
    assert_eq!(rb.detalle[0].cantidad_contratada, dec!(1470.00)); // 210 × 7 m/m²
    assert_eq!(rb.coste_contratado, dec!(2352.00));
    assert_eq!(rb.coste_escenario, dec!(6967.35));

    let rc = simular(&p, std::slice::from_ref(&c)).unwrap();
    assert_eq!(rc.coste_contratado, dec!(2300.00));
    assert_eq!(rc.coste_escenario, dec!(6915.35));

    // Comparación homogénea en €/m² de partida
    assert_eq!(ra.precio_equivalente("SUB-A", "SR.M2", 2), Some(dec!(9.00)));
    assert_eq!(rb.precio_equivalente("SUB-B", "SR.M2", 2), Some(dec!(11.20)));
    assert_eq!(rc.precio_equivalente("SUB-C", "SR.M2", 2), Some(dec!(10.95)));

    // El presupuesto original no cambia
    assert_eq!(p.pem().unwrap(), pem_antes);
    assert_eq!(p, suelo_radiante());
    // Los colectores siguen con su mano de obra propia
    assert_eq!(p.precio("SR.COL8").unwrap(), dec!(242.75));
}

#[test]
fn no_se_duplican_costes_entre_paquetes() {
    let p = suelo_radiante();
    let [a, b, _] = ofertas_montaje_suelo_radiante();
    match simular(&p, &[a, b]) {
        Err(ErrorMotor::DobleSustitucion {
            recurso,
            paquete_previo,
            ..
        }) => {
            assert_eq!(recurso, "MO.OF1F");
            assert_eq!(paquete_previo, "SUB-A");
        }
        otro => panic!("se esperaba DobleSustitucion, se obtuvo {otro:?}"),
    }
}

#[test]
fn paquetes_complementarios_si_se_pueden_combinar() {
    // Oficial subcontratado a 5,00 €/m² y ayudante a 4,00 €/m²: no se solapan.
    let p = suelo_radiante();
    let paq = |cod: &str, rec: &str, precio| PaqueteTrabajo {
        codigo: cod.into(),
        descripcion: cod.into(),
        contratacion: Contratacion::PorUnidad {
            unidad: "m2".into(),
            precio,
        },
        asignaciones: vec![Asignacion {
            partida: "SR.M2".into(),
            recursos: vec![rec.into()],
            factor_conversion: dec!(1),
        }],
        incluye: vec![],
        excluye: vec![],
    };
    let r = simular(
        &p,
        &[paq("OF", "MO.OF1F", dec!(5.00)), paq("AY", "MO.AYUF", dec!(4.00))],
    )
    .unwrap();
    assert_eq!(r.coste_retirado, dec!(2389.80));
    assert_eq!(r.coste_contratado, dec!(1890.00));
}

#[test]
fn recurso_ajeno_a_la_partida() {
    let p = suelo_radiante();
    let paq = PaqueteTrabajo {
        codigo: "X".into(),
        descripcion: "x".into(),
        contratacion: Contratacion::Alzado { importe: dec!(1) },
        asignaciones: vec![Asignacion {
            partida: "SR.M2".into(),
            recursos: vec!["MT.COL8".into()],
            factor_conversion: dec!(0),
        }],
        incluye: vec![],
        excluye: vec![],
    };
    assert!(matches!(
        simular(&p, &[paq]),
        Err(ErrorMotor::RecursoNoEnPartida { .. })
    ));
}

#[test]
fn resumen_y_margen_sobre_el_ejemplo() {
    let p = suelo_radiante();
    let r = venta::resumen(
        p.pem().unwrap(),
        venta::GG_HABITUAL,
        venta::BI_HABITUAL,
        venta::IVA_GENERAL,
        2,
    );
    assert_eq!(r.total, dec!(10086.72));
    // Venta con 30 % de margen sobre el coste directo del suelo radiante
    assert_eq!(venta::venta_por_margen(dec!(29.89), dec!(30), 2).unwrap(), dec!(42.70));
}

#[test]
fn edicion_de_mediciones_rendimientos_y_precios() {
    use ppto_core::medicion::LineaMedicion;
    let mut p = suelo_radiante();
    // Añadir 10 m² más de medición: 220 × 29,89 = 6.575,80
    let mut m = p.mediciones[&("C01".to_string(), "SR.M2".to_string())].clone();
    m.lineas.push(LineaMedicion::normal(
        "Ampliación",
        Some(dec!(1)),
        Some(dec!(5)),
        Some(dec!(2)),
        None,
    ));
    let avisos = p.actualizar_medicion("C01", "SR.M2", m).unwrap();
    assert!(avisos.is_empty());
    assert_eq!(p.lineas_valoradas("C01").unwrap()[0].importe, dec!(6575.80));
    // Rendimiento de tubo 7 → 8 m/m²: +1,10 → suma 30,40; MA 0,608 → 0,61; precio 31,01
    p.fijar_rendimiento("SR.M2", "MT.TUBO16", dec!(8)).unwrap();
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(31.01));
    // Precio del oficial 24,50 → 26,00: colector 185 + 39,00 + 21 = 245,00
    p.fijar_precio("MO.OF1F", dec!(26.00)).unwrap();
    assert_eq!(p.precio("SR.COL8").unwrap(), dec!(245.00));
    // No se puede fijar precio a una partida ni un precio negativo
    assert!(p.fijar_precio("SR.M2", dec!(1)).is_err());
    assert!(p.fijar_precio("MT.TUBO16", dec!(-1)).is_err());
    assert!(p.fijar_rendimiento("SR.M2", "MT.COL8", dec!(1)).is_err());
}

#[test]
fn costes_indirectos_por_partida_como_presto() {
    use ppto_core::presupuesto::OpcionesCi;
    let mut p = suelo_radiante();
    p.costes_indirectos = dec!(45);
    // SR.M2: 29,89 × 1,45 = 43,3405 → 43,34 ; SR.COL8: 242,75 × 1,45 = 351,9875 → 351,99
    assert_eq!(p.precio_con_indirectos("SR.M2").unwrap(), dec!(43.34));
    assert_eq!(p.precio_con_indirectos("SR.COL8").unwrap(), dec!(351.99));
    // El coste unitario no cambia
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(29.89));
    // PEM = 210 × 43,34 + 3 × 351,99 = 9.101,40 + 1.055,97 = 10.157,37
    assert_eq!(p.pem().unwrap(), dec!(10157.37));
    assert_eq!(p.coste_directo().unwrap(), dec!(7005.15));
    // Aplicado al total daría 7.005,15 × 1,45 = 10.157,4675 → 10.157,47: el redondeo por
    // partida explica la diferencia, igual que en Presto.
    let v = p.valorar().unwrap();
    assert_eq!((v.pem, v.coste_directo), (dec!(10157.37), dec!(7005.15)));
    // Recursos y escenarios trabajan sobre coste directo
    let e = p.explosion_recursos().unwrap();
    assert_eq!((e.coste_directo, e.descuadre_redondeo), (dec!(7005.15), dec!(2.10)));
    let [a, _, _] = ofertas_montaje_suelo_radiante();
    let r = simular(&p, &[a]).unwrap();
    assert_eq!((r.coste_original, r.coste_escenario), (dec!(7005.15), dec!(6505.35)));

    // Opción «no aplicar indirectos a partidas sin descomponer»
    let mut q = Presupuesto::new("t", "OBRA", "Obra");
    q.insertar(ppto_core::Concepto::basico("R", "ud", "r", Naturaleza::Otros, dec!(10)))
        .unwrap();
    q.colgar("OBRA", "R", None).unwrap();
    q.costes_indirectos = dec!(45);
    assert_eq!(q.pem().unwrap(), dec!(14.50));
    q.opciones_ci = OpcionesCi {
        aplicar_a_sin_descomponer: false,
        ..Default::default()
    };
    assert_eq!(q.pem().unwrap(), dec!(10.00));
}

#[test]
fn opcion_no_redondear_coste_antes_de_indirectos() {
    use ppto_core::concepto::{Concepto, LineaDescomposicion as L};
    use ppto_core::presupuesto::OpcionesCi;
    // Rendimiento e importe de línea con 4 decimales: 1,0004 × 10,00 = 10,0040.
    // Coste de la partida redondeado a 2 decimales: 10,00.
    let mut p = Presupuesto::new("t", "OBRA", "Obra");
    p.decimales.rendimiento = 4;
    p.decimales.importe_linea = 4;
    p.insertar(Concepto::basico("M", "ud", "m", Naturaleza::Material, dec!(10.00)))
        .unwrap();
    p.insertar(Concepto::partida("P", "ud", "p", vec![L::new("M", dec!(1.0004))]))
        .unwrap();
    p.colgar("OBRA", "P", None).unwrap();
    p.costes_indirectos = dec!(45);
    assert_eq!(p.precio("P").unwrap(), dec!(10.00));
    // Redondeando antes (Presto por defecto): 10,00 × 1,45 = 14,50
    assert_eq!(p.precio_con_indirectos("P").unwrap(), dec!(14.50));
    // Sin redondear antes: 10,0040 × 1,45 = 14,5058 → 14,51
    p.opciones_ci = OpcionesCi {
        redondear_coste_antes: false,
        ..Default::default()
    };
    assert_eq!(p.precio_con_indirectos("P").unwrap(), dec!(14.51));
}
